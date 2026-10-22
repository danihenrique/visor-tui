use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Me {
    pub id: String,
    pub name: String,
    pub email: String,
    pub locale: Option<String>,
    pub display_currency_code: Option<String>,
    pub plan: Option<String>,
    pub active_space_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Space {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub space_type: Option<String>,
    #[serde(default)]
    pub default: bool,
    pub role: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub account_type: Option<String>,
    pub source: Option<String>,
    pub balance: Option<String>,
    pub currency_code: Option<String>,
    #[serde(default)]
    pub hidden: bool,
    pub provider_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: String,
    pub account_id: String,
    pub category_id: Option<String>,
    #[serde(default)]
    pub tag_ids: Vec<String>,
    pub amount: String,
    #[serde(rename = "type")]
    pub tx_type: String,
    pub currency_code: Option<String>,
    pub source: Option<String>,
    pub status: Option<String>,
    #[serde(default)]
    pub ignored: bool,
    pub description: Option<String>,
    pub merchant_name: Option<String>,
    pub notes: Option<String>,
    pub transacted_at: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    pub id: String,
    pub key: Option<String>,
    pub name: String,
    pub parent_id: Option<String>,
    #[serde(rename = "type")]
    pub category_type: Option<String>,
    pub kind: Option<String>,
    #[serde(default)]
    pub hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Card {
    pub id: String,
    pub name: String,
    pub provider_name: Option<String>,
    pub currency_code: Option<String>,
    pub credit_limit: Option<String>,
    pub used_limit: Option<String>,
    pub available_limit: Option<String>,
    pub current_bill_close_date: Option<String>,
    pub current_bill_due_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bill {
    pub id: Option<String>,
    pub status: Option<String>,
    pub close_date: Option<String>,
    pub due_date: Option<String>,
    pub total_amount: Option<String>,
    pub paid_amount: Option<String>,
    pub minimum_payment: Option<String>,
    #[serde(default)]
    pub amount_is_estimate: bool,
    pub currency_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub id: String,
    pub name: String,
    pub kind: Option<String>,
    pub status: Option<String>,
    pub target_amount: Option<String>,
    pub current_amount: Option<String>,
    pub currency_code: Option<String>,
    pub progress_percent: Option<f64>,
    pub target_date: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Investment {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub investment_type: Option<String>,
    pub subtype: Option<String>,
    pub category: Option<String>,
    pub source: Option<String>,
    pub balance: Option<String>,
    pub currency_code: Option<String>,
    pub provider_name: Option<String>,
    #[serde(default)]
    pub contributes_to_net_worth: bool,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvestmentSummary {
    pub total_value: Option<String>,
    pub currency_code: Option<String>,
    pub count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecurringExpense {
    pub id: String,
    pub name: String,
    pub amount: Option<String>,
    pub currency_code: Option<String>,
    pub frequency: Option<String>,
    pub next_due_date: Option<String>,
    pub last_paid_date: Option<String>,
    #[serde(default)]
    pub active: bool,
    pub category_id: Option<String>,
    pub account_id: Option<String>,
    pub service_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallmentPlan {
    pub id: String,
    pub description: Option<String>,
    pub account_id: Option<String>,
    pub installment_amount: Option<String>,
    pub total_amount: Option<String>,
    pub remaining_amount: Option<String>,
    pub currency_code: Option<String>,
    pub total_installments: Option<u32>,
    pub paid_installments: Option<u32>,
    pub first_due_date: Option<String>,
    pub next_due_date: Option<String>,
    pub status: Option<String>,
    pub category_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub id: String,
    pub provider_name: Option<String>,
    pub status: Option<String>,
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub requires_reauth: bool,
    pub last_successful_sync_at: Option<String>,
    pub next_auto_sync_at: Option<String>,
    #[serde(default)]
    pub account_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: String,
    pub topic: Option<String>,
    pub title: Option<String>,
    pub body: Option<String>,
    #[serde(default)]
    pub read: bool,
    pub read_at: Option<String>,
    pub space_id: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Overview {
    pub period_start: Option<String>,
    pub period_end: Option<String>,
    pub currency_code: Option<String>,
    pub bank_balance: Option<String>,
    pub credit_card_debt: Option<String>,
    pub open_bills_total: Option<String>,
    pub net_worth: Option<String>,
    #[serde(default)]
    pub upcoming_bills: Vec<OverviewUpcomingBill>,
    pub period: Option<OverviewPeriod>,
    pub projection: Option<OverviewProjection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverviewPeriod {
    pub income: Option<String>,
    pub expenses: Option<String>,
    pub result: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverviewProjection {
    pub current_balance: Option<String>,
    pub expected_income: Option<String>,
    pub committed_outflows: Option<OverviewCommittedOutflows>,
    pub estimated_day_to_day: Option<String>,
    pub projected_balance: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverviewCommittedOutflows {
    pub bills: Option<String>,
    pub installments: Option<String>,
    pub subscriptions: Option<String>,
    pub scheduled: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverviewUpcomingBill {
    pub account_id: Option<String>,
    pub description: Option<String>,
    pub amount: Option<String>,
    pub due_date: Option<String>,
    #[serde(default)]
    pub amount_is_estimate: bool,
}

/// Wrapper for list responses that include pagination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResponse<T> {
    pub data: Vec<T>,
    pub next_cursor: Option<String>,
}

/// Wrapper for single-resource responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataResponse<T> {
    pub data: T,
}