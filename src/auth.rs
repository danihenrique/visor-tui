use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use crate::config;

// ---------------------------------------------------------------------------
// Keyring constants
// ---------------------------------------------------------------------------

const KEYRING_SERVICE: &str = "visor-cli";
const KEYRING_USER: &str = "default";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// The first-party client ID registered for the Visor CLI / TUI.
pub const CLIENT_ID: &str = "908603ea-465f-4d4c-b5ac-30955885ee1d";

/// Default authorization server base URL.
pub const DEFAULT_AUTH_URL: &str = "https://dashboard.visorfinance.app";

/// How long `login_with_browser` waits for the browser callback (seconds).
pub const LOGIN_TIMEOUT_SECS: u64 = 300;

/// Refresh this many seconds before the access token expires so a request
/// never leaves with a token that dies in transit.
pub const EXPIRY_MARGIN_SECONDS: i64 = 60;

// ---------------------------------------------------------------------------
// Credential
// ---------------------------------------------------------------------------

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
    #[must_use]
    pub fn bearer_token(&self) -> &str {
        match self {
            Self::PersonalToken { token } => token,
            Self::OAuth { access_token, .. } => access_token,
        }
    }

    /// Redact the token for safe display.
    #[must_use]
    pub fn redacted(&self) -> String {
        redact_token(self.bearer_token())
    }

    /// Human-readable credential kind label.
    #[must_use]
    pub fn kind_label(&self) -> &'static str {
        match self {
            Self::PersonalToken { .. } => "Personal Access Token",
            Self::OAuth { .. } => "OAuth",
        }
    }

    /// Formatted expiry string for OAuth tokens, or "N/A" for PATs.
    #[must_use]
    pub fn expiry_display(&self) -> String {
        match self {
            Self::PersonalToken { .. } => "N/A".to_string(),
            Self::OAuth { expires_at, .. } => match expires_at {
                Some(ts) => {
                    let secs: u64 = (*ts).max(0).try_into().unwrap_or(0);
                    let days = secs / 86400;
                    let hours = (secs % 86400) / 3600;
                    let mins = (secs % 3600) / 60;
                    if days > 0 {
                        format!("{days}d {hours}h remaining")
                    } else if hours > 0 {
                        format!("{hours}h {mins}m remaining")
                    } else {
                        format!("{mins}m remaining")
                    }
                }
                None => "No expiry".to_string(),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Credential store
// ---------------------------------------------------------------------------

/// Where credentials are stored.
#[derive(Debug, Clone)]
pub enum CredentialStore {
    Keyring,
    File(PathBuf),
}

/// Resolve which credential store to use based on `VISOR_CREDENTIAL_STORE`.
///
/// - Unset or `"keyring"` → Keyring
/// - `"file"` → `File(config_dir()/credentials.json)`
/// - Other value → `File` with a warning printed to stderr
#[must_use]
pub fn resolve_store() -> CredentialStore {
    match std::env::var("VISOR_CREDENTIAL_STORE").as_deref() {
        Err(_) | Ok("" | "keyring") => CredentialStore::Keyring,
        Ok("file") => CredentialStore::File(credentials_path()),
        Ok(other) => {
            eprintln!(
                "warning: VISOR_CREDENTIAL_STORE={other} is not recognized, falling back to file store"
            );
            CredentialStore::File(credentials_path())
        }
    }
}

/// Resolve the credential file path.
fn credentials_path() -> PathBuf {
    config::config_dir().join("credentials.json")
}

/// Redact a token for safe display.
///
/// If the token starts with `visor_pat_`, show `visor_pat_{prefix}_…` where
/// prefix is the segment between `visor_pat_` and the next underscore (or end).
/// Otherwise, return `…`.
#[must_use]
pub fn redact_token(token: &str) -> String {
    match token.strip_prefix("visor_pat_") {
        Some(rest) => {
            let public_id = rest.split('_').next().unwrap_or_default();
            format!("visor_pat_{public_id}_…")
        }
        None => "…".to_owned(),
    }
}

/// Load credential from env override, keyring, or file store.
///
/// Priority: `VISOR_TOKEN` env var > resolved store (keyring or file).
/// When the store is Keyring and keyring access fails, falls back to file
/// store with a warning rather than crashing.
#[must_use]
pub fn load_credential() -> Option<Credential> {
    // Env override takes precedence
    if let Ok(token) = std::env::var("VISOR_TOKEN")
        && !token.is_empty() {
            return Some(Credential::PersonalToken { token });
        }

    let store = resolve_store();
    load_from_store(&store)
}

fn load_from_store(store: &CredentialStore) -> Option<Credential> {
    match store {
        CredentialStore::Keyring => {
            match keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER) {
                Ok(entry) => match entry.get_password() {
                    Ok(text) => serde_json::from_str(&text).ok(),
                    Err(keyring::Error::NoEntry) => {
                        // No entry in keyring; fall back to file for backward compat
                        load_from_file()
                    }
                    Err(e) => {
                        eprintln!("warning: keyring read failed ({e}), falling back to file store");
                        load_from_file()
                    }
                },
                Err(e) => {
                    eprintln!("warning: keyring unavailable ({e}), falling back to file store");
                    load_from_file()
                }
            }
        }
        CredentialStore::File(_) => load_from_file(),
    }
}

fn load_from_file() -> Option<Credential> {
    let path = credentials_path();
    let contents = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&contents).ok()
}

/// Save credential to the resolved store.
///
/// When the store is Keyring and keyring access fails, falls back to file
/// store with a warning rather than crashing.
pub fn save_credential(cred: &Credential) -> std::io::Result<()> {
    let store = resolve_store();
    save_to_store(&store, cred)
}

fn save_to_store(store: &CredentialStore, cred: &Credential) -> std::io::Result<()> {
    match store {
        CredentialStore::Keyring => {
            let json = serde_json::to_string(cred)
                .map_err(std::io::Error::other)?;
            match keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER) {
                Ok(entry) => match entry.set_password(&json) {
                    Ok(()) => Ok(()),
                    Err(e) => {
                        eprintln!("warning: keyring write failed ({e}), falling back to file store");
                        save_to_file(cred)
                    }
                },
                Err(e) => {
                    eprintln!("warning: keyring unavailable ({e}), falling back to file store");
                    save_to_file(cred)
                }
            }
        }
        CredentialStore::File(_) => save_to_file(cred),
    }
}

