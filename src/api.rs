use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use uuid::Uuid;

use crate::auth::Credential;
use crate::config;
use crate::models::{
    Account, Bill, Card, Category, Connection, Goal, InstallmentPlan, Investment,
    InvestmentSummary, Me, Notification, Overview, RecurringExpense, Space, Tag, Transaction,
};
use crate::period::DateRange;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum TxTypeFilter {
    #[default]
    All,
    Credit,
    Debit,
}

impl TxTypeFilter {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "Todos",
            Self::Credit => "Receita",
            Self::Debit => "Despesa",
        }
    }

    #[must_use]
    pub fn api_value(self) -> Option<&'static str> {
        match self {
            Self::All => None,
            Self::Credit => Some("credit"),
            Self::Debit => Some("debit"),
        }
    }

    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::All => Self::Credit,
            Self::Credit => Self::Debit,
            Self::Debit => Self::All,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum IgnoredFilter {
    #[default]
    All,
    OnlyIgnored,
    NotIgnored,
}

impl IgnoredFilter {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "Todos",
            Self::OnlyIgnored => "Ignorados",
            Self::NotIgnored => "Não ignorados",
        }
    }

    #[must_use]
    pub fn api_value(self) -> Option<bool> {
        match self {
            Self::All => None,
            Self::OnlyIgnored => Some(true),
            Self::NotIgnored => Some(false),
        }
    }

    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::All => Self::OnlyIgnored,
            Self::OnlyIgnored => Self::NotIgnored,
            Self::NotIgnored => Self::All,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct TransactionFilters {
    pub account_id: Option<String>,
    pub category_id: Option<String>,
    pub tag_id: Option<String>,
    pub tx_type: TxTypeFilter,
    pub ignored: IgnoredFilter,
    pub search: String,
}

impl TransactionFilters {
    pub fn normalize(&mut self) {
        self.search = self.search.trim().to_string();
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TransactionQuery {
    pub range: DateRange,
    pub filters: TransactionFilters,
}

#[derive(Clone)]
struct RangeCacheEntry {
    rows: Arc<[Transaction]>,
    fetched_at: Instant,
}

#[derive(Clone)]
struct CacheEntry {
    response: Result<String, CachedError>,
    fetched_at: Instant,
}

#[derive(Clone)]
enum CachedError {
    Api(Box<ApiErrorData>),
    Transport(String),
}

impl CachedError {
    fn from_error(error: &ApiError) -> Self {
        match error {
            ApiError::Api(data) => Self::Api(data.clone()),
            _ => Self::Transport(error.to_string()),
        }
    }

    fn into_error(self) -> ApiError {
        match self {
            Self::Api(data) => ApiError::Api(data),
            Self::Transport(message) => ApiError::Unavailable(message),
        }
    }
}

struct RequestState {
    cache: HashMap<String, CacheEntry>,
    ranges: HashMap<(String, TransactionQuery), RangeCacheEntry>,
    fetch_allowed: bool,
    cooldown_until: Option<Instant>,
    last_sync: Option<chrono::DateTime<chrono::Local>>,
    last_failed: bool,
}

impl Default for RequestState {
    fn default() -> Self {
        Self {
            cache: HashMap::new(),
            ranges: HashMap::new(),
            fetch_allowed: true,
            cooldown_until: None,
            last_sync: None,
            last_failed: false,
        }
    }
}

/// Default cache TTL in seconds.
const CACHE_TTL_SECS: u64 = 60;

/// Maximum retries for idempotent GET requests on 429.
const MAX_RETRIES: u32 = 3;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_RETRY_WAIT: Duration = Duration::from_secs(1);
const MAX_TOTAL_RETRY_WAIT: Duration = Duration::from_secs(3);
const MAX_COOLDOWN: Duration = Duration::from_hours(24);

/// API client that reuses visor-cli's auth and config mechanisms.
pub struct ApiClient {
    client: Client,
    base_url: String,
    credential: Credential,
    space_id: Option<String>,
    requests: Mutex<RequestState>,
}

/// Exit code constants matching the CLI contract.
pub mod exit {
    pub const SUCCESS: u8 = 0;
    pub const FAILURE: u8 = 1;
    pub const USAGE: u8 = 2;
    pub const AUTH: u8 = 3;
    pub const NOT_FOUND: u8 = 4;
    pub const REJECTED: u8 = 5;
    pub const UNAVAILABLE: u8 = 6;
}

/// Inner data for an API error, boxed to keep `ApiError` small.
#[derive(Clone, Debug)]
pub struct ApiErrorData {
    pub status: u16,
    pub code: String,
    pub message: String,
    pub details: serde_json::Value,
    pub request_id: Option<String>,
    pub display: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("{}", .0.display)]
    Api(Box<ApiErrorData>),
    #[error("No credential configured")]
    NoCredential,
    #[error("No active space configured")]
    NoSpace,
    #[error("JSON decode error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Unavailable(String),
}

impl ApiError {
    /// Build an API error from parsed envelope fields, computing the display string.
    fn api(
        status: u16,
        code: String,
        message: String,
        details: serde_json::Value,
        request_id: Option<String>,
    ) -> Self {
        let display = format_api_error_display(&code, &message, &details, request_id.as_deref());
        Self::Api(Box::new(ApiErrorData {
            status,
            code,
            message,
            details,
            request_id,
            display,
        }))
    }

    /// Parse an error body into an `ApiError`. A body outside the standard
    /// envelope still yields an error that carries the HTTP status.
    fn from_body(status: u16, body: &serde_json::Value) -> Self {
        if let Some(e) = body
            .get("errors")
            .and_then(|e| e.as_array())
            .and_then(|arr| arr.first())
        {
            let code = e
                .get("code")
                .and_then(|c| c.as_str())
                .unwrap_or("UNKNOWN")
                .to_string();
            let message = e
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("")
                .to_string();
            let details = e.get("details").cloned().unwrap_or(serde_json::Value::Null);
            let request_id = e
                .get("request_id")
                .and_then(|r| r.as_str())
                .map(str::to_string);
            Self::api(status, code, message, details, request_id)
        } else {
            let code = format!("HTTP_{status}");
            let message = format!("the API answered with HTTP {status}");
            Self::api(status, code, message, serde_json::Value::Null, None)
        }
    }

    /// Returns the exit code for this error, matching the CLI's mapping.
    #[must_use]
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Http(_) | Self::Unavailable(_) => exit::UNAVAILABLE,
            Self::Api(data) => match data.status {
                401 | 403 => exit::AUTH,
                404 => exit::NOT_FOUND,
                400 | 406 | 409 | 422 => exit::REJECTED,
                429 | 500.. => exit::UNAVAILABLE,
                _ => exit::FAILURE,
            },
            Self::NoCredential | Self::NoSpace => exit::AUTH,
            Self::Json(_) => exit::FAILURE,
        }
    }

    /// Returns a user-friendly hint for known error codes, or `None`.
    #[must_use]
    pub fn hint(&self) -> Option<String> {
        if let Self::Api(data) = self {
            return match data.code.as_str() {
                "INVALID_TOKEN" | "UNAUTHENTICATED" | "TOKEN_EXPIRED" | "TOKEN_REVOKED" => {
                    Some("Sessão expirada. Faça login novamente.".to_owned())
                }
                "INSUFFICIENT_SCOPE" => {
                    let scope = data
                        .details
                        .get("required_scope")
                        .and_then(|s| s.as_str())
                        .unwrap_or("outro escopo");
                    Some(format!(
                        "Esta sessão não possui o escopo `{scope}`. Faça login novamente ou crie um token com esse acesso."
                    ))
                }
                "RATE_LIMITED" => match data
                    .details
                    .get("retry_after_seconds")
                    .and_then(serde_json::Value::as_u64)
                {
                    Some(seconds) => Some(format!("Tente novamente em {seconds} segundos.")),
                    None => Some("Tente novamente em um minuto.".to_owned()),
                },
                "IDEMPOTENCY_CONFLICT" => Some("Esta operação já foi realizada.".to_owned()),
                "IMMUTABLE_PROVIDER_FIELD" => Some(
                    "Este campo não pode ser editado (sincronizado automaticamente).".to_owned(),
                ),
                "TRANSACTION_CONFLICT" => {
                    let dep = data
                        .details
                        .get("dependency")
                        .and_then(|d| d.as_str())
                        .or_else(|| data.details.get("reason").and_then(|r| r.as_str()));
                    match dep {
                        Some(info) => Some(format!("Conflito de dependência: {info}")),
                        None => {
                            Some("Conflito de transação. Verifique as dependências.".to_owned())
                        }
                    }
                }
                "PLAN_REQUIRED" => {
                    Some("O plano pessoal requer um plano Pro ou Premium ativo.".to_owned())
                }
                _ => None,
            };
        }
        None
    }

    /// Extract field-level validation errors from `VALIDATION_FAILED` details.
    /// Returns `(field, message)` pairs.
    #[must_use]
    pub fn field_errors(&self) -> Vec<(String, String)> {
        if let Self::Api(data) = self {
            let mut result = Vec::new();
            if let Some(errors) = data.details.get("errors").and_then(|e| e.as_array()) {
                for error in errors {
                    let field = error
                        .get("field")
                        .and_then(|f| f.as_str())
                        .unwrap_or("body")
                        .to_string();
                    let message = error
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("é inválido")
                        .to_string();
                    result.push((field, message));
                }
            }
            return result;
        }
        Vec::new()
    }

    /// Returns the API error code if this is an API error.
    #[must_use]
    pub fn api_code(&self) -> Option<&str> {
        if let Self::Api(data) = self {
            Some(data.code.as_str())
        } else {
            None
        }
    }

    /// Returns the HTTP status code if this is an API error.
    #[must_use]
    pub fn http_status(&self) -> Option<u16> {
        if let Self::Api(data) = self {
            Some(data.status)
        } else {
            None
        }
    }
}

