use std::path::PathBuf;

/// Resolve the Visor config directory using the same precedence as visor-cli:
/// 1. $VISOR_CONFIG_DIR
/// 2. $XDG_CONFIG_HOME/visor
/// 3. ~/.config/visor
pub fn config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("VISOR_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("visor");
    }
    dirs::config_dir()
        .map(|d| d.join("visor"))
        .unwrap_or_else(|| PathBuf::from(".config/visor"))
}

/// Read the default space ID from config.toml, if present.
pub fn default_space_id() -> Option<String> {
    let path = config_dir().join("config.toml");
    let contents = std::fs::read_to_string(&path).ok()?;
    let table: toml::Value = contents.parse().ok()?;
    table
        .get("default_space")
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// Write the default space ID to config.toml.
pub fn set_default_space_id(space_id: &str) -> std::io::Result<()> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("config.toml");
    let contents = format!("default_space = \"{space_id}\"\n");
    std::fs::write(&path, contents)
}

/// Return the active space ID, respecting VISOR_SPACE env override.
pub fn active_space_id() -> Option<String> {
    std::env::var("VISOR_SPACE")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(default_space_id)
}

/// Return the API base URL, respecting VISOR_API_URL env override.
pub fn api_base_url() -> String {
    std::env::var("VISOR_API_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://api.visorfinance.app".to_string())
}

/// Return the OAuth authorization server URL, respecting VISOR_AUTH_URL env override.
pub fn auth_base_url() -> String {
    std::env::var("VISOR_AUTH_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://dashboard.visorfinance.app".to_string())
}

/// Check whether debug mode is enabled via VISOR_DEBUG.
pub fn debug_enabled() -> bool {
    std::env::var("VISOR_DEBUG")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}