fn save_to_file(cred: &Credential) -> std::io::Result<()> {
    let dir = config::config_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("credentials.json");
    let json = serde_json::to_string_pretty(cred)
        .map_err(std::io::Error::other)?;
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

/// Remove stored credential from the resolved store.
///
/// When the store is Keyring and keyring access fails, falls back to file
/// store with a warning rather than crashing.
pub fn clear_credential() -> std::io::Result<()> {
    let store = resolve_store();
    clear_from_store(&store)
}

fn clear_from_store(store: &CredentialStore) -> std::io::Result<()> {
    match store {
        CredentialStore::Keyring => {
            match keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER) {
                Ok(entry) => match entry.delete_credential() {
                    Ok(()) => Ok(()),
                    Err(keyring::Error::NoEntry) => {
                        // Nothing in keyring; also try to clean up file for backward compat
                        clear_from_file()
                    }
                    Err(e) => {
                        eprintln!("warning: keyring delete failed ({e}), falling back to file store");
                        clear_from_file()
                    }
                },
                Err(e) => {
                    eprintln!("warning: keyring unavailable ({e}), falling back to file store");
                    clear_from_file()
                }
            }
        }
        CredentialStore::File(_) => clear_from_file(),
    }
}

fn clear_from_file() -> std::io::Result<()> {
    let path = credentials_path();
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// PKCE (RFC 7636)
// ---------------------------------------------------------------------------

/// The PKCE pair of one login attempt.
#[derive(Debug)]
struct Pkce {
    verifier: String,
    challenge: String,
}

impl Pkce {
    fn new() -> Result<Self, String> {
        Ok(Self::from_verifier(random_token(32)?))
    }

    fn from_verifier(verifier: String) -> Self {
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        Self { verifier, challenge }
    }
}

// ---------------------------------------------------------------------------
// Random token generation
// ---------------------------------------------------------------------------

/// Generate a URL-safe random token of `bytes` random bytes, base64-encoded.
fn random_token(bytes: usize) -> Result<String, String> {
    let mut buffer = vec![0u8; bytes];
    getrandom::fill(&mut buffer)
        .map_err(|e| format!("cannot generate a random value: {e}"))?;
    Ok(URL_SAFE_NO_PAD.encode(buffer))
}

// ---------------------------------------------------------------------------
// Authorize URL builder
// ---------------------------------------------------------------------------

/// Build the OAuth 2.0 authorize URL with PKCE parameters.
fn authorize_url(
    auth_url: &str,
    redirect_uri: &str,
    challenge: &str,
    state: &str,
) -> String {
    let base = auth_url.trim_end_matches('/');
    format!(
        "{base}/oauth/authorize?\
         response_type=code&\
         client_id={client_id}&\
         redirect_uri={redirect_uri}&\
         code_challenge={challenge}&\
         code_challenge_method=S256&\
         state={state}",
        client_id = CLIENT_ID,
        redirect_uri = urlencoded(redirect_uri),
        challenge = urlencoded(challenge),
        state = urlencoded(state),
    )
}

/// Minimal percent-encoding for query parameter values.
#[allow(clippy::format_push_string)]
fn urlencoded(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push_str(&format!("%{b:02X}"));
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Browser login
// ---------------------------------------------------------------------------

/// Run the full OAuth 2.0 browser login flow and return the resulting
/// credential.
///
/// When `open_browser` is true the function attempts to launch the system
/// browser via `xdg-open` (Linux) or `open` (macOS). If the command fails
/// or the platform is unsupported the URL is printed to stderr instead.
pub fn login_with_browser(open_browser: bool) -> Result<Credential, String> {
    login_with_browser_at(DEFAULT_AUTH_URL, open_browser)
}

/// Like [`login_with_browser`] but against an explicit auth server URL.
fn login_with_browser_at(auth_url: &str, open_browser: bool) -> Result<Credential, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("cannot listen for the browser callback: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("cannot read the callback port: {e}"))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");

    let pkce = Pkce::new()?;
    let state = random_token(16)?;
    let url = authorize_url(auth_url, &redirect_uri, &pkce.challenge, &state);

    if open_browser {
        let opened = open_in_browser(&url);
        if opened {
            eprintln!("Opening the Visor login page in your browser:");
        } else {
            eprintln!("Open this page in a browser on this machine and approve the login:");
        }
    } else {
        eprintln!("Open this page in a browser on this machine and approve the login:");
    }
    eprintln!("    {url}");

    let timeout = Duration::from_secs(LOGIN_TIMEOUT_SECS);
    let code = wait_for_code(&listener, &state, timeout)?;

    let tokens = exchange_code(
        auth_url,
        &code,
        &redirect_uri,
        &pkce.verifier,
    )?;
    tokens.into_credential(None)
}

/// Try to open `url` in the system browser. Returns true on success.
fn open_in_browser(url: &str) -> bool {
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .is_ok()
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .is_ok()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = url;
        false
    }
}