/// Format a user-friendly display string for TUI toast/modal display.
fn format_api_error_display(
    code: &str,
    message: &str,
    details: &serde_json::Value,
    request_id: Option<&str>,
) -> String {
    let mut out = String::new();

    match code {
        "INVALID_TOKEN" | "UNAUTHENTICATED" | "TOKEN_EXPIRED" | "TOKEN_REVOKED" => {
            out.push_str("Sessão expirada. Faça login novamente.");
        }
        "INSUFFICIENT_SCOPE" => {
            let scope = details
                .get("required_scope")
                .and_then(|s| s.as_str())
                .unwrap_or("outro escopo");
            let _ = write!(
                out,
                "Escopo insuficiente: `{scope}` necessário. Faça login novamente."
            );
        }
        "VALIDATION_FAILED" => {
            out.push_str(message);
            if let Some(errors) = details.get("errors").and_then(|e| e.as_array()) {
                for error in errors {
                    let field = error
                        .get("field")
                        .and_then(|f| f.as_str())
                        .unwrap_or("body");
                    let msg = error
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("é inválido");
                    let _ = write!(out, "\n  {field}: {msg}");
                }
            }
        }
        "RATE_LIMITED" => {
            match details
                .get("retry_after_seconds")
                .and_then(serde_json::Value::as_u64)
            {
                Some(seconds) => {
                    let _ = write!(
                        out,
                        "Limite de requisições atingido. Tente novamente em {seconds} segundos."
                    );
                }
                None => {
                    out.push_str("Limite de requisições atingido. Tente novamente em um minuto.");
                }
            }
        }
        "IDEMPOTENCY_CONFLICT" => {
            out.push_str("Esta operação já foi realizada.");
        }
        "IMMUTABLE_PROVIDER_FIELD" => {
            out.push_str("Este campo não pode ser editado (sincronizado automaticamente).");
        }
        "TRANSACTION_CONFLICT" => {
            let dep = details
                .get("dependency")
                .and_then(|d| d.as_str())
                .or_else(|| details.get("reason").and_then(|r| r.as_str()));
            match dep {
                Some(info) => {
                    let _ = write!(out, "Conflito de transação: {info}");
                }
                None => {
                    out.push_str("Conflito de transação. Verifique as dependências.");
                }
            }
        }
        "NOT_FOUND" => {
            out.push_str("Recurso não encontrado.");
        }
        "BAD_REQUEST" => {
            out.push_str(message);
            for key in ["field", "parameter", "reason"] {
                if let Some(value) = details.get(key).and_then(|v| v.as_str()) {
                    let _ = write!(out, "\n  {key}: {value}");
                }
            }
        }
        _ => {
            out.push_str(message);
        }
    }

    if let Some(rid) = request_id {
        let _ = write!(out, "\nRequest ID: {rid}");
    }

    out
}

impl ApiClient {
    /// Create a new API client from stored/env credential and config.
    pub fn new() -> Result<Self, ApiError> {
        let credential = crate::auth::load_credential().ok_or(ApiError::NoCredential)?;
        Self::from_credential(credential)
    }

    /// Create a new API client with an explicit credential (no env lookup).
    pub fn with_credential(credential: crate::auth::Credential) -> Result<Self, ApiError> {
        Self::from_credential(credential)
    }

    pub fn with_space(credential: Credential, space_id: &str) -> Result<Self, ApiError> {
        let mut client = Self::from_credential(credential)?;
        client.space_id = Some(space_id.to_owned());
        Ok(client)
    }

    #[must_use]
    pub fn space_id(&self) -> Option<&str> {
        self.space_id.as_deref()
    }

    /// Return a reference to the current credential (for display purposes).
    #[must_use]
    pub fn credential(&self) -> &Credential {
        &self.credential
    }

    fn from_credential(credential: crate::auth::Credential) -> Result<Self, ApiError> {
        let space_id = config::active_space_id();
        let base_url = config::api_base_url();

        let client = Client::builder()
            .user_agent(format!("visor-tui/{}", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(3))
            .timeout(REQUEST_TIMEOUT)
            .build()?;

        Ok(Self {
            client,
            base_url,
            credential,
            space_id,
            requests: Mutex::new(RequestState::default()),
        })
    }

    /// Create a test client with explicit base URL, credential, and space ID.
    #[cfg(test)]
    pub(crate) fn for_test(
        base_url: &str,
        credential: Credential,
        space_id: &str,
    ) -> Result<Self, ApiError> {
        let client = Client::builder()
            .user_agent(format!("visor-tui/{}", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(3))
            .timeout(REQUEST_TIMEOUT)
            .build()?;

        Ok(Self {
            client,
            base_url: base_url.to_string(),
            credential,
            space_id: Some(space_id.to_string()),
            requests: Mutex::new(RequestState::default()),
        })
    }

    /// Control network reads during screen entry and explicit user actions.
    pub fn set_fetch_allowed(&self, allowed: bool) {
        self.requests.lock().unwrap().fetch_allowed = allowed;
    }

    /// Force the next allowed read to fetch fresh data. Rate limits still apply.
    pub fn invalidate_cache(&self) {
        let mut state = self.requests.lock().unwrap();
        state.cache.clear();
        state.ranges.clear();
    }

    pub fn invalidate_all_cache(&self) {
        self.invalidate_cache();
    }

    #[must_use]
    pub fn request_status(&self) -> String {
        let state = self.requests.lock().unwrap();
        let sync = state.last_sync.map_or_else(
            || "sem sincronização".to_owned(),
            |at| format!("sync {}", at.format("%H:%M:%S")),
        );
        let activity = if let Some(remaining) = state
            .cooldown_until
            .and_then(|until| until.checked_duration_since(Instant::now()))
        {
            format!("limite {}s", remaining.as_secs().saturating_add(1))
        } else if state.last_failed {
            "falha · r tentar novamente".to_owned()
        } else {
            format!("cache {}", state.cache.len())
        };
        format!("{sync} · {activity}")
    }

    fn backoff_duration(attempt: u32, retry_after: Option<Duration>) -> Duration {
        retry_after.unwrap_or_else(|| {
            let base_ms = 250u64.saturating_mul(2u64.saturating_pow(attempt));
            Duration::from_millis(base_ms.min(1_000))
        })
    }

    fn retry_after(header: Option<&str>, body: &serde_json::Value) -> Option<Duration> {
        let header_delay = header.and_then(|value| {
            value
                .trim()
                .parse::<u64>()
                .ok()
                .map(Duration::from_secs)
                .or_else(|| {
                    chrono::DateTime::parse_from_rfc2822(value).ok().map(|at| {
                        at.signed_duration_since(chrono::Utc::now())
                            .to_std()
                            .unwrap_or_default()
                    })
                })
        });
        header_delay.or_else(|| {
            body.pointer("/errors/0/details/retry_after_seconds")
                .and_then(serde_json::Value::as_u64)
                .map(Duration::from_secs)
        })
    }

    fn set_cooldown(&self, delay: Duration) {
        self.requests.lock().unwrap().cooldown_until =
            Some(Instant::now() + delay.min(MAX_COOLDOWN));
    }

    fn cooldown_error(remaining: Duration) -> ApiError {
        ApiError::api(
            429,
            "RATE_LIMITED".to_owned(),
            "Limite de requisições".to_owned(),
            serde_json::json!({"retry_after_seconds": remaining.as_secs().saturating_add(1)}),
            None,
        )
    }

    fn check_cooldown(&self) -> Result<(), ApiError> {
        let state = self.requests.lock().unwrap();
        if let Some(remaining) = state
            .cooldown_until
            .and_then(|until| until.checked_duration_since(Instant::now()))
        {
            return Err(Self::cooldown_error(remaining));
        }
        Ok(())
    }

    fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiError> {
        let text = self.get_response(path, "application/json")?;
        serde_json::from_str(&text).map_err(|error| {
            self.requests.lock().unwrap().last_failed = true;
            ApiError::Json(error)
        })
    }

    fn get_response(&self, path: &str, accept: &str) -> Result<String, ApiError> {
        let key = format!(
            "{}:{accept}:{path}",
            self.space_id.as_deref().unwrap_or("_")
        );
        {
            let state = self.requests.lock().unwrap();
            if let Some(entry) = state.cache.get(&key)
                && (!state.fetch_allowed
                    || entry.fetched_at.elapsed() < Duration::from_secs(CACHE_TTL_SECS))
            {
                return entry.response.clone().map_err(CachedError::into_error);
            }
            if !state.fetch_allowed {
                return Err(ApiError::Unavailable(
                    "Sem dados em cache. Pressione r para atualizar.".to_owned(),
                ));
            }
        }
        self.check_cooldown()?;

        let response = self.fetch_response(path, accept);
        let cached = response
            .as_ref()
            .map(Clone::clone)
            .map_err(CachedError::from_error);
        let mut state = self.requests.lock().unwrap();
        state.last_failed = response.is_err();
        if response.is_ok() {
            state.last_sync = Some(chrono::Local::now());
        }
        state.cache.insert(
            key,
            CacheEntry {
                response: cached,
                fetched_at: Instant::now(),
            },
        );
        response
    }

    fn fetch_response(&self, path: &str, accept: &str) -> Result<String, ApiError> {
        let url = format!("{}{}", self.base_url, path);
        let started = Instant::now();
        let mut total_wait = Duration::ZERO;
        for attempt in 0..=MAX_RETRIES {
            let remaining = REQUEST_TIMEOUT.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return Err(ApiError::Unavailable(
                    "A requisição excedeu o tempo limite. Pressione r para tentar novamente."
                        .to_owned(),
                ));
            }
            let response = self
                .client
                .get(&url)
                .header(
                    "Authorization",
                    format!("Bearer {}", self.credential.bearer_token()),
                )
                .header("Accept", accept)
                .timeout(remaining)
                .send();
            let (error, wait) = match response {
                Ok(response) if response.status().is_success() => return Ok(response.text()?),
                Ok(response) => {
                    let status = response.status().as_u16();
                    let retry_header = response
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_owned);
                    let body = response.json().unwrap_or_default();
                    let error = ApiError::from_body(status, &body);
                    if status != 429 {
                        return Err(error);
                    }
                    let wait = Self::backoff_duration(
                        attempt,
                        Self::retry_after(retry_header.as_deref(), &body),
                    );
                    self.set_cooldown(wait);
                    (error, wait)
                }
                Err(error) => {
                    let retryable = error.is_connect() || error.is_timeout();
                    let error = ApiError::Http(error);
                    if !retryable {
                        return Err(error);
                    }
                    (error, Self::backoff_duration(attempt, None))
                }
            };
            if attempt == MAX_RETRIES
                || wait > MAX_RETRY_WAIT
                || total_wait + wait > MAX_TOTAL_RETRY_WAIT
                || started.elapsed() + wait >= REQUEST_TIMEOUT
            {
                return Err(error);
            }
            std::thread::sleep(wait);
            total_wait += wait;
        }
        unreachable!("each final attempt returns its response or error")
    }

    fn query_string(params: &[(&str, String)]) -> String {
        let mut url = reqwest::Url::parse("http://localhost").unwrap();
        url.query_pairs_mut()
            .extend_pairs(params.iter().map(|(key, value)| (*key, value)));
        url.query()
            .filter(|query| !query.is_empty())
            .map_or_else(String::new, |query| format!("?{query}"))
    }

    fn space_prefix(&self) -> Result<String, ApiError> {
        let sid = self.space_id.as_deref().ok_or(ApiError::NoSpace)?;
        Ok(format!("/v1/spaces/{sid}"))
    }

    // ── User & Global ──

    pub fn me(&self) -> Result<Me, ApiError> {
        let resp: serde_json::Value = self.get("/v1/me")?;
        let data = resp.get("data").cloned().unwrap_or(resp);
        Ok(serde_json::from_value(data)?)
    }

    pub fn spaces(&self) -> Result<Vec<Space>, ApiError> {
        let resp: serde_json::Value = self.get("/v1/spaces")?;
        let data = resp.get("data").cloned().unwrap_or_default();
        Ok(serde_json::from_value(data)?)
    }

