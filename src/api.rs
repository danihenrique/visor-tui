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
            let request_id = errors
                .and_then(|arr| arr.first())
                .and_then(|e| e.get("request_id"))
                .and_then(|r| r.as_str())
                .map(String::from);
            return Err(ApiError::Api { status, code, message, request_id });
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
            let request_id = errors
                .and_then(|arr| arr.first())
                .and_then(|e| e.get("request_id"))
                .and_then(|r| r.as_str())
                .map(String::from);
            return Err(ApiError::Api { status, code, message, request_id });
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
            let request_id = errors
                .and_then(|arr| arr.first())
                .and_then(|e| e.get("request_id"))
                .and_then(|r| r.as_str())
                .map(String::from);
            return Err(ApiError::Api { status, code, message, request_id });
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
            let request_id = errors
                .and_then(|arr| arr.first())
                .and_then(|e| e.get("request_id"))
                .and_then(|r| r.as_str())
                .map(String::from);
            return Err(ApiError::Api { status, code, message, request_id });
        }

        Ok(resp.text()?)
    }
}