// ---------------------------------------------------------------------------
// Callback handling
// ---------------------------------------------------------------------------

/// What the browser sent to the callback endpoint.
#[derive(Debug, PartialEq, Eq)]
enum Callback {
    Code(String),
    Denied(String),
    /// Anything else, such as `/favicon.ico`.
    Other,
}

/// Accept browser requests until one brings the authorization code, an error,
/// or the timeout passes.
fn wait_for_code(
    listener: &TcpListener,
    state: &str,
    timeout: Duration,
) -> Result<String, String> {
    listener
        .set_nonblocking(true)
        .map_err(|e| format!("cannot wait for the browser callback: {e}"))?;
    let deadline = Instant::now() + timeout;

    while Instant::now() < deadline {
        match listener.accept() {
            Ok((stream, _)) => match handle_callback(stream, state)? {
                Callback::Code(code) => return Ok(code),
                Callback::Denied(reason) => return Err(reason),
                Callback::Other => {}
            },
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(format!("cannot accept the browser callback: {e}")),
        }
    }
    Err(format!(
        "nobody approved the login within {} minutes. Try again.",
        timeout.as_secs() / 60
    ))
}

fn handle_callback(mut stream: TcpStream, state: &str) -> Result<Callback, String> {
    let io = |e: std::io::Error| format!("cannot read the browser callback: {e}");
    stream.set_nonblocking(false).map_err(io)?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(io)?;

    let mut request_line = String::new();
    let _ = BufReader::new(&stream)
        .take(8192)
        .read_line(&mut request_line);
    let callback = parse_callback(&request_line, state);

    let (status, title, message) = match &callback {
        Ok(Callback::Code(_)) => (
            "200 OK",
            "You are logged in",
            "You can close this tab and go back to the terminal.",
        ),
        Ok(Callback::Denied(_)) | Err(_) => (
            "400 Bad Request",
            "The login did not finish",
            "Go back to the terminal to see why.",
        ),
        Ok(Callback::Other) => ("404 Not Found", "Not found", ""),
    };
    let page = callback_page(title, message);
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{page}",
        page.len()
    );
    callback
}