    pub fn notifications(
        &self,
        unread: Option<bool>,
        limit: Option<u32>,
    ) -> Result<Vec<Notification>, ApiError> {
        let mut params = Vec::new();
        if let Some(u) = unread {
            params.push(("unread", u.to_string()));
        }
        if let Some(l) = limit {
            params.push(("limit", l.to_string()));
        }
        let qs = Self::query_string(&params);
        let resp: serde_json::Value = self.get(&format!("/v1/notifications{qs}"))?;
        let data = resp.get("data").cloned().unwrap_or_default();
        Ok(serde_json::from_value(data)?)
    }

    pub fn mark_notification_read(&self, id: &str) -> Result<(), ApiError> {
        let body = serde_json::json!({ "read": true });
        let _ = self.patch(&format!("/v1/notifications/{id}"), &body)?;
        Ok(())
    }

    // ── Space-Scoped Reads ──

    pub fn accounts(&self, include_hidden: bool) -> Result<Vec<Account>, ApiError> {
        let prefix = self.space_prefix()?;
        let qs = if include_hidden {
            "?include_hidden=true"
        } else {
            ""
        };
        let resp: serde_json::Value = self.get(&format!("{prefix}/accounts{qs}"))?;
        let data = resp.get("data").cloned().unwrap_or_default();
        Ok(serde_json::from_value(data)?)
    }

    pub fn cards(&self) -> Result<Vec<Card>, ApiError> {
        let prefix = self.space_prefix()?;
        let resp: serde_json::Value = self.get(&format!("{prefix}/cards"))?;
        let data = resp.get("data").cloned().unwrap_or_default();
        Ok(serde_json::from_value(data)?)
    }

    pub fn bills(
        &self,
        card_id: &str,
        from: Option<&str>,
        to: Option<&str>,
    ) -> Result<Vec<Bill>, ApiError> {
        let prefix = self.space_prefix()?;
        let mut params = Vec::new();
        if let Some(f) = from {
            params.push(("from", f.to_string()));
        }
        if let Some(t) = to {
            params.push(("to", t.to_string()));
        }
        let qs = Self::query_string(&params);
        let resp: serde_json::Value = self.get(&format!("{prefix}/cards/{card_id}/bills{qs}"))?;
        let data = resp.get("data").cloned().unwrap_or_default();
        Ok(serde_json::from_value(data)?)
    }

    pub fn categories(&self, include_hidden: bool) -> Result<Vec<Category>, ApiError> {
        let prefix = self.space_prefix()?;
        let qs = if include_hidden {
            "?include_hidden=true"
        } else {
            ""
        };
        let resp: serde_json::Value = self.get(&format!("{prefix}/categories{qs}"))?;
        let data = resp.get("data").cloned().unwrap_or_default();
        Ok(serde_json::from_value(data)?)
    }

    pub fn tags(&self) -> Result<Vec<Tag>, ApiError> {
        let prefix = self.space_prefix()?;
        let resp: serde_json::Value = self.get(&format!("{prefix}/tags"))?;
        let data = resp.get("data").cloned().unwrap_or_default();
        Ok(serde_json::from_value(data)?)
    }

    pub fn goals(&self, status: Option<&str>) -> Result<Vec<Goal>, ApiError> {
        let prefix = self.space_prefix()?;
        let qs = status.map_or_else(String::new, |status| {
            Self::query_string(&[("status", status.to_owned())])
        });
        let resp: serde_json::Value = self.get(&format!("{prefix}/goals{qs}"))?;
        let data = resp.get("data").cloned().unwrap_or_default();
        Ok(serde_json::from_value(data)?)
    }

    pub fn investments(&self) -> Result<(Vec<Investment>, Option<InvestmentSummary>), ApiError> {
        let prefix = self.space_prefix()?;
        let resp: serde_json::Value = self.get(&format!("{prefix}/investments"))?;
        let data = resp.get("data").cloned().unwrap_or_default();
        let summary = resp
            .get("summary")
            .cloned()
            .and_then(|s| serde_json::from_value(s).ok());
        Ok((serde_json::from_value(data)?, summary))
    }

    pub fn subscriptions(&self) -> Result<Vec<RecurringExpense>, ApiError> {
        let prefix = self.space_prefix()?;
        let resp: serde_json::Value = self.get(&format!("{prefix}/subscriptions"))?;
        let data = resp.get("data").cloned().unwrap_or_default();
        Ok(serde_json::from_value(data)?)
    }

    pub fn fixed_bills(&self) -> Result<Vec<RecurringExpense>, ApiError> {
        let prefix = self.space_prefix()?;
        let resp: serde_json::Value = self.get(&format!("{prefix}/fixed-bills"))?;
        let data = resp.get("data").cloned().unwrap_or_default();
        Ok(serde_json::from_value(data)?)
    }

    pub fn installment_plans(
        &self,
        status: Option<&str>,
    ) -> Result<Vec<InstallmentPlan>, ApiError> {
        let prefix = self.space_prefix()?;
        let qs = status.map_or_else(String::new, |status| {
            Self::query_string(&[("status", status.to_owned())])
        });
        let resp: serde_json::Value = self.get(&format!("{prefix}/installment-plans{qs}"))?;
        let data = resp.get("data").cloned().unwrap_or_default();
        Ok(serde_json::from_value(data)?)
    }

    pub fn connections(&self) -> Result<Vec<Connection>, ApiError> {
        let prefix = self.space_prefix()?;
        let resp: serde_json::Value = self.get(&format!("{prefix}/connections"))?;
        let data = resp.get("data").cloned().unwrap_or_default();
        Ok(serde_json::from_value(data)?)
    }

    pub fn overview(&self) -> Result<Overview, ApiError> {
        let prefix = self.space_prefix()?;
        let resp: serde_json::Value = self.get(&format!("{prefix}/overview"))?;
        let data = resp.get("data").cloned().unwrap_or(resp);
        Ok(serde_json::from_value(data)?)
    }

    pub fn transactions_in_range(
        &self,
        query: &TransactionQuery,
    ) -> Result<Arc<[Transaction]>, ApiError> {
        let space = self.space_id.as_ref().ok_or(ApiError::NoSpace)?;
        let key = (space.clone(), query.clone());
        {
            let state = self.requests.lock().unwrap();
            if let Some(entry) = state.ranges.get(&key)
                && (!state.fetch_allowed
                    || entry.fetched_at.elapsed() < Duration::from_secs(CACHE_TTL_SECS))
            {
                return Ok(Arc::clone(&entry.rows));
            }
            if !state.fetch_allowed {
                return Err(ApiError::Unavailable(
                    "Período não carregado. Pressione r para atualizar.".into(),
                ));
            }
        }
        let result = self.fetch_transaction_range(query);
        let mut state = self.requests.lock().unwrap();
        state.last_failed = result.is_err();
        if let Ok(rows) = &result {
            state.last_sync = Some(chrono::Local::now());
            state.ranges.insert(
                key,
                RangeCacheEntry {
                    rows: Arc::clone(rows),
                    fetched_at: Instant::now(),
                },
            );
        }
        result
    }

    fn fetch_transaction_range(
        &self,
        query: &TransactionQuery,
    ) -> Result<Arc<[Transaction]>, ApiError> {
        let prefix = self.space_prefix()?;
        let mut rows = Vec::new();
        let mut ids = HashSet::new();
        let mut cursors = HashSet::new();
        let mut cursor: Option<String> = None;
        loop {
            self.check_cooldown()?;
            let filters = &query.filters;
            let mut params = vec![
                ("from_date", query.range.start().to_string()),
                ("to_date", query.range.end().to_string()),
                ("limit", "100".into()),
            ];
            for (name, value) in [
                ("account_id", filters.account_id.as_deref()),
                ("category_id", filters.category_id.as_deref()),
                ("tag_id", filters.tag_id.as_deref()),
                ("type", filters.tx_type.api_value()),
                (
                    "search",
                    (!filters.search.is_empty()).then_some(filters.search.as_str()),
                ),
                ("after", cursor.as_deref()),
            ] {
                if let Some(value) = value {
                    params.push((name, value.to_owned()));
                }
            }
            if let Some(ignored) = filters.ignored.api_value() {
                params.push(("ignored", ignored.to_string()));
            }
            let path = format!("{prefix}/transactions{}", Self::query_string(&params));
            let text = self.fetch_response(&path, "application/json")?;
            let page: crate::models::ListResponse<Transaction> = serde_json::from_str(&text)?;
            for row in page.data {
                if ids.insert(row.id.clone()) {
                    rows.push(row);
                }
            }
            match page.next_cursor.filter(|cursor| !cursor.is_empty()) {
                None => return Ok(rows.into()),
                Some(next) => {
                    if !cursors.insert(next.clone()) {
                        return Err(ApiError::Unavailable(
                            "Paginação repetida. Não foi possível carregar o período completo."
                                .into(),
                        ));
                    }
                    cursor = Some(next);
                }
            }
        }
    }

    pub fn transaction(&self, id: &str) -> Result<Transaction, ApiError> {
        let prefix = self.space_prefix()?;
        let resp: serde_json::Value = self.get(&format!("{prefix}/transactions/{id}"))?;
        let data = resp.get("data").cloned().unwrap_or(resp);
        Ok(serde_json::from_value(data)?)
    }

    // ── Private HTTP helpers ──

    fn post(
        &self,
        path: &str,
        body: &serde_json::Value,
        idempotency_key: Option<&str>,
    ) -> Result<serde_json::Value, ApiError> {
        self.check_cooldown()?;
        let url = format!("{}{}", self.base_url, path);
        let mut req = self
            .client
            .post(&url)
            .header(
                "Authorization",
                format!("Bearer {}", self.credential.bearer_token()),
            )
            .header("Accept", "application/json")
            .json(body);
        if let Some(key) = idempotency_key {
            req = req.header("Idempotency-Key", key);
        }
        let resp = req.send()?;

        self.write_response(resp)
    }

    fn patch(&self, path: &str, body: &serde_json::Value) -> Result<serde_json::Value, ApiError> {
        self.check_cooldown()?;
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .patch(&url)
            .header(
                "Authorization",
                format!("Bearer {}", self.credential.bearer_token()),
            )
            .header("Accept", "application/json")
            .json(body)
            .send()?;

        self.write_response(resp)
    }

    fn delete(&self, path: &str) -> Result<(), ApiError> {
        self.check_cooldown()?;
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .delete(&url)
            .header(
                "Authorization",
                format!("Bearer {}", self.credential.bearer_token()),
            )
            .header("Accept", "application/json")
            .send()?;

        self.write_response(resp).map(|_| ())
    }

    fn write_response(
        &self,
        response: reqwest::blocking::Response,
    ) -> Result<serde_json::Value, ApiError> {
        let status = response.status().as_u16();
        if !response.status().is_success() {
            let retry_header = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let body = response.json().unwrap_or_default();
            if status == 429 {
                self.set_cooldown(
                    Self::retry_after(retry_header.as_deref(), &body)
                        .unwrap_or(Duration::from_secs(60)),
                );
            }
            self.requests.lock().unwrap().last_failed = true;
            return Err(ApiError::from_body(status, &body));
        }
        // A successful mutation changes aggregates and filtered lists even if its body cannot be decoded.
        self.invalidate_cache();
        {
            let mut state = self.requests.lock().unwrap();
            state.last_sync = Some(chrono::Local::now());
            state.last_failed = false;
        }
        let text = response.text()?;
        if text.trim().is_empty() {
            Ok(serde_json::Value::Null)
        } else {
            Ok(serde_json::from_str(&text)?)
        }
    }

