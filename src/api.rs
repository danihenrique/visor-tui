use std::fmt::Write;

use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use uuid::Uuid;

use crate::auth::Credential;
use crate::config;
use crate::models::{
    Account, Bill, Card, Category, Connection, Goal, InstallmentPlan, Investment,
    InvestmentSummary, Me, Notification, Overview, RecurringExpense, Space, Tag, Transaction,
};

/// API client that reuses visor-cli's auth and config mechanisms.
pub struct ApiClient {
    client: Client,
    base_url: String,
    credential: Credential,
    space_id: Option<String>,
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
#[derive(Debug)]
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
            Self::Http(_) => exit::UNAVAILABLE,
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
                "IDEMPOTENCY_CONFLICT" => {
                    Some("Esta operação já foi realizada.".to_owned())
                }
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
                    let field = error.get("field").and_then(|f| f.as_str()).unwrap_or("body");
                    let msg = error.get("message").and_then(|m| m.as_str()).unwrap_or("é inválido");
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
                    out.push_str(
                        "Limite de requisições atingido. Tente novamente em um minuto.",
                    );
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
            .build()?;

        Ok(Self {
            client,
            base_url,
            credential,
            space_id,
        })
    }

    /// Create a test client with explicit base URL, credential, and space ID.
    #[cfg(test)]
    fn for_test(base_url: &str, credential: Credential, space_id: &str) -> Result<Self, ApiError> {
        let client = Client::builder()
            .user_agent(format!("visor-tui/{}", env!("CARGO_PKG_VERSION")))
            .build()?;

        Ok(Self {
            client,
            base_url: base_url.to_string(),
            credential,
            space_id: Some(space_id.to_string()),
        })
    }

    fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.credential.bearer_token()))
            .header("Accept", "application/json")
            .send()?;

        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body: serde_json::Value = resp.json().unwrap_or_default();
            return Err(ApiError::from_body(status, &body));
        }

        Ok(resp.json()?)
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

    pub fn notifications(&self, unread: Option<bool>, limit: Option<u32>) -> Result<Vec<Notification>, ApiError> {
        let mut params = Vec::new();
        if let Some(u) = unread {
            params.push(format!("unread={u}"));
        }
        if let Some(l) = limit {
            params.push(format!("limit={l}"));
        }
        let qs = if params.is_empty() {
            String::new()
        } else {
            format!("?{}", params.join("&"))
        };
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
        let qs = if include_hidden { "?include_hidden=true" } else { "" };
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

    pub fn bills(&self, card_id: &str, from: Option<&str>, to: Option<&str>) -> Result<Vec<Bill>, ApiError> {
        let prefix = self.space_prefix()?;
        let mut params = Vec::new();
        if let Some(f) = from {
            params.push(format!("from={f}"));
        }
        if let Some(t) = to {
            params.push(format!("to={t}"));
        }
        let qs = if params.is_empty() {
            String::new()
        } else {
            format!("?{}", params.join("&"))
        };
        let resp: serde_json::Value = self.get(&format!("{prefix}/cards/{card_id}/bills{qs}"))?;
        let data = resp.get("data").cloned().unwrap_or_default();
        Ok(serde_json::from_value(data)?)
    }

    pub fn categories(&self, include_hidden: bool) -> Result<Vec<Category>, ApiError> {
        let prefix = self.space_prefix()?;
        let qs = if include_hidden { "?include_hidden=true" } else { "" };
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
        let qs = status.map_or(String::new(), |s| format!("?status={s}"));
        let resp: serde_json::Value = self.get(&format!("{prefix}/goals{qs}"))?;
        let data = resp.get("data").cloned().unwrap_or_default();
        Ok(serde_json::from_value(data)?)
    }

    pub fn investments(&self) -> Result<(Vec<Investment>, Option<InvestmentSummary>), ApiError> {
        let prefix = self.space_prefix()?;
        let resp: serde_json::Value = self.get(&format!("{prefix}/investments"))?;
        let data = resp.get("data").cloned().unwrap_or_default();
        let summary = resp.get("summary").cloned().and_then(|s| serde_json::from_value(s).ok());
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

    pub fn installment_plans(&self, status: Option<&str>) -> Result<Vec<InstallmentPlan>, ApiError> {
        let prefix = self.space_prefix()?;
        let qs = status.map_or(String::new(), |s| format!("?status={s}"));
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

    pub fn transactions(
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
    ) -> Result<(Vec<Transaction>, Option<String>), ApiError> {
        let prefix = self.space_prefix()?;
        let mut params = Vec::new();
        if let Some(v) = from_date { params.push(format!("from_date={v}")); }
        if let Some(v) = to_date { params.push(format!("to_date={v}")); }
        if let Some(v) = account_id { params.push(format!("account_id={v}")); }
        if let Some(v) = category_id { params.push(format!("category_id={v}")); }
        if let Some(v) = tag_id { params.push(format!("tag_id={v}")); }
        if let Some(v) = tx_type { params.push(format!("type={v}")); }
        if let Some(v) = ignored { params.push(format!("ignored={v}")); }
        if let Some(v) = search { params.push(format!("search={v}")); }
        if let Some(v) = limit { params.push(format!("limit={v}")); }
        if let Some(v) = after { params.push(format!("after={v}")); }
        let qs = if params.is_empty() {
            String::new()
        } else {
            format!("?{}", params.join("&"))
        };
        let resp: serde_json::Value = self.get(&format!("{prefix}/transactions{qs}"))?;
        let data = resp.get("data").cloned().unwrap_or_default();
        let next_cursor = resp.get("next_cursor").and_then(|c| c.as_str()).map(String::from);
        Ok((serde_json::from_value(data)?, next_cursor))
    }

    pub fn transaction(&self, id: &str) -> Result<Transaction, ApiError> {
        let prefix = self.space_prefix()?;
        let resp: serde_json::Value = self.get(&format!("{prefix}/transactions/{id}"))?;
        let data = resp.get("data").cloned().unwrap_or(resp);
        Ok(serde_json::from_value(data)?)
    }

    // ── Private HTTP helpers ──

    fn post(&self, path: &str, body: &serde_json::Value, idempotency_key: Option<&str>) -> Result<serde_json::Value, ApiError> {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.credential.bearer_token()))
            .header("Accept", "application/json")
            .json(body);
        if let Some(key) = idempotency_key {
            req = req.header("Idempotency-Key", key);
        }
        let resp = req.send()?;

        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body: serde_json::Value = resp.json().unwrap_or_default();
            return Err(ApiError::from_body(status, &body));
        }

        let text = resp.text()?;
        if text.trim().is_empty() {
            Ok(serde_json::Value::Null)
        } else {
            Ok(serde_json::from_str(&text)?)
        }
    }

    fn patch(&self, path: &str, body: &serde_json::Value) -> Result<serde_json::Value, ApiError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .patch(&url)
            .header("Authorization", format!("Bearer {}", self.credential.bearer_token()))
            .header("Accept", "application/json")
            .json(body)
            .send()?;

        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body: serde_json::Value = resp.json().unwrap_or_default();
            return Err(ApiError::from_body(status, &body));
        }

        let text = resp.text()?;
        if text.trim().is_empty() {
            Ok(serde_json::Value::Null)
        } else {
            Ok(serde_json::from_str(&text)?)
        }
    }

    fn delete(&self, path: &str) -> Result<(), ApiError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .delete(&url)
            .header("Authorization", format!("Bearer {}", self.credential.bearer_token()))
            .header("Accept", "application/json")
            .send()?;

        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body: serde_json::Value = resp.json().unwrap_or_default();
            return Err(ApiError::from_body(status, &body));
        }

        Ok(())
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
        let prefix = self.space_prefix()?;
        let idempotency_key = Uuid::new_v4().to_string();

        let mut body = serde_json::Map::new();
        body.insert("amount".into(), serde_json::Value::String(amount.to_string()));
        body.insert("description".into(), serde_json::Value::String(description.to_string()));
        body.insert("type".into(), serde_json::Value::String(tx_type.to_string()));
        body.insert("account_id".into(), serde_json::Value::String(account_id.to_string()));
        if let Some(cat) = category_id {
            body.insert("category_id".into(), serde_json::Value::String(cat.to_string()));
        }
        if !tag_ids.is_empty() {
            body.insert("tag_ids".into(), serde_json::Value::Array(
                tag_ids.iter().map(|t| serde_json::Value::String(t.clone())).collect(),
            ));
        }
        if let Some(merchant) = merchant_name {
            body.insert("merchant_name".into(), serde_json::Value::String(merchant.to_string()));
        }
        if let Some(n) = notes {
            body.insert("notes".into(), serde_json::Value::String(n.to_string()));
        }
        if let Some(at) = transacted_at {
            body.insert("transacted_at".into(), serde_json::Value::String(at.to_string()));
        }
        if ignored {
            body.insert("ignored".into(), serde_json::Value::Bool(true));
        }

        let resp = self.post(
            &format!("{prefix}/transactions"),
            &serde_json::Value::Object(body),
            Some(&idempotency_key),
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
        let prefix = self.space_prefix()?;
        let mut body = serde_json::Map::new();

        if let Some(v) = amount {
            body.insert("amount".into(), serde_json::Value::String(v.to_string()));
        }
        if let Some(v) = description {
            body.insert("description".into(), serde_json::Value::String(v.to_string()));
        }
        if let Some(v) = tx_type {
            body.insert("type".into(), serde_json::Value::String(v.to_string()));
        }
        if let Some(v) = category_id {
            body.insert("category_id".into(), serde_json::Value::String(v.to_string()));
        }
        if clear_tags {
            body.insert("tag_ids".into(), serde_json::Value::Array(vec![]));
        } else if let Some(tags) = tag_ids {
            body.insert("tag_ids".into(), serde_json::Value::Array(
                tags.iter().map(|t| serde_json::Value::String(t.clone())).collect(),
            ));
        }
        if let Some(v) = merchant_name {
            body.insert("merchant_name".into(), serde_json::Value::String(v.to_string()));
        }
        if let Some(v) = notes {
            body.insert("notes".into(), serde_json::Value::String(v.to_string()));
        }
        if let Some(v) = transacted_at {
            body.insert("transacted_at".into(), serde_json::Value::String(v.to_string()));
        }
        if let Some(v) = ignored {
            body.insert("ignored".into(), serde_json::Value::Bool(v));
        }

        let resp = self.patch(
            &format!("{prefix}/transactions/{id}"),
            &serde_json::Value::Object(body),
        )?;
        let data = resp.get("data").cloned().unwrap_or(resp);
        Ok(serde_json::from_value(data)?)
    }

    pub fn delete_transaction(&self, id: &str) -> Result<(), ApiError> {
        let prefix = self.space_prefix()?;
        self.delete(&format!("{prefix}/transactions/{id}"))
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

    pub fn update_tag(&self, id: &str, name: Option<&str>, color: Option<&str>) -> Result<Tag, ApiError> {
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
        self.delete(&format!("{prefix}/tags/{id}"))
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
        body.insert("type".into(), serde_json::Value::String(category_type.to_string()));
        if let Some(pid) = parent_id {
            body.insert("parent_id".into(), serde_json::Value::String(pid.to_string()));
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
            body.insert("parent_id".into(), serde_json::Value::String(pid.to_string()));
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
        self.delete(&format!("{prefix}/categories/{id}"))
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
        body.insert("target_amount".into(), serde_json::Value::String(target_amount.to_string()));
        if let Some(k) = kind {
            body.insert("kind".into(), serde_json::Value::String(k.to_string()));
        }
        if let Some(d) = target_date {
            body.insert("target_date".into(), serde_json::Value::String(d.to_string()));
        }
        if let Some(c) = currency_code {
            body.insert("currency_code".into(), serde_json::Value::String(c.to_string()));
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
            body.insert("target_amount".into(), serde_json::Value::String(t.to_string()));
        }
        if let Some(s) = status {
            body.insert("status".into(), serde_json::Value::String(s.to_string()));
        }
        if let Some(d) = target_date {
            body.insert("target_date".into(), serde_json::Value::String(d.to_string()));
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
        self.delete(&format!("{prefix}/goals/{id}"))
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
        body.insert("amount".into(), serde_json::Value::String(amount.to_string()));
        body.insert("frequency".into(), serde_json::Value::String(frequency.to_string()));
        if let Some(a) = account_id {
            body.insert("account_id".into(), serde_json::Value::String(a.to_string()));
        }
        if let Some(c) = category_id {
            body.insert("category_id".into(), serde_json::Value::String(c.to_string()));
        }
        if let Some(s) = service_name {
            body.insert("service_name".into(), serde_json::Value::String(s.to_string()));
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
        self.delete(&format!("{prefix}/subscriptions/{id}"))
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
        body.insert("amount".into(), serde_json::Value::String(amount.to_string()));
        body.insert("frequency".into(), serde_json::Value::String(frequency.to_string()));
        if let Some(a) = account_id {
            body.insert("account_id".into(), serde_json::Value::String(a.to_string()));
        }
        if let Some(c) = category_id {
            body.insert("category_id".into(), serde_json::Value::String(c.to_string()));
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
        self.delete(&format!("{prefix}/fixed-bills/{id}"))
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
        if let Some(v) = from_date { params.push(format!("from_date={v}")); }
        if let Some(v) = to_date { params.push(format!("to_date={v}")); }
        if let Some(v) = account_id { params.push(format!("account_id={v}")); }
        if let Some(v) = category_id { params.push(format!("category_id={v}")); }
        if let Some(v) = tag_id { params.push(format!("tag_id={v}")); }
        if let Some(v) = tx_type { params.push(format!("type={v}")); }
        if let Some(v) = ignored { params.push(format!("ignored={v}")); }
        if let Some(v) = search { params.push(format!("search={v}")); }
        if let Some(v) = limit { params.push(format!("limit={v}")); }
        if let Some(v) = after { params.push(format!("after={v}")); }
        let qs = if params.is_empty() {
            String::new()
        } else {
            format!("?{}", params.join("&"))
        };

        let url = format!("{}{}/transactions{}", self.base_url, prefix, qs);
        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.credential.bearer_token()))
            .header("Accept", "text/csv")
            .send()?;

        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body: serde_json::Value = resp.json().unwrap_or_default();
            return Err(ApiError::from_body(status, &body));
        }

        Ok(resp.text()?)
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
            .create_transaction("42.50", "Coffee", "expense", "acc-1", None, &[], None, None, None, false)
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
            .create_transaction("", "", "expense", "acc-1", None, &[], None, None, None, false)
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
        let body = json!({"errors": [{"code": "IMMUTABLE_PROVIDER_FIELD", "message": "Cannot edit"}]});
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
        assert_eq!(fields[0], ("amount".to_string(), "must be positive".to_string()));
        assert_eq!(fields[1], ("description".to_string(), "is required".to_string()));
    }

    #[test]
    fn test_field_errors_empty_for_non_validation() {
        let body = json!({"errors": [{"code": "NOT_FOUND", "message": "Not found"}]});
        let err = ApiError::from_body(404, &body);
        assert!(err.field_errors().is_empty());
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
}