/// A minimal HTML page shown in the browser after the redirect.
fn callback_page(title: &str, message: &str) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>Visor - {title}</title><style>\
         :root{{color-scheme:light dark;--bg:#f5f8ff;--card:#fff;--text:#0b1220;--muted:#5b6475}}\
         @media (prefers-color-scheme:dark){{:root{{--bg:#0b1220;--card:#121a2b;--text:#f5f8ff;--muted:#9aa4b8}}}}\
         html,body{{margin:0;height:100%;background:var(--bg);color:var(--text);\
         font-family:system-ui,-apple-system,sans-serif}}\
         main{{min-height:100%;display:grid;place-items:center;padding:16px;box-sizing:border-box}}\
         .card{{background:var(--card);border-radius:24px;padding:40px 32px;max-width:28rem;text-align:center;\
         box-shadow:0 12px 40px rgba(1,71,227,.12)}}\
         h1{{font-size:1.4rem;margin:20px 0 8px}}p{{color:var(--muted);margin:0}}\
         </style><main><div class=\"card\"><h1>{title}</h1><p>{message}</p></div></main></html>"
    )
}

/// Parse the HTTP request line and extract the callback result.
fn parse_callback(request_line: &str, state: &str) -> Result<Callback, String> {
    let mut parts = request_line.split_whitespace();
    let (Some("GET"), Some(target)) = (parts.next(), parts.next()) else {
        return Ok(Callback::Other);
    };

    // Reconstruct a full URL so we can parse query parameters.
    let params = parse_query_params(target);

    if !target.starts_with("/callback") {
        return Ok(Callback::Other);
    }

    let param = |name: &str| -> Option<String> {
        params.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone())
    };

    if param("state").as_deref() != Some(state) {
        return Err(
            "the browser callback did not match this login. Try again.".to_owned(),
        );
    }
    if let Some(error) = param("error") {
        let reason = match error.as_str() {
            "access_denied" => "the login was denied in the browser".to_owned(),
            _ => format!(
                "Visor refused the login: {}",
                param("error_description").unwrap_or(error)
            ),
        };
        return Ok(Callback::Denied(reason));
    }
    match param("code") {
        Some(code) if !code.is_empty() => Ok(Callback::Code(code)),
        _ => Err("the browser callback had no code".to_owned()),
    }
}

/// Very small query-string parser — splits on `&` and `=`, percent-decodes.
fn parse_query_params(path_and_query: &str) -> Vec<(String, String)> {
    let Some(query) = path_and_query.split_once('?').map(|(_, q)| q) else {
        return Vec::new();
    };
    query
        .split('&')
        .filter_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            Some((percent_decode(k), percent_decode(v)))
        })
        .collect()
}

fn percent_decode(s: &str) -> String {
    let mut out = Vec::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len()
            && let Ok(val) = u8::from_str_radix(
                &s[i + 1..i + 3],
                16,
            ) {
                out.push(val);
                i += 3;
                continue;
            }
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ---------------------------------------------------------------------------
// Token exchange
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: i64,
    #[serde(default)]
    scope: Option<String>,
}

impl TokenResponse {
    fn into_credential(self, previous: Option<&Credential>) -> Result<Credential, String> {
        let (old_refresh, old_scope) = match previous {
            Some(Credential::OAuth {
                refresh_token,
                scope,
                ..
            }) => (refresh_token.clone(), scope.clone()),
            _ => (None, None),
        };
        let refresh_token = self
            .refresh_token
            .or(old_refresh)
            .ok_or_else(|| "Visor sent no refresh token".to_owned())?;
        Ok(Credential::OAuth {
            access_token: self.access_token,
            refresh_token: Some(refresh_token),
            expires_at: Some(now() + self.expires_in),
            scope: self.scope.or(old_scope),
        })
    }
}

#[derive(Debug, Deserialize)]
struct TokenError {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Exchange an authorization code for tokens.
fn exchange_code(
    auth_url: &str,
    code: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> Result<TokenResponse, String> {
    let url = format!("{}/oauth/token", auth_url.trim_end_matches('/'));
    let client = reqwest::blocking::Client::new();
    let response = client
        .post(&url)
        .header(reqwest::header::ACCEPT, "application/json")
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", CLIENT_ID),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("code_verifier", code_verifier),
        ])
        .send()
        .map_err(|e| format!("token request failed: {e}"))?;

    let status = response.status();
    let text = response
        .text()
        .map_err(|e| format!("cannot read token response: {e}"))?;

    if status.is_success() {
        return serde_json::from_str(&text)
            .map_err(|e| format!("cannot decode token response: {e}"));
    }
    match serde_json::from_str::<TokenError>(&text) {
        Ok(err) if err.error == "invalid_grant" => Err(
            "Visor did not accept the login code. Try logging in again.".to_owned(),
        ),
        Ok(err) => Err(format!(
            "Visor refused the token request: {}",
            err.error_description.unwrap_or(err.error)
        )),
        Err(_) => Err(format!(
            "Visor answered the token request with HTTP {}",
            status.as_u16()
        )),
    }
}

// ---------------------------------------------------------------------------
// Token refresh
// ---------------------------------------------------------------------------

/// Refresh an expired (or about-to-expire) OAuth credential and return the
/// updated credential.
pub fn refresh_token(auth_url: &str, credential: &Credential) -> Result<Credential, String> {
    let Credential::OAuth {
        refresh_token: Some(rt),
        ..
    } = credential
    else {
        return Err("credential has no refresh token".to_owned());
    };

    let url = format!("{}/oauth/token", auth_url.trim_end_matches('/'));
    let client = reqwest::blocking::Client::new();
    let response = client
        .post(&url)
        .header(reqwest::header::ACCEPT, "application/json")
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", CLIENT_ID),
            ("refresh_token", rt.as_str()),
        ])
        .send()
        .map_err(|e| format!("refresh request failed: {e}"))?;

    let status = response.status();
    let text = response
        .text()
        .map_err(|e| format!("cannot read refresh response: {e}"))?;

    if status.is_success() {
        let tokens: TokenResponse = serde_json::from_str(&text)
            .map_err(|e| format!("cannot decode refresh response: {e}"))?;
        return tokens.into_credential(Some(credential));
    }
    match serde_json::from_str::<TokenError>(&text) {
        Ok(err) if err.error == "invalid_grant" => {
            Err("the refresh token is no longer valid. Please log in again.".to_owned())
        }
        Ok(err) => Err(format!(
            "Visor refused the refresh request: {}",
            err.error_description.unwrap_or(err.error)
        )),
        Err(_) => Err(format!(
            "Visor answered the refresh request with HTTP {}",
            status.as_u16()
        )),
    }
}