    // ── Transaction Mutations ──

    pub fn create_transaction(
        &self,
        amount: &str,
        description: &str,
        tx_type: &str,
        account_id: &str,
        category_id: Option<&str>,
        tag_ids: &[String],
        merchant_name: Option<&str>,
        notes: Option<&str>,
        transacted_at: Option<&str>,
        ignored: bool,
    ) -> Result<Transaction, ApiError> {
        let idempotency_key = Uuid::new_v4().to_string();

        let mut body = serde_json::Map::new();
        body.insert(
            "amount".into(),
            serde_json::Value::String(amount.to_string()),
        );
        body.insert(
            "description".into(),
            serde_json::Value::String(description.to_string()),
        );
        body.insert(
            "type".into(),
            serde_json::Value::String(tx_type.to_string()),
        );
        body.insert(
            "account_id".into(),
            serde_json::Value::String(account_id.to_string()),
        );
        if let Some(cat) = category_id {
            body.insert(
                "category_id".into(),
                serde_json::Value::String(cat.to_string()),
            );
        }
        if !tag_ids.is_empty() {
            body.insert(
                "tag_ids".into(),
                serde_json::Value::Array(
                    tag_ids
                        .iter()
                        .map(|t| serde_json::Value::String(t.clone()))
                        .collect(),
                ),
            );
        }
        if let Some(merchant) = merchant_name {
            body.insert(
                "merchant_name".into(),
                serde_json::Value::String(merchant.to_string()),
            );
        }
        if let Some(n) = notes {
            body.insert("notes".into(), serde_json::Value::String(n.to_string()));
        }
        if let Some(at) = transacted_at {
            body.insert(
                "transacted_at".into(),
                serde_json::Value::String(at.to_string()),
            );
        }
        if ignored {
            body.insert("ignored".into(), serde_json::Value::Bool(true));
        }

        self.create_transaction_payload(&serde_json::Value::Object(body), &idempotency_key)
    }

    /// Reuse the draft's key when retrying after an uncertain create response.
    pub fn create_transaction_payload(
        &self,
        body: &serde_json::Value,
        idempotency_key: &str,
    ) -> Result<Transaction, ApiError> {
        let prefix = self.space_prefix()?;
        let resp = self.post(
            &format!("{prefix}/transactions"),
            body,
            Some(idempotency_key),
        )?;
        let data = resp.get("data").cloned().unwrap_or(resp);
        Ok(serde_json::from_value(data)?)
    }

    pub fn update_transaction(
        &self,
        id: &str,
        amount: Option<&str>,
        description: Option<&str>,
        tx_type: Option<&str>,
        category_id: Option<&str>,
        tag_ids: Option<&[String]>,
        merchant_name: Option<&str>,
        notes: Option<&str>,
        transacted_at: Option<&str>,
        ignored: Option<bool>,
        clear_tags: bool,
    ) -> Result<Transaction, ApiError> {
        let mut body = serde_json::Map::new();

        if let Some(v) = amount {
            body.insert("amount".into(), serde_json::Value::String(v.to_string()));
        }
        if let Some(v) = description {
            body.insert(
                "description".into(),
                serde_json::Value::String(v.to_string()),
            );
        }
        if let Some(v) = tx_type {
            body.insert("type".into(), serde_json::Value::String(v.to_string()));
        }
        if let Some(v) = category_id {
            body.insert(
                "category_id".into(),
                serde_json::Value::String(v.to_string()),
            );
        }
        if clear_tags {
            body.insert("tag_ids".into(), serde_json::Value::Array(vec![]));
        } else if let Some(tags) = tag_ids {
            body.insert(
                "tag_ids".into(),
                serde_json::Value::Array(
                    tags.iter()
                        .map(|t| serde_json::Value::String(t.clone()))
                        .collect(),
                ),
            );
        }
        if let Some(v) = merchant_name {
            body.insert(
                "merchant_name".into(),
                serde_json::Value::String(v.to_string()),
            );
        }
        if let Some(v) = notes {
            body.insert("notes".into(), serde_json::Value::String(v.to_string()));
        }
        if let Some(v) = transacted_at {
            body.insert(
                "transacted_at".into(),
                serde_json::Value::String(v.to_string()),
            );
        }
        if let Some(v) = ignored {
            body.insert("ignored".into(), serde_json::Value::Bool(v));
        }

        self.patch_transaction(id, &serde_json::Value::Object(body))
    }

    pub fn patch_transaction(
        &self,
        id: &str,
        body: &serde_json::Value,
    ) -> Result<Transaction, ApiError> {
        let prefix = self.space_prefix()?;
        let resp = self.patch(&format!("{prefix}/transactions/{id}"), body)?;
        let data = resp.get("data").cloned().unwrap_or(resp);
        Ok(serde_json::from_value(data)?)
    }

    pub fn delete_transaction(&self, id: &str) -> Result<(), ApiError> {
        let prefix = self.space_prefix()?;
        self.delete(&format!("{prefix}/transactions/{id}"))?;
        Ok(())
    }

    // ── Tag Mutations ──

    pub fn create_tag(&self, name: &str, color: Option<&str>) -> Result<Tag, ApiError> {
        let prefix = self.space_prefix()?;
        let idempotency_key = Uuid::new_v4().to_string();

        let mut body = serde_json::Map::new();
        body.insert("name".into(), serde_json::Value::String(name.to_string()));
        if let Some(c) = color {
            body.insert("color".into(), serde_json::Value::String(c.to_string()));
        }

        let resp = self.post(
            &format!("{prefix}/tags"),
            &serde_json::Value::Object(body),
            Some(&idempotency_key),
        )?;
        let data = resp.get("data").cloned().unwrap_or(resp);
        Ok(serde_json::from_value(data)?)
    }

    pub fn update_tag(
        &self,
        id: &str,
        name: Option<&str>,
        color: Option<&str>,
    ) -> Result<Tag, ApiError> {
        let prefix = self.space_prefix()?;
        let mut body = serde_json::Map::new();

        if let Some(n) = name {
            body.insert("name".into(), serde_json::Value::String(n.to_string()));
        }
        if let Some(c) = color {
            body.insert("color".into(), serde_json::Value::String(c.to_string()));
        }

        let resp = self.patch(
            &format!("{prefix}/tags/{id}"),
            &serde_json::Value::Object(body),
        )?;
        let data = resp.get("data").cloned().unwrap_or(resp);
        Ok(serde_json::from_value(data)?)
    }

    pub fn delete_tag(&self, id: &str) -> Result<(), ApiError> {
        let prefix = self.space_prefix()?;
        self.delete(&format!("{prefix}/tags/{id}"))?;
        Ok(())
    }

    // ── Category Mutations ──

    pub fn create_category(
        &self,
        name: &str,
        category_type: &str,
        parent_id: Option<&str>,
        kind: Option<&str>,
    ) -> Result<Category, ApiError> {
        let prefix = self.space_prefix()?;
        let idempotency_key = Uuid::new_v4().to_string();

        let mut body = serde_json::Map::new();
        body.insert("name".into(), serde_json::Value::String(name.to_string()));
        body.insert(
            "type".into(),
            serde_json::Value::String(category_type.to_string()),
        );
        if let Some(pid) = parent_id {
            body.insert(
                "parent_id".into(),
                serde_json::Value::String(pid.to_string()),
            );
        }
        if let Some(k) = kind {
            body.insert("kind".into(), serde_json::Value::String(k.to_string()));
        }

        let resp = self.post(
            &format!("{prefix}/categories"),
            &serde_json::Value::Object(body),
            Some(&idempotency_key),
        )?;
        let data = resp.get("data").cloned().unwrap_or(resp);
        Ok(serde_json::from_value(data)?)
    }

    pub fn update_category(
        &self,
        id: &str,
        name: Option<&str>,
        parent_id: Option<&str>,
        hidden: Option<bool>,
    ) -> Result<Category, ApiError> {
        let prefix = self.space_prefix()?;
        let mut body = serde_json::Map::new();

        if let Some(n) = name {
            body.insert("name".into(), serde_json::Value::String(n.to_string()));
        }
        if let Some(pid) = parent_id {
            body.insert(
                "parent_id".into(),
                serde_json::Value::String(pid.to_string()),
            );
        }
        if let Some(h) = hidden {
            body.insert("hidden".into(), serde_json::Value::Bool(h));
        }

        let resp = self.patch(
            &format!("{prefix}/categories/{id}"),
            &serde_json::Value::Object(body),
        )?;
        let data = resp.get("data").cloned().unwrap_or(resp);
        Ok(serde_json::from_value(data)?)
    }

    pub fn delete_category(&self, id: &str) -> Result<(), ApiError> {
        let prefix = self.space_prefix()?;
        self.delete(&format!("{prefix}/categories/{id}"))?;
        Ok(())
    }

    // ── Goal Mutations ──

    pub fn create_goal(
        &self,
        name: &str,
        target_amount: &str,
        kind: Option<&str>,
        target_date: Option<&str>,
        currency_code: Option<&str>,
    ) -> Result<Goal, ApiError> {
        let prefix = self.space_prefix()?;
        let idempotency_key = Uuid::new_v4().to_string();

        let mut body = serde_json::Map::new();
        body.insert("name".into(), serde_json::Value::String(name.to_string()));
        body.insert(
            "target_amount".into(),
            serde_json::Value::String(target_amount.to_string()),
        );
        if let Some(k) = kind {
            body.insert("kind".into(), serde_json::Value::String(k.to_string()));
        }
        if let Some(d) = target_date {
            body.insert(
                "target_date".into(),
                serde_json::Value::String(d.to_string()),
            );
        }
        if let Some(c) = currency_code {
            body.insert(
                "currency_code".into(),
                serde_json::Value::String(c.to_string()),
            );
        }

        let resp = self.post(
            &format!("{prefix}/goals"),
            &serde_json::Value::Object(body),
            Some(&idempotency_key),
        )?;
        let data = resp.get("data").cloned().unwrap_or(resp);
        Ok(serde_json::from_value(data)?)
    }

    pub fn update_goal(
        &self,
        id: &str,
        name: Option<&str>,
        target_amount: Option<&str>,
        status: Option<&str>,
        target_date: Option<&str>,
    ) -> Result<Goal, ApiError> {
        let prefix = self.space_prefix()?;
        let mut body = serde_json::Map::new();

        if let Some(n) = name {
            body.insert("name".into(), serde_json::Value::String(n.to_string()));
        }
        if let Some(t) = target_amount {
            body.insert(
                "target_amount".into(),
                serde_json::Value::String(t.to_string()),
            );
        }
        if let Some(s) = status {
            body.insert("status".into(), serde_json::Value::String(s.to_string()));
        }
        if let Some(d) = target_date {
            body.insert(
                "target_date".into(),
                serde_json::Value::String(d.to_string()),
            );
        }

        let resp = self.patch(
            &format!("{prefix}/goals/{id}"),
            &serde_json::Value::Object(body),
        )?;
        let data = resp.get("data").cloned().unwrap_or(resp);
        Ok(serde_json::from_value(data)?)
    }

