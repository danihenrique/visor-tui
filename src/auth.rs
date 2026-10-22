use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::config;

/// Credential stored by visor-cli — tagged union matching its format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Credential {
    #[serde(rename = "personal_token")]
    PersonalToken { token: String },
    #[serde(rename = "oauth")]
    OAuth {
        access_token: String,
        refresh_token: Option<String>,
        expires_at: Option<i64>,
        scope: Option<String>,
    },
}

impl Credential {
    /// Return the bearer token string for API requests.
    pub fn bearer_token(&self) -> &str {
        match self {
            Self::PersonalToken { token } => token,
            Self::OAuth { access_token, .. } => access_token,
        }
    }

    /// Redact the token for safe display.
    pub fn redacted(&self) -> String {
        let raw = self.bearer_token();
        if raw.len() > 12 {
            format!("{}...{}", &raw[..8], &raw[raw.len() - 4..])
        } else {
            "***".to_string()
        }
    }
}

/// Resolve the credential store path (file-based only).
fn credentials_path() -> PathBuf {
    config::config_dir().join("credentials.json")
}

/// Load credential from env override or file store.
/// Keyring support is deferred to a later phase.
pub fn load_credential() -> Option<Credential> {
    // Env override takes precedence
    if let Ok(token) = std::env::var("VISOR_TOKEN") {
        if !token.is_empty() {
            return Some(Credential::PersonalToken { token });
        }
    }

    // File store
    let path = credentials_path();
    let contents = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&contents).ok()
}

/// Save credential to file store with restrictive permissions.
pub fn save_credential(cred: &Credential) -> std::io::Result<()> {
    let dir = config::config_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("credentials.json");
    let json = serde_json::to_string_pretty(cred)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(&path, json)?;

    // Set 0600 on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&path, perms)?;
    }

    Ok(())
}

/// Remove stored credential.
pub fn clear_credential() -> std::io::Result<()> {
    let path = credentials_path();
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}