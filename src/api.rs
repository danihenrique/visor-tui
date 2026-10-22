use reqwest::blocking::Client;
use serde::de::DeserializeOwned;

use crate::auth::Credential;
use crate::config;
use crate::models::*;

/// API client that reuses visor-cli's auth and config mechanisms.
pub struct ApiClient {
    client: Client,
    base_url: String,
    credential: Credential,
    space_id: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error {status}: {code} — {message}")]
    Api {
        status: u16,
        code: String,
        message: String,
        request_id: Option<String>,
    },
    #[error("No credential configured")]
    NoCredential,
    #[error("No active space configured")]
    NoSpace,
    #[error("JSON decode error: {0}")]
    Json(#[from] serde_json::Error),
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
            let errors = body.get("errors").and_then(|e| e.as_array());
            let (code, message) = errors
                .and_then(|arr| arr.first())
                .map(|e| {
                    (
                        e.get("code").and_then(|c| c.as_str()).unwrap_or("UNKNOWN").to_string(),
                        e.get("message").and_then(|m| m.as_str()).unwrap_or("").to_string(),
                    )
                })
                .unwrap_or_else(|| ("UNKNOWN".to_string(), format!("HTTP {status}")));
            let request_id = body
                .get("errors")
                .and_then(|e| e.as_array())
                .and_then(|arr| arr.first())
                .and_then(|e| e.get("request_id"))
                .and_then(|r| r.as_str())
                .map(String::from);
            return Err(ApiError::Api {
                status,
                code,
                message,
                request_id,
            });
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
}