    pub fn delete_goal(&self, id: &str) -> Result<(), ApiError> {
        let prefix = self.space_prefix()?;
        self.delete(&format!("{prefix}/goals/{id}"))?;
        Ok(())
    }

    // ── Subscription Mutations ──

    pub fn create_subscription(
        &self,
        name: &str,
        amount: &str,
        frequency: &str,
        account_id: Option<&str>,
        category_id: Option<&str>,
        service_name: Option<&str>,
    ) -> Result<RecurringExpense, ApiError> {
        let prefix = self.space_prefix()?;
        let idempotency_key = Uuid::new_v4().to_string();

        let mut body = serde_json::Map::new();
        body.insert("name".into(), serde_json::Value::String(name.to_string()));
        body.insert(
            "amount".into(),
            serde_json::Value::String(amount.to_string()),
        );
        body.insert(
            "frequency".into(),
            serde_json::Value::String(frequency.to_string()),
        );
        if let Some(a) = account_id {
            body.insert(
                "account_id".into(),
                serde_json::Value::String(a.to_string()),
            );
        }
        if let Some(c) = category_id {
            body.insert(
                "category_id".into(),
                serde_json::Value::String(c.to_string()),
            );
        }
        if let Some(s) = service_name {
            body.insert(
                "service_name".into(),
                serde_json::Value::String(s.to_string()),
            );
        }

        let resp = self.post(
            &format!("{prefix}/subscriptions"),
            &serde_json::Value::Object(body),
            Some(&idempotency_key),
        )?;
        let data = resp.get("data").cloned().unwrap_or(resp);
        Ok(serde_json::from_value(data)?)
    }

    pub fn update_subscription(
        &self,
        id: &str,
        name: Option<&str>,
        amount: Option<&str>,
        frequency: Option<&str>,
        active: Option<bool>,
    ) -> Result<RecurringExpense, ApiError> {
        let prefix = self.space_prefix()?;
        let mut body = serde_json::Map::new();

        if let Some(v) = name {
            body.insert("name".into(), serde_json::Value::String(v.to_string()));
        }
        if let Some(v) = amount {
            body.insert("amount".into(), serde_json::Value::String(v.to_string()));
        }
        if let Some(v) = frequency {
            body.insert("frequency".into(), serde_json::Value::String(v.to_string()));
        }
        if let Some(v) = active {
            body.insert("active".into(), serde_json::Value::Bool(v));
        }

        let resp = self.patch(
            &format!("{prefix}/subscriptions/{id}"),
            &serde_json::Value::Object(body),
        )?;
        let data = resp.get("data").cloned().unwrap_or(resp);
        Ok(serde_json::from_value(data)?)
    }

    pub fn delete_subscription(&self, id: &str) -> Result<(), ApiError> {
        let prefix = self.space_prefix()?;
        self.delete(&format!("{prefix}/subscriptions/{id}"))?;
        Ok(())
    }

    // ── Fixed Bill Mutations ──

    pub fn create_fixed_bill(
        &self,
        name: &str,
        amount: &str,
        frequency: &str,
        account_id: Option<&str>,
        category_id: Option<&str>,
    ) -> Result<RecurringExpense, ApiError> {
        let prefix = self.space_prefix()?;
        let idempotency_key = Uuid::new_v4().to_string();

        let mut body = serde_json::Map::new();
        body.insert("name".into(), serde_json::Value::String(name.to_string()));
        body.insert(
            "amount".into(),
            serde_json::Value::String(amount.to_string()),
        );
        body.insert(
            "frequency".into(),
            serde_json::Value::String(frequency.to_string()),
        );
        if let Some(a) = account_id {
            body.insert(
                "account_id".into(),
                serde_json::Value::String(a.to_string()),
            );
        }
        if let Some(c) = category_id {
            body.insert(
                "category_id".into(),
                serde_json::Value::String(c.to_string()),
            );
        }

        let resp = self.post(
            &format!("{prefix}/fixed-bills"),
            &serde_json::Value::Object(body),
            Some(&idempotency_key),
        )?;
        let data = resp.get("data").cloned().unwrap_or(resp);
        Ok(serde_json::from_value(data)?)
    }

    pub fn update_fixed_bill(
        &self,
        id: &str,
        name: Option<&str>,
        amount: Option<&str>,
        frequency: Option<&str>,
        active: Option<bool>,
    ) -> Result<RecurringExpense, ApiError> {
        let prefix = self.space_prefix()?;
        let mut body = serde_json::Map::new();

        if let Some(v) = name {
            body.insert("name".into(), serde_json::Value::String(v.to_string()));
        }
        if let Some(v) = amount {
            body.insert("amount".into(), serde_json::Value::String(v.to_string()));
        }
        if let Some(v) = frequency {
            body.insert("frequency".into(), serde_json::Value::String(v.to_string()));
        }
        if let Some(v) = active {
            body.insert("active".into(), serde_json::Value::Bool(v));
        }

        let resp = self.patch(
            &format!("{prefix}/fixed-bills/{id}"),
            &serde_json::Value::Object(body),
        )?;
        let data = resp.get("data").cloned().unwrap_or(resp);
        Ok(serde_json::from_value(data)?)
    }

    pub fn delete_fixed_bill(&self, id: &str) -> Result<(), ApiError> {
        let prefix = self.space_prefix()?;
        self.delete(&format!("{prefix}/fixed-bills/{id}"))?;
        Ok(())
    }

    pub fn export_transactions_csv(
        &self,
        from_date: Option<&str>,
        to_date: Option<&str>,
        account_id: Option<&str>,
        category_id: Option<&str>,
        tag_id: Option<&str>,
        tx_type: Option<&str>,
        ignored: Option<bool>,
        search: Option<&str>,
        limit: Option<u32>,
        after: Option<&str>,
    ) -> Result<String, ApiError> {
        let prefix = self.space_prefix()?;
        let mut params = Vec::new();
        if let Some(v) = from_date {
            params.push(("from_date", v.to_string()));
        }
        if let Some(v) = to_date {
            params.push(("to_date", v.to_string()));
        }
        if let Some(v) = account_id {
            params.push(("account_id", v.to_string()));
        }
        if let Some(v) = category_id {
            params.push(("category_id", v.to_string()));
        }
        if let Some(v) = tag_id {
            params.push(("tag_id", v.to_string()));
        }
        if let Some(v) = tx_type {
            params.push(("type", v.to_string()));
        }
        if let Some(v) = ignored {
            params.push(("ignored", v.to_string()));
        }
        if let Some(v) = search {
            params.push(("search", v.to_string()));
        }
        if let Some(v) = limit {
            params.push(("limit", v.to_string()));
        }
        if let Some(v) = after {
            params.push(("after", v.to_string()));
        }
        let qs = Self::query_string(&params);

        self.get_response(&format!("{prefix}/transactions{qs}"), "text/csv")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::Credential;
    use serde_json::json;

    fn test_client(server: &mockito::Server) -> ApiClient {
        let cred = Credential::PersonalToken {
            token: "visor_pat_test_secret".to_string(),
        };
        ApiClient::for_test(&server.url(), cred, "s").unwrap()
    }

    #[test]
    fn test_create_transaction_sends_correct_request() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/v1/spaces/s/transactions")
            .match_header("authorization", "Bearer visor_pat_test_secret")
            .match_header("accept", "application/json")
            .match_header("idempotency-key", mockito::Matcher::Any)
            .match_body(mockito::Matcher::Json(json!({
                "amount": "42.50",
                "description": "Coffee",
                "type": "expense",
                "account_id": "acc-1"
            })))
            .with_status(201)
            .with_body(
                json!({
                    "data": {
                        "id": "tx-new",
                        "account_id": "acc-1",
                        "amount": "42.50",
                        "type": "expense",
                        "description": "Coffee",
                        "tag_ids": [],
                        "ignored": false
                    }
                })
                .to_string(),
            )
            .create();

        let client = test_client(&server);
        let tx = client
            .create_transaction(
                "42.50",
                "Coffee",
                "expense",
                "acc-1",
                None,
                &[],
                None,
                None,
                None,
                false,
            )
            .unwrap();

        assert_eq!(tx.id, "tx-new");
        assert_eq!(tx.amount, "42.50");
        assert_eq!(tx.description.as_deref(), Some("Coffee"));
        assert_eq!(tx.tx_type, "expense");
        assert_eq!(tx.account_id, "acc-1");
        mock.assert();
    }

    #[test]
    fn test_update_transaction_sends_sparse_body() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("PATCH", "/v1/spaces/s/transactions/t123")
            .match_header("authorization", "Bearer visor_pat_test_secret")
            .match_body(mockito::Matcher::Json(json!({
                "amount": "99.00",
                "description": "Updated coffee"
            })))
            .with_status(200)
            .with_body(
                json!({
                    "data": {
                        "id": "t123",
                        "account_id": "acc-1",
                        "amount": "99.00",
                        "type": "expense",
                        "description": "Updated coffee",
                        "tag_ids": [],
                        "ignored": false
                    }
                })
                .to_string(),
            )
            .create();

        let client = test_client(&server);
        let tx = client
            .update_transaction(
                "t123",
                Some("99.00"),
                Some("Updated coffee"),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                false,
            )
            .unwrap();

        assert_eq!(tx.id, "t123");
        assert_eq!(tx.amount, "99.00");
        assert_eq!(tx.description.as_deref(), Some("Updated coffee"));
        mock.assert();
    }

    #[test]
    fn test_delete_transaction_handles_204() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("DELETE", "/v1/spaces/s/transactions/t123")
            .match_header("authorization", "Bearer visor_pat_test_secret")
            .with_status(204)
            .create();

        let client = test_client(&server);
        let result = client.delete_transaction("t123");

        assert!(result.is_ok());
        mock.assert();
    }

    #[test]
    fn test_create_tag_with_idempotency() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/v1/spaces/s/tags")
            .match_header("authorization", "Bearer visor_pat_test_secret")
            .match_header("idempotency-key", mockito::Matcher::Any)
            .match_body(mockito::Matcher::Json(json!({
                "name": "Food",
                "color": "#ff0000"
            })))
            .with_status(201)
            .with_body(
                json!({
                    "data": {
                        "id": "tag-new",
                        "name": "Food",
                        "color": "#ff0000"
                    }
                })
                .to_string(),
            )
            .create();

        let client = test_client(&server);
        let tag = client.create_tag("Food", Some("#ff0000")).unwrap();

        assert_eq!(tag.id, "tag-new");
        assert_eq!(tag.name, "Food");
        assert_eq!(tag.color.as_deref(), Some("#ff0000"));
        mock.assert();
    }

    #[test]
    fn test_create_goal_sends_correct_body() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/v1/spaces/s/goals")
            .match_header("authorization", "Bearer visor_pat_test_secret")
            .match_header("idempotency-key", mockito::Matcher::Any)
            .match_body(mockito::Matcher::Json(json!({
                "name": "Emergency Fund",
                "target_amount": "10000.00",
                "kind": "savings"
            })))
            .with_status(201)
            .with_body(
                json!({
                    "data": {
                        "id": "goal-new",
                        "name": "Emergency Fund",
                        "target_amount": "10000.00",
                        "kind": "savings"
                    }
                })
                .to_string(),
            )
            .create();

        let client = test_client(&server);
        let goal = client
            .create_goal("Emergency Fund", "10000.00", Some("savings"), None, None)
            .unwrap();

        assert_eq!(goal.id, "goal-new");
        assert_eq!(goal.name, "Emergency Fund");
        assert_eq!(goal.target_amount.as_deref(), Some("10000.00"));
        assert_eq!(goal.kind.as_deref(), Some("savings"));
        mock.assert();
    }

    #[test]
    fn test_api_error_parsing() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/v1/spaces/s/transactions")
            .with_status(422)
            .with_body(
                json!({
                    "errors": [{
                        "code": "VALIDATION_FAILED",
                        "message": "Amount is required"
                    }]
                })
                .to_string(),
            )
            .create();

        let client = test_client(&server);
        let err = client
            .create_transaction(
                "",
                "",
                "expense",
                "acc-1",
                None,
                &[],
                None,
                None,
                None,
                false,
            )
            .unwrap_err();

        match &err {
            ApiError::Api(data) => {
                assert_eq!(data.status, 422);
                assert_eq!(data.code, "VALIDATION_FAILED");
                assert_eq!(data.message, "Amount is required");
            }
            other => panic!("expected ApiError::Api, got: {other:?}"),
        }
        mock.assert();
    }

    // ── Structured error handling tests ──

    #[test]
    fn test_exit_code_mapping() {
        let make = |status: u16| -> u8 {
            let body = json!({"errors": [{"code": "TEST", "message": "test"}]});
            ApiError::from_body(status, &body).exit_code()
        };
        assert_eq!(make(401), exit::AUTH);
        assert_eq!(make(403), exit::AUTH);
        assert_eq!(make(404), exit::NOT_FOUND);
        assert_eq!(make(400), exit::REJECTED);
        assert_eq!(make(409), exit::REJECTED);
        assert_eq!(make(422), exit::REJECTED);
        assert_eq!(make(429), exit::UNAVAILABLE);
        assert_eq!(make(500), exit::UNAVAILABLE);
        assert_eq!(make(502), exit::UNAVAILABLE);
        assert_eq!(make(418), exit::FAILURE);
    }

    #[test]
    fn test_hint_invalid_token() {
        let body = json!({"errors": [{"code": "INVALID_TOKEN", "message": "Bad token"}]});
        let err = ApiError::from_body(401, &body);
        let hint = err.hint().expect("should have hint");
        assert!(hint.contains("login"), "{hint}");
    }

    #[test]
    fn test_hint_insufficient_scope() {
        let body = json!({"errors": [{"code": "INSUFFICIENT_SCOPE", "message": "Missing scope", "details": {"required_scope": "pfm:transactions:write"}}]});
        let err = ApiError::from_body(403, &body);
        let hint = err.hint().expect("should have hint");
        assert!(hint.contains("pfm:transactions:write"), "{hint}");
    }

    #[test]
    fn test_hint_rate_limited_with_retry() {
        let body = json!({"errors": [{"code": "RATE_LIMITED", "message": "Too many requests", "details": {"retry_after_seconds": 30}}]});
        let err = ApiError::from_body(429, &body);
        let hint = err.hint().expect("should have hint");
        assert!(hint.contains("30"), "{hint}");
    }

    #[test]
    fn test_hint_rate_limited_without_retry() {
        let body = json!({"errors": [{"code": "RATE_LIMITED", "message": "Too many requests"}]});
        let err = ApiError::from_body(429, &body);
        let hint = err.hint().expect("should have hint");
        assert!(hint.contains("minuto"), "{hint}");
    }

    #[test]
    fn test_hint_idempotency_conflict() {
        let body = json!({"errors": [{"code": "IDEMPOTENCY_CONFLICT", "message": "Duplicate"}]});
        let err = ApiError::from_body(409, &body);
        let hint = err.hint().expect("should have hint");
        assert!(hint.contains("já foi realizada"), "{hint}");
    }

    #[test]
    fn test_hint_immutable_provider_field() {
        let body =
            json!({"errors": [{"code": "IMMUTABLE_PROVIDER_FIELD", "message": "Cannot edit"}]});
        let err = ApiError::from_body(409, &body);
        let hint = err.hint().expect("should have hint");
        assert!(hint.contains("sincronizado"), "{hint}");
    }

    #[test]
    fn test_hint_transaction_conflict() {
        let body = json!({"errors": [{"code": "TRANSACTION_CONFLICT", "message": "Conflict", "details": {"dependency": "bill-123"}}]});
        let err = ApiError::from_body(409, &body);
        let hint = err.hint().expect("should have hint");
        assert!(hint.contains("bill-123"), "{hint}");
    }

    #[test]
    fn test_hint_unknown_code_returns_none() {
        let body = json!({"errors": [{"code": "SOMETHING_ELSE", "message": "Unknown"}]});
        let err = ApiError::from_body(500, &body);
        assert!(err.hint().is_none());
    }

    #[test]
    fn test_field_errors_extraction() {
        let body = json!({"errors": [{"code": "VALIDATION_FAILED", "message": "Invalid", "details": {"errors": [
            {"field": "amount", "message": "must be positive"},
            {"field": "description", "message": "is required"}
        ]}}]});
        let err = ApiError::from_body(422, &body);
        let fields = err.field_errors();
        assert_eq!(fields.len(), 2);
        assert_eq!(
            fields[0],
            ("amount".to_string(), "must be positive".to_string())
        );
        assert_eq!(
            fields[1],
            ("description".to_string(), "is required".to_string())
        );
    }

    #[test]
    fn test_field_errors_empty_for_non_validation() {
        let body = json!({"errors": [{"code": "NOT_FOUND", "message": "Not found"}]});
        let err = ApiError::from_body(404, &body);
        assert_eq!(err.field_errors(), Vec::<(String, String)>::new());
    }

    #[test]
    fn test_display_includes_request_id() {
        let body = json!({"errors": [{"code": "NOT_FOUND", "message": "Not found", "request_id": "req-abc-123"}]});
        let err = ApiError::from_body(404, &body);
        let display = err.to_string();
        assert!(display.contains("req-abc-123"), "{display}");
    }

    #[test]
    fn test_display_validation_failed_shows_fields() {
        let body = json!({"errors": [{"code": "VALIDATION_FAILED", "message": "Invalid body", "details": {"errors": [
            {"field": "amount", "message": "must be positive"}
        ]}}]});
        let err = ApiError::from_body(422, &body);
        let display = err.to_string();
        assert!(display.contains("amount"), "{display}");
        assert!(display.contains("must be positive"), "{display}");
    }

    #[test]
    fn test_from_body_fallback_for_non_envelope() {
        let body = json!({"error": "something went wrong"});
        let err = ApiError::from_body(502, &body);
        assert_eq!(err.exit_code(), exit::UNAVAILABLE);
        match &err {
            ApiError::Api(data) => assert_eq!(data.code, "HTTP_502"),
            other => panic!("expected Api variant, got: {other:?}"),
        }
    }

    #[test]
    fn test_api_code_accessor() {
        let body = json!({"errors": [{"code": "NOT_FOUND", "message": "Not found"}]});
        let err = ApiError::from_body(404, &body);
        assert_eq!(err.api_code(), Some("NOT_FOUND"));

        let http_err = ApiError::NoCredential;
        assert_eq!(http_err.api_code(), None);
    }

    #[test]
    fn test_error_via_mockito_get() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/v1/me")
            .with_status(401)
            .with_body(
                json!({"errors": [{"code": "INVALID_TOKEN", "message": "Token expired", "request_id": "r1"}]})
                .to_string(),
            )
            .create();

        let client = test_client(&server);
        let err = client.me().unwrap_err();
        assert_eq!(err.exit_code(), exit::AUTH);
        assert_eq!(err.api_code(), Some("INVALID_TOKEN"));
        assert!(err.hint().is_some());
        let display = err.to_string();
        assert!(display.contains("r1"), "{display}");
        mock.assert();
    }

    #[test]
    fn test_get_retries_on_429_with_retry_after() {
        let mut server = mockito::Server::new();
        // First request returns 429 with retry_after_seconds = 1
        let mock_429 = server
            .mock("GET", "/v1/spaces/s/tags")
            .match_header("authorization", "Bearer visor_pat_test_secret")
            .with_status(429)
            .with_body(
                json!({"errors": [{"code": "RATE_LIMITED", "message": "Too many requests", "details": {"retry_after_seconds": 1}}]})
                .to_string(),
            )
            .create();
        // Second request succeeds
        let mock_ok = server
            .mock("GET", "/v1/spaces/s/tags")
            .match_header("authorization", "Bearer visor_pat_test_secret")
            .with_status(200)
            .with_body(
                json!({"data": [{"id": "t1", "name": "Food", "color": "#ff0000"}]}).to_string(),
            )
            .create();

        let client = test_client(&server);
        let tags = client.tags().unwrap();

        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].id, "t1");
        mock_429.assert();
        mock_ok.assert();
    }

    #[test]
    fn test_categories_returns_error_after_max_retries_on_429() {
        let mut server = mockito::Server::new();
        // All requests return 429 — should exhaust retries
        let mock = server
            .mock("GET", "/v1/spaces/s/categories")
            .match_header("authorization", "Bearer visor_pat_test_secret")
            .with_status(429)
            .with_body(
                json!({"errors": [{"code": "RATE_LIMITED", "message": "Too many requests", "details": {"retry_after_seconds": 1}}]})
                .to_string(),
            )
            .expect(MAX_RETRIES as usize + 1)
            .create();

        let client = test_client(&server);
        let result = client.categories(false);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.http_status(), Some(429));
        mock.assert();
    }

    #[test]
    fn test_cache_hit_avoids_second_request() {
        let mut server = mockito::Server::new();
        // Only one GET should be made; second call should hit cache
        let mock = server
            .mock("GET", "/v1/spaces/s/goals")
            .match_header("authorization", "Bearer visor_pat_test_secret")
            .with_status(200)
            .with_body(
                json!({"data": [{"id": "g1", "name": "Savings", "target_amount": "5000", "kind": "savings"}]})
                .to_string(),
            )
            .expect(1)
            .create();

        let client = test_client(&server);
        let goals1 = client.goals(None).unwrap();
        let goals2 = client.goals(None).unwrap();

        assert_eq!(goals1.len(), 1);
        assert_eq!(goals2.len(), 1);
        assert_eq!(goals1[0].id, goals2[0].id);
        mock.assert();
    }

    #[test]
    fn test_write_invalidates_cache_for_subsequent_get() {
        let mut server = mockito::Server::new();
        // First GET populates cache
        let mock_get1 = server
            .mock("GET", "/v1/spaces/s/tags")
            .match_header("authorization", "Bearer visor_pat_test_secret")
            .with_status(200)
            .with_body(json!({"data": [{"id": "t1", "name": "Old", "color": "#000"}]}).to_string())
            .create();
        // POST creates a tag (invalidates cache)
        let mock_post = server
            .mock("POST", "/v1/spaces/s/tags")
            .match_header("authorization", "Bearer visor_pat_test_secret")
            .with_status(201)
            .with_body(json!({"data": {"id": "t2", "name": "New", "color": "#fff"}}).to_string())
            .create();
        // Second GET must hit server again (cache was invalidated)
        let mock_get2 = server
            .mock("GET", "/v1/spaces/s/tags")
            .match_header("authorization", "Bearer visor_pat_test_secret")
            .with_status(200)
            .with_body(
                json!({"data": [{"id": "t1", "name": "Old", "color": "#000"}, {"id": "t2", "name": "New", "color": "#fff"}]})
                .to_string(),
            )
            .create();

        let client = test_client(&server);
        let tags1 = client.tags().unwrap();
        assert_eq!(tags1.len(), 1);

        let _new_tag = client.create_tag("New", Some("#fff")).unwrap();

        let tags2 = client.tags().unwrap();
        assert_eq!(tags2.len(), 2);

        mock_get1.assert();
        mock_post.assert();
        mock_get2.assert();
    }

    fn expire_cache(client: &ApiClient) {
        let at = Instant::now()
            .checked_sub(Duration::from_secs(CACHE_TTL_SECS + 1))
            .unwrap();
        let mut state = client.requests.lock().unwrap();
        for entry in state.cache.values_mut() {
            entry.fetched_at = at;
        }
        for entry in state.ranges.values_mut() {
            entry.fetched_at = at;
        }
    }

    fn october_query() -> TransactionQuery {
        TransactionQuery {
            range: DateRange::parse("2026-10-01", "2026-10-31").unwrap(),
            filters: TransactionFilters::default(),
        }
    }

    #[test]
    fn redraw_uses_expired_cache_until_fetch_is_allowed() {
        let mut server = mockito::Server::new();
        let first = server
            .mock("GET", "/v1/spaces/s/tags")
            .with_body(r#"{"data":[{"id":"one","name":"Cached"}]}"#)
            .expect(1)
            .create();
        let second = server
            .mock("GET", "/v1/spaces/s/tags")
            .with_body(r#"{"data":[{"id":"two","name":"Fresh"}]}"#)
            .expect(1)
            .create();
        let client = test_client(&server);
        assert_eq!(client.tags().unwrap()[0].id, "one");
        expire_cache(&client);
        client.set_fetch_allowed(false);
        for _ in 0..3 {
            assert_eq!(client.tags().unwrap()[0].id, "one");
        }
        client.set_fetch_allowed(true);
        assert_eq!(client.tags().unwrap()[0].id, "two");
        first.assert();
        second.assert();
    }

    #[test]
    fn redraw_cache_miss_never_sends_a_request() {
        let mut server = mockito::Server::new();
        let mock = server.mock("GET", mockito::Matcher::Any).expect(0).create();
        let client = test_client(&server);
        client.set_fetch_allowed(false);
        assert!(client.tags().unwrap_err().to_string().contains("cache"));
        assert!(
            client
                .export_transactions_csv(None, None, None, None, None, None, None, None, None, None)
                .is_err()
        );
        mock.assert();
    }

    #[test]
    fn cached_failures_preserve_api_details_without_redraw_retries() {
        let mut server = mockito::Server::new();
        let mock = server.mock("GET", "/v1/spaces/s/tags")
            .with_status(422)
            .with_body(json!({"errors":[{"code":"VALIDATION_FAILED","message":"Invalid","request_id":"r1","details":{"errors":[{"field":"name","message":"Required"}]}}]}).to_string())
            .expect(1).create();
        let client = test_client(&server);
        let original = client.tags().unwrap_err();
        assert!(client.tags().is_err());
        expire_cache(&client);
        client.set_fetch_allowed(false);
        let cached = client.tags().unwrap_err();
        assert_eq!(cached.to_string(), original.to_string());
        assert_eq!(
            cached.field_errors(),
            vec![("name".to_owned(), "Required".to_owned())]
        );
        assert_eq!(cached.http_status(), Some(422));
        assert!(client.request_status().contains("falha"));
        mock.assert();
    }

    #[test]
    fn malformed_response_does_not_retry_on_redraw() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/v1/spaces/s/tags")
            .with_body("not JSON")
            .expect(1)
            .create();
        let client = test_client(&server);
        assert!(client.tags().is_err());
        client.set_fetch_allowed(false);
        assert!(client.tags().is_err());
        mock.assert();
    }

    #[test]
    fn refresh_fetches_before_ttl_expires() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/v1/spaces/s/tags")
            .with_body(r#"{"data":[]}"#)
            .expect(2)
            .create();
        let client = test_client(&server);
        client.tags().unwrap();
        client.invalidate_cache();
        client.tags().unwrap();
        assert!(client.request_status().contains("sync "));
        assert!(client.request_status().contains("cache 1"));
        mock.assert();
    }

    #[test]
    fn range_fetch_encodes_filters_and_cursor_without_partial_cache_entries() {
        let mut server = mockito::Server::new();
        let first = server.mock("GET", "/v1/spaces/s/transactions")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("from_date".into(), "2026-10-01".into()),
                mockito::Matcher::UrlEncoded("to_date".into(), "2026-10-31".into()),
                mockito::Matcher::UrlEncoded("limit".into(), "100".into()),
                mockito::Matcher::UrlEncoded("type".into(), "debit".into()),
                mockito::Matcher::UrlEncoded("search".into(), "café + pão & type=income#x".into()),
            ]))
            .with_body(json!({"data":[{"id":"one","account_id":"a","amount":"1.00","type":"debit"}],"next_cursor":"a+/=b&c"}).to_string()).expect(1).create();
        let second = server.mock("GET", "/v1/spaces/s/transactions")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("from_date".into(), "2026-10-01".into()),
                mockito::Matcher::UrlEncoded("to_date".into(), "2026-10-31".into()),
                mockito::Matcher::UrlEncoded("after".into(), "a+/=b&c".into()),
                mockito::Matcher::UrlEncoded("type".into(), "debit".into()),
                mockito::Matcher::UrlEncoded("search".into(), "café + pão & type=income#x".into()),
            ]))
            .with_body(json!({"data":[{"id":"one","account_id":"a","amount":"1.00","type":"debit"},{"id":"two","account_id":"a","amount":"2.00","type":"debit"}],"next_cursor":null}).to_string()).expect(1).create();
        let api = test_client(&server);
        let mut query = october_query();
        query.filters.tx_type = TxTypeFilter::Debit;
        query.filters.search = "café + pão & type=income#x".into();
        let rows = api.transactions_in_range(&query).unwrap();
        assert_eq!(
            rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            ["one", "two"]
        );
        assert!(Arc::ptr_eq(
            &rows,
            &api.transactions_in_range(&query).unwrap()
        ));
        assert!(api.requests.lock().unwrap().cache.is_empty());
        first.assert();
        second.assert();
    }

    #[test]
    fn failed_later_page_is_never_cached_and_retry_starts_from_first_page() {
        let mut server = mockito::Server::new();
        let first = server.mock("GET", "/v1/spaces/s/transactions")
            .match_query("from_date=2026-10-01&to_date=2026-10-31&limit=100")
            .with_body(json!({"data":[{"id":"one","account_id":"a","amount":"1","type":"debit"}],"next_cursor":"next"}).to_string()).expect(2).create();
        let failure = server
            .mock("GET", "/v1/spaces/s/transactions")
            .match_query("from_date=2026-10-01&to_date=2026-10-31&limit=100&after=next")
            .with_status(503)
            .with_body(r#"{"errors":[{"code":"OFFLINE","message":"Unavailable"}]}"#)
            .expect(1)
            .create();
        let api = test_client(&server);
        let query = october_query();
        assert!(api.transactions_in_range(&query).is_err());
        assert!(api.requests.lock().unwrap().ranges.is_empty());
        failure.assert();
        failure.remove();
        let success = server.mock("GET", "/v1/spaces/s/transactions")
            .match_query("from_date=2026-10-01&to_date=2026-10-31&limit=100&after=next")
            .with_body(json!({"data":[{"id":"two","account_id":"a","amount":"2","type":"debit"}],"next_cursor":null}).to_string()).expect(1).create();
        assert_eq!(api.transactions_in_range(&query).unwrap().len(), 2);
        first.assert();
        success.assert();
    }

    #[test]
    fn repeated_cursor_fails_without_publishing_an_incomplete_range() {
        let mut server = mockito::Server::new();
        let pages = server
            .mock("GET", "/v1/spaces/s/transactions")
            .match_query(mockito::Matcher::Any)
            .with_body(r#"{"data":[],"next_cursor":"repeat"}"#)
            .expect(2)
            .create();
        let api = test_client(&server);
        let error = api.transactions_in_range(&october_query()).unwrap_err();
        assert!(error.to_string().contains("Paginação repetida"));
        assert!(api.requests.lock().unwrap().ranges.is_empty());
        pages.assert();
    }

    #[test]
    fn complete_range_cache_obeys_ttl_fetch_gate_refresh_and_all_query_dimensions() {
        let mut server = mockito::Server::new();
        let pages = server
            .mock("GET", "/v1/spaces/s/transactions")
            .match_query(mockito::Matcher::Any)
            .with_body(r#"{"data":[]}"#)
            .expect(10)
            .create();
        let api = test_client(&server);
        let query = october_query();
        api.transactions_in_range(&query).unwrap();
        expire_cache(&api);
        api.set_fetch_allowed(false);
        api.transactions_in_range(&query).unwrap();
        let mut variants = Vec::new();
        let mut changed = query.clone();
        changed.range = DateRange::parse("2026-11-01", "2026-11-30").unwrap();
        variants.push(changed);
        let mut changed = query.clone();
        changed.filters.account_id = Some("a".into());
        variants.push(changed);
        let mut changed = query.clone();
        changed.filters.category_id = Some("c".into());
        variants.push(changed);
        let mut changed = query.clone();
        changed.filters.tag_id = Some("t".into());
        variants.push(changed);
        let mut changed = query.clone();
        changed.filters.tx_type = TxTypeFilter::Credit;
        variants.push(changed);
        let mut changed = query.clone();
        changed.filters.ignored = IgnoredFilter::NotIgnored;
        variants.push(changed);
        let mut changed = query.clone();
        changed.filters.search = "coffee".into();
        variants.push(changed);
        assert!(api.transactions_in_range(&variants[0]).is_err());
        api.set_fetch_allowed(true);
        api.transactions_in_range(&query).unwrap();
        for variant in &variants {
            api.transactions_in_range(variant).unwrap();
            api.transactions_in_range(variant).unwrap();
        }
        api.invalidate_cache();
        api.transactions_in_range(&query).unwrap();
        pages.assert();
    }

    #[test]
    fn complete_range_keys_include_space() {
        let mut server = mockito::Server::new();
        let first = server
            .mock("GET", "/v1/spaces/s/transactions")
            .match_query(mockito::Matcher::Any)
            .with_body(
                json!({"data":[{"id":"one","account_id":"a","amount":"1","type":"debit"}]})
                    .to_string(),
            )
            .create();
        let second = server
            .mock("GET", "/v1/spaces/other/transactions")
            .match_query(mockito::Matcher::Any)
            .with_body(
                json!({"data":[{"id":"two","account_id":"a","amount":"1","type":"debit"}]})
                    .to_string(),
            )
            .create();
        let mut api = test_client(&server);
        let query = october_query();
        assert_eq!(api.transactions_in_range(&query).unwrap()[0].id, "one");
        api.space_id = Some("other".into());
        assert_eq!(api.transactions_in_range(&query).unwrap()[0].id, "two");
        api.space_id = Some("s".into());
        assert_eq!(api.transactions_in_range(&query).unwrap()[0].id, "one");
        first.assert();
        second.assert();
    }

    #[test]
    fn csv_cache_is_separate_from_json_and_uses_encoded_queries() {
        let mut server = mockito::Server::new();
        let matcher = mockito::Matcher::UrlEncoded("search".to_owned(), "A&B".to_owned());
        let json_mock = server
            .mock("GET", "/v1/spaces/s/transactions")
            .match_query(matcher.clone())
            .match_header("accept", "application/json")
            .with_body(r#"{"data":[]}"#)
            .expect(1)
            .create();
        let csv_mock = server
            .mock("GET", "/v1/spaces/s/transactions")
            .match_query(matcher)
            .match_header("accept", "text/csv")
            .with_body("description,amount\nA&B,10.00\n")
            .expect(1)
            .create();
        let client = test_client(&server);
        let mut query = october_query();
        query.filters.search = "A&B".into();
        client.transactions_in_range(&query).unwrap();
        let csv = client
            .export_transactions_csv(
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                Some("A&B"),
                None,
                None,
            )
            .unwrap();
        client.set_fetch_allowed(false);
        assert_eq!(
            client
                .export_transactions_csv(
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some("A&B"),
                    None,
                    None
                )
                .unwrap(),
            csv
        );
        json_mock.assert();
        csv_mock.assert();
    }

    #[test]
    fn successful_writes_invalidate_filtered_reads_and_aggregates() {
        let mut server = mockito::Server::new();
        let read = server
            .mock("GET", "/v1/spaces/s/transactions")
            .match_query(mockito::Matcher::UrlEncoded(
                "ignored".into(),
                "true".into(),
            ))
            .with_body(r#"{"data":[]}"#)
            .expect(2)
            .create();
        let aggregate = server
            .mock("GET", "/v1/spaces/s/overview")
            .with_body(r#"{"data":{}}"#)
            .expect(2)
            .create();
        let write = server
            .mock("DELETE", "/v1/spaces/s/transactions/tx1")
            .with_status(204)
            .expect(1)
            .create();
        let client = test_client(&server);
        for iteration in 0..2 {
            let mut query = october_query();
            query.filters.ignored = IgnoredFilter::OnlyIgnored;
            client.transactions_in_range(&query).unwrap();
            let _: serde_json::Value = client.get("/v1/spaces/s/overview").unwrap();
            if iteration == 0 {
                client.delete_transaction("tx1").unwrap();
            }
        }
        read.assert();
        aggregate.assert();
        write.assert();
    }

    #[test]
    fn notification_read_invalidates_filtered_notification_cache() {
        let mut server = mockito::Server::new();
        let read = server
            .mock("GET", "/v1/notifications?unread=true&limit=20")
            .with_body(r#"{"data":[]}"#)
            .expect(2)
            .create();
        let write = server
            .mock("PATCH", "/v1/notifications/n1")
            .match_body(mockito::Matcher::Json(json!({"read":true})))
            .with_status(204)
            .expect(1)
            .create();
        let client = test_client(&server);
        client.notifications(Some(true), Some(20)).unwrap();
        client.mark_notification_read("n1").unwrap();
        client.notifications(Some(true), Some(20)).unwrap();
        read.assert();
        write.assert();
    }

    #[test]
    fn successful_mutation_invalidates_even_with_an_invalid_response_body() {
        let mut server = mockito::Server::new();
        let read = server
            .mock("GET", "/v1/spaces/s/tags")
            .with_body(r#"{"data":[]}"#)
            .expect(2)
            .create();
        let write = server
            .mock("POST", "/v1/spaces/s/tags")
            .with_status(201)
            .with_body("invalid JSON")
            .expect(1)
            .create();
        let client = test_client(&server);
        client.tags().unwrap();
        assert!(client.create_tag("new", None).is_err());
        client.tags().unwrap();
        read.assert();
        write.assert();
    }

    #[test]
    fn retry_after_header_overrides_json_delay() {
        let mut server = mockito::Server::new();
        let limited = server
            .mock("GET", "/v1/spaces/s/tags")
            .with_status(429)
            .with_header("retry-after", "0")
            .with_body(
                json!({"errors":[{"code":"RATE_LIMITED","details":{"retry_after_seconds":60}}]})
                    .to_string(),
            )
            .expect(1)
            .create();
        let recovered = server
            .mock("GET", "/v1/spaces/s/tags")
            .with_body(r#"{"data":[]}"#)
            .expect(1)
            .create();
        let client = test_client(&server);
        assert!(client.tags().unwrap().is_empty());
        limited.assert();
        recovered.assert();
    }

    #[test]
    fn long_retry_delays_defer_and_refresh_cannot_bypass_cooldown() {
        for header in [
            "18446744073709551615".to_owned(),
            (chrono::Utc::now() + chrono::Duration::minutes(2))
                .format("%a, %d %b %Y %H:%M:%S GMT")
                .to_string(),
        ] {
            let mut server = mockito::Server::new();
            let limited = server
                .mock("GET", "/v1/spaces/s/tags")
                .with_status(429)
                .with_header("retry-after", &header)
                .with_body(r#"{"errors":[{"code":"RATE_LIMITED"}]}"#)
                .expect(1)
                .create();
            let blocked = server
                .mock("GET", "/v1/spaces/s/categories")
                .expect(0)
                .create();
            let client = test_client(&server);
            assert_eq!(client.tags().unwrap_err().http_status(), Some(429));
            client.invalidate_cache();
            assert_eq!(
                client.categories(false).unwrap_err().http_status(),
                Some(429)
            );
            assert_eq!(client.tags().unwrap_err().http_status(), Some(429));
            assert!(client.request_status().contains("limite "));
            limited.assert();
            blocked.assert();
        }
    }

    #[test]
    fn expired_cooldown_allows_explicit_refresh() {
        let mut server = mockito::Server::new();
        let limited = server
            .mock("GET", "/v1/spaces/s/tags")
            .with_status(429)
            .with_header("retry-after", "60")
            .expect(1)
            .create();
        let recovered = server
            .mock("GET", "/v1/spaces/s/tags")
            .with_body(r#"{"data":[]}"#)
            .expect(1)
            .create();
        let client = test_client(&server);
        assert!(client.tags().is_err());
        client.requests.lock().unwrap().cooldown_until =
            Instant::now().checked_sub(Duration::from_secs(1));
        client.invalidate_cache();
        client.tags().unwrap();
        limited.assert();
        recovered.assert();
    }

    #[test]
    fn transaction_create_can_reuse_a_draft_key_after_an_uncertain_response() {
        let mut server = mockito::Server::new();
        let payload = json!({"amount":"18.90", "description":"Padaria", "type":"debit", "account_id":"a", "category_id":"c", "transacted_at":"2026-10-22T12:00:00Z"});
        let first = server
            .mock("POST", "/v1/spaces/s/transactions")
            .match_header("idempotency-key", "draft-key")
            .match_body(mockito::Matcher::Json(payload.clone()))
            .with_status(201)
            .with_body("incomplete response")
            .expect(1)
            .create();
        let replay = server
            .mock("POST", "/v1/spaces/s/transactions")
            .match_header("idempotency-key", "draft-key")
            .match_body(mockito::Matcher::Json(payload.clone()))
            .with_body(
                json!({"data":{"id":"tx1", "account_id":"a", "amount":"18.90", "type":"debit"}})
                    .to_string(),
            )
            .expect(1)
            .create();
        let client = test_client(&server);
        assert!(
            client
                .create_transaction_payload(&payload, "draft-key")
                .is_err()
        );
        assert_eq!(
            client
                .create_transaction_payload(&payload, "draft-key")
                .unwrap()
                .id,
            "tx1"
        );
        first.assert();
        replay.assert();
    }

    #[test]
    fn writes_observe_an_active_rate_limit() {
        let mut server = mockito::Server::new();
        let limited = server
            .mock("POST", "/v1/spaces/s/tags")
            .with_status(429)
            .with_header("retry-after", "60")
            .with_body(r#"{"errors":[{"code":"RATE_LIMITED"}]}"#)
            .expect(1)
            .create();
        let patch = server
            .mock("PATCH", "/v1/spaces/s/tags/t1")
            .expect(0)
            .create();
        let delete = server
            .mock("DELETE", "/v1/spaces/s/tags/t1")
            .expect(0)
            .create();
        let client = test_client(&server);
        assert_eq!(
            client.create_tag("test", None).unwrap_err().http_status(),
            Some(429)
        );
        for result in [
            client.post("/v1/spaces/s/tags", &json!({}), None),
            client.patch("/v1/spaces/s/tags/t1", &json!({})),
        ] {
            assert_eq!(result.unwrap_err().http_status(), Some(429));
        }
        assert_eq!(
            client
                .delete("/v1/spaces/s/tags/t1")
                .unwrap_err()
                .http_status(),
            Some(429)
        );
        limited.assert();
        patch.assert();
        delete.assert();
    }

    #[test]
    fn invalid_json_marks_sync_failed() {
        let mut server = mockito::Server::new();
        let invalid = server
            .mock("GET", "/v1/spaces/s/tags")
            .with_body("not json")
            .expect(1)
            .create();
        let client = test_client(&server);
        assert!(client.tags().is_err());
        assert!(
            client.request_status().contains("falha"),
            "{}",
            client.request_status()
        );
        invalid.assert();
    }

    #[test]
    fn write_rate_limit_is_never_retried() {
        let mut server = mockito::Server::new();
        let write = server
            .mock("POST", "/v1/spaces/s/tags")
            .with_status(429)
            .with_header("retry-after", "0")
            .with_body(r#"{"errors":[{"code":"RATE_LIMITED"}]}"#)
            .expect(1)
            .create();
        let client = test_client(&server);
        assert_eq!(
            client.create_tag("new", None).unwrap_err().http_status(),
            Some(429)
        );
        write.assert();
    }

    #[test]
    fn transaction_patch_preserves_the_validated_payload() {
        let mut server = mockito::Server::new();
        let body = json!({"category_id":"category-2","tag_ids":[],"notes":""});
        let write = server.mock("PATCH", "/v1/spaces/s/transactions/tx1")
            .match_body(mockito::Matcher::Json(body.clone()))
            .with_body(r#"{"data":{"id":"tx1","account_id":"acc1","amount":"12.00","type":"expense","tag_ids":[],"ignored":false}}"#)
            .expect(1).create();
        let client = test_client(&server);
        assert_eq!(client.patch_transaction("tx1", &body).unwrap().id, "tx1");
        write.assert();
    }
}