// ---------------------------------------------------------------------------
// Expiry check
// ---------------------------------------------------------------------------

/// Returns true when the credential's access token has expired (or is about
/// to expire within the safety margin).
#[must_use]
pub fn is_expired(credential: &Credential) -> bool {
    match credential {
        Credential::PersonalToken { .. } => false,
        Credential::OAuth { expires_at, .. } => match expires_at {
            Some(at) => at - now() <= EXPIRY_MARGIN_SECONDS,
            None => false,
        },
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_matches_rfc_example() {
        let pkce =
            Pkce::from_verifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".into());
        assert_eq!(pkce.challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn random_tokens_unique() {
        let a = random_token(32).unwrap();
        let b = random_token(32).unwrap();
        assert_ne!(a, b);
        assert_eq!(a.len(), 43);
        assert!(a
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn authorize_url_format() {
        let url = authorize_url(
            "https://dash.example",
            "http://127.0.0.1:5000/callback",
            "ch",
            "st",
        );
        assert!(url.contains("/oauth/authorize"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains(&format!("client_id={CLIENT_ID}")));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("state=st"));
        assert!(url.contains("code_challenge=ch"));
    }

    #[test]
    fn parse_callback_code() {
        let ok = parse_callback("GET /callback?code=abc&state=s1 HTTP/1.1\r\n", "s1").unwrap();
        assert_eq!(ok, Callback::Code("abc".into()));

        let denied = parse_callback(
            "GET /callback?error=access_denied&state=s1 HTTP/1.1",
            "s1",
        )
        .unwrap();
        assert!(matches!(denied, Callback::Denied(r) if r.contains("denied")));

        assert_eq!(
            parse_callback("GET /favicon.ico HTTP/1.1", "s1").unwrap(),
            Callback::Other
        );
        assert_eq!(parse_callback("", "s1").unwrap(), Callback::Other);
    }

    #[test]
    fn rejects_callback_with_wrong_state() {
        let err =
            parse_callback("GET /callback?code=abc&state=evil HTTP/1.1", "s1").unwrap_err();
        assert!(err.contains("did not match"), "{err}");
    }

    #[test]
    fn is_expired_checks_margin() {
        let fresh = Credential::OAuth {
            access_token: "a".into(),
            refresh_token: Some("r".into()),
            expires_at: Some(now() + 3600),
            scope: None,
        };
        assert!(!is_expired(&fresh));

        let stale = Credential::OAuth {
            access_token: "a".into(),
            refresh_token: Some("r".into()),
            expires_at: Some(now() - 10),
            scope: None,
        };
        assert!(is_expired(&stale));

        let token = Credential::PersonalToken {
            token: "t".into(),
        };
        assert!(!is_expired(&token));
    }

    #[test]
    fn file_store_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.json");

        // Save
        let cred = Credential::PersonalToken {
            token: "visor_pat_test123_secretvalue".into(),
        };
        let json = serde_json::to_string_pretty(&cred).unwrap();
        std::fs::write(&path, &json).unwrap();

        // Load via file helper (simulate File store)
        let contents = std::fs::read_to_string(&path).unwrap();
        let loaded: Credential = serde_json::from_str(&contents).unwrap();
        assert_eq!(loaded.bearer_token(), "visor_pat_test123_secretvalue");

        // Delete
        std::fs::remove_file(&path).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn credential_serialization() {
        let pat = Credential::PersonalToken {
            token: "tok".into(),
        };
        let json = serde_json::to_string(&pat).unwrap();
        assert!(json.contains(r#""kind":"personal_token""#), "{json}");
        assert!(json.contains(r#""token":"tok""#), "{json}");

        let oauth = Credential::OAuth {
            access_token: "at".into(),
            refresh_token: Some("rt".into()),
            expires_at: Some(1_700_000_000),
            scope: Some("pfm:read".into()),
        };
        let json = serde_json::to_string(&oauth).unwrap();
        assert!(json.contains(r#""kind":"oauth""#), "{json}");
        assert!(json.contains(r#""access_token":"at""#), "{json}");
    }

    #[test]
    fn redact_known_token() {
        assert_eq!(
            redact_token("visor_pat_abcd2345_supersecret"),
            "visor_pat_abcd2345_…"
        );
        assert_eq!(redact_token("visor_pat_xyz"), "visor_pat_xyz_…");
    }

    #[test]
    fn redact_unknown_token() {
        assert_eq!(redact_token("some-other-token"), "…");
        assert_eq!(redact_token(""), "…");
        assert_eq!(redact_token("visor_pat_"), "visor_pat__…");
    }
}