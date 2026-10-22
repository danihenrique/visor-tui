use std::collections::BTreeMap;

use chrono::{NaiveDate, TimeZone};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::api::ApiError;
use crate::models::Transaction;

pub use crate::api::{IgnoredFilter, TransactionFilters, TxTypeFilter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FormField {
    Amount,
    Description,
    Type,
    Account,
    Category,
    Tags,
    Merchant,
    Notes,
    Date,
    Ignored,
}

impl FormField {
    pub const ALL: [Self; 10] = [
        Self::Amount,
        Self::Description,
        Self::Type,
        Self::Account,
        Self::Category,
        Self::Tags,
        Self::Merchant,
        Self::Notes,
        Self::Date,
        Self::Ignored,
    ];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Amount => "Valor (R$)",
            Self::Description => "Descrição",
            Self::Type => "Tipo",
            Self::Account => "Conta",
            Self::Category => "Categoria",
            Self::Tags => "Tags",
            Self::Merchant => "Estabelecimento",
            Self::Notes => "Notas",
            Self::Date => "Data",
            Self::Ignored => "Ignorado",
        }
    }

    #[must_use]
    pub fn api_name(self) -> &'static str {
        match self {
            Self::Amount => "amount",
            Self::Description => "description",
            Self::Type => "type",
            Self::Account => "account_id",
            Self::Category => "category_id",
            Self::Tags => "tag_ids",
            Self::Merchant => "merchant_name",
            Self::Notes => "notes",
            Self::Date => "transacted_at",
            Self::Ignored => "ignored",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TransactionForm {
    pub original: Option<Transaction>,
    pub create_key: String,
    pub pending_create: Option<Value>,
    pub amount: String,
    pub description: String,
    pub tx_type: TxTypeFilter,
    pub account_id: Option<String>,
    pub category_id: Option<String>,
    pub tag_ids: Vec<String>,
    pub merchant: String,
    pub notes: String,
    pub date: String,
    pub ignored: bool,
    pub focused: FormField,
    pub errors: BTreeMap<FormField, String>,
    pub error: Option<String>,
    pub picker: Option<Picker>,
}

impl TransactionForm {
    pub fn new() -> Self {
        Self {
            original: None,
            create_key: Uuid::new_v4().to_string(),
            pending_create: None,
            amount: String::new(),
            description: String::new(),
            tx_type: TxTypeFilter::Debit,
            account_id: None,
            category_id: None,
            tag_ids: Vec::new(),
            merchant: String::new(),
            notes: String::new(),
            date: chrono::Local::now().format("%Y-%m-%d").to_string(),
            ignored: false,
            focused: FormField::Amount,
            errors: BTreeMap::new(),
            error: None,
            picker: None,
        }
    }

    pub fn edit(tx: Transaction) -> Self {
        let mut form = Self {
            amount: tx.amount.clone(),
            description: tx.description.clone().unwrap_or_default(),
            tx_type: if tx.tx_type.eq_ignore_ascii_case("credit") {
                TxTypeFilter::Credit
            } else {
                TxTypeFilter::Debit
            },
            account_id: Some(tx.account_id.clone()),
            category_id: tx.category_id.clone(),
            tag_ids: tx.tag_ids.clone(),
            merchant: tx.merchant_name.clone().unwrap_or_default(),
            notes: tx.notes.clone().unwrap_or_default(),
            date: tx
                .transacted_at
                .as_deref()
                .unwrap_or("")
                .split('T')
                .next()
                .unwrap_or("")
                .to_string(),
            ignored: tx.ignored,
            original: Some(tx),
            ..Self::new()
        };
        if !form.editable(form.focused) {
            form.focused = FormField::Category;
        }
        form
    }

    pub fn connected(&self) -> bool {
        self.original
            .as_ref()
            .and_then(|tx| tx.source.as_deref())
            .is_some_and(|source| {
                source.eq_ignore_ascii_case("connected") || source.eq_ignore_ascii_case("apple_pay")
            })
    }

    pub fn editable(&self, field: FormField) -> bool {
        if matches!(field, FormField::Account | FormField::Merchant) && self.original.is_some() {
            return false;
        }
        !(self.connected()
            && matches!(
                field,
                FormField::Amount | FormField::Description | FormField::Type | FormField::Date
            ))
    }

    pub fn move_focus(&mut self, backwards: bool) {
        let mut index = FormField::ALL
            .iter()
            .position(|f| *f == self.focused)
            .unwrap_or(0);
        loop {
            index = if backwards {
                (index + 9) % 10
            } else {
                (index + 1) % 10
            };
            if self.editable(FormField::ALL[index]) {
                self.focused = FormField::ALL[index];
                break;
            }
        }
    }

    pub fn text_mut(&mut self) -> Option<&mut String> {
        if !self.editable(self.focused) {
            return None;
        }
        match self.focused {
            FormField::Amount => Some(&mut self.amount),
            FormField::Description => Some(&mut self.description),
            FormField::Merchant => Some(&mut self.merchant),
            FormField::Notes => Some(&mut self.notes),
            FormField::Date => Some(&mut self.date),
            _ => None,
        }
    }

    pub fn payload(&mut self) -> Option<Value> {
        self.errors.clear();
        self.error = None;
        let mut body = json!({ "tag_ids": self.tag_ids, "ignored": self.ignored });
        if self
            .original
            .as_ref()
            .is_none_or(|tx| tx.notes.as_deref().unwrap_or_default() != self.notes)
        {
            body["notes"] = json!(self.notes.trim());
        }
        if let Some(id) = &self.category_id {
            body["category_id"] = json!(id);
        } else if self.original.is_none() {
            self.errors
                .insert(FormField::Category, "Selecione uma categoria.".into());
        }
        if self.notes.chars().count() > 500 {
            self.errors
                .insert(FormField::Notes, "Use até 500 caracteres.".into());
        }
        if self.tag_ids.len() > 20 {
            self.errors
                .insert(FormField::Tags, "Selecione até 20 tags.".into());
        }
        if !self.connected() {
            match parse_amount(&self.amount) {
                Ok(amount) => body["amount"] = json!(amount),
                Err(message) => {
                    self.errors.insert(FormField::Amount, message.into());
                }
            }
            match parse_date(&self.date) {
                Ok(date) => {
                    let original = self
                        .original
                        .as_ref()
                        .and_then(|tx| tx.transacted_at.as_deref());
                    if original.is_none_or(|value| !value.starts_with(&date.to_string())) {
                        let local = date
                            .and_hms_opt(12, 0, 0)
                            .and_then(|date| chrono::Local.from_local_datetime(&date).single());
                        if let Some(local) = local {
                            body["transacted_at"] = json!(local.to_rfc3339());
                        } else {
                            self.errors
                                .insert(FormField::Date, "Data indisponível no fuso local.".into());
                        }
                    }
                }
                Err(message) => {
                    self.errors.insert(FormField::Date, message.into());
                }
            }
            if self.description.trim().is_empty() || self.description.chars().count() > 255 {
                self.errors.insert(
                    FormField::Description,
                    "Informe uma descrição de 1 a 255 caracteres.".into(),
                );
            }
            if self
                .original
                .as_ref()
                .is_none_or(|tx| tx.description.as_deref().unwrap_or_default() != self.description)
            {
                body["description"] = json!(self.description.trim());
            }
            body["type"] = json!(self.tx_type.api_value().unwrap_or("debit"));
        }
        if self.original.is_none() {
            if self.merchant.chars().count() > 255 {
                self.errors
                    .insert(FormField::Merchant, "Use até 255 caracteres.".into());
            }
            body["merchant_name"] = json!(self.merchant.trim());
            if let Some(id) = &self.account_id {
                body["account_id"] = json!(id);
            } else {
                self.errors
                    .insert(FormField::Account, "Selecione uma conta.".into());
            }
        }
        if let Some(field) = self.errors.keys().next().copied() {
            self.focused = field;
            None
        } else {
            Some(body)
        }
    }

    pub fn api_error(&mut self, error: &ApiError) {
        self.error = Some(error.to_string());
        if let ApiError::Api(data) = error {
            collect_field_errors(&data.details, &mut self.errors);
            if let Some(field) = self.errors.keys().next().copied() {
                self.focused = field;
            }
        }
    }
}

fn collect_field_errors(value: &Value, errors: &mut BTreeMap<FormField, String>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_field_errors(item, errors);
            }
        }
        Value::Object(object) => {
            let path = object.get("field").or_else(|| object.get("path"));
            if let (Some(path), Some(message)) =
                (path, object.get("message").and_then(Value::as_str))
            {
                let name = path.as_str().map(str::to_string).or_else(|| {
                    path.as_array().map(|p| {
                        p.iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(".")
                    })
                });
                if let Some(name) = name {
                    for field in FormField::ALL {
                        if name.split('.').any(|part| part == field.api_name()) {
                            errors.insert(field, message.into());
                        }
                    }
                }
            }
            for (key, value) in object {
                if let Some(field) = FormField::ALL.iter().find(|field| field.api_name() == key) {
                    let message = value.as_str().map(str::to_string).or_else(|| {
                        value.as_array().map(|items| {
                            items
                                .iter()
                                .filter_map(Value::as_str)
                                .collect::<Vec<_>>()
                                .join("; ")
                        })
                    });
                    if let Some(message) = message.filter(|text| !text.is_empty()) {
                        errors.insert(*field, message);
                    }
                }
                collect_field_errors(value, errors);
            }
        }
        _ => {}
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterField {
    Account,
    Category,
    Tag,
    Type,
    Ignored,
    Search,
}

impl FilterField {
    pub const ALL: [Self; 6] = [
        Self::Account,
        Self::Category,
        Self::Tag,
        Self::Type,
        Self::Ignored,
        Self::Search,
    ];
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Account => "Conta",
            Self::Category => "Categoria",
            Self::Tag => "Tag",
            Self::Type => "Tipo",
            Self::Ignored => "Ignorados",
            Self::Search => "Busca",
        }
    }
}

#[derive(Debug, Clone)]
pub struct FilterForm {
    pub draft: TransactionFilters,
    pub focused: FilterField,
    pub picker: Option<Picker>,
    pub error: Option<String>,
}

impl FilterForm {
    pub fn new(draft: TransactionFilters) -> Self {
        Self {
            draft,
            focused: FilterField::Account,
            picker: None,
            error: None,
        }
    }
    pub fn move_focus(&mut self, backwards: bool) {
        let index = FilterField::ALL
            .iter()
            .position(|f| *f == self.focused)
            .unwrap_or(0);
        self.focused = FilterField::ALL[if backwards {
            (index + 5) % 6
        } else {
            (index + 1) % 6
        }];
    }
    pub fn text_mut(&mut self) -> Option<&mut String> {
        match self.focused {
            FilterField::Search => Some(&mut self.draft.search),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerKind {
    Account,
    Category,
    Tags,
}

#[derive(Debug, Clone)]
pub struct Picker {
    pub kind: PickerKind,
    pub query: String,
    pub selected: usize,
}

impl Picker {
    pub fn new(kind: PickerKind) -> Self {
        Self {
            kind,
            query: String::new(),
            selected: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Choice {
    pub id: Option<String>,
    pub label: String,
}

pub fn parse_amount(input: &str) -> Result<String, &'static str> {
    let value = input
        .trim()
        .strip_prefix("R$")
        .unwrap_or(input.trim())
        .trim();
    let normalized = if value.contains(',') {
        let (whole, fraction) = value.split_once(',').ok_or("Valor inválido.")?;
        let groups: Vec<&str> = whole.split('.').collect();
        if groups.len() > 1
            && (groups[0].is_empty()
                || groups[0].len() > 3
                || groups[1..].iter().any(|group| group.len() != 3))
        {
            return Err("Use 1.234,56 ou 1234.56.");
        }
        format!("{}.{}", groups.join(""), fraction)
    } else {
        value.to_string()
    };
    let mut pieces = normalized.split('.');
    let whole = pieces.next().unwrap_or("");
    let fraction = pieces.next().unwrap_or("");
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > 2
        || !fraction.bytes().all(|b| b.is_ascii_digit())
        || pieces.next().is_some()
    {
        return Err("Informe um valor positivo, com até 2 casas decimais.");
    }
    let whole = whole.trim_start_matches('0');
    let whole = if whole.is_empty() { "0" } else { whole };
    if whole.len() > 13 {
        return Err("Use até 13 dígitos antes dos centavos.");
    }
    let fraction = format!("{fraction:0<2}");
    if whole == "0" && fraction == "00" {
        return Err("O valor deve ser maior que zero.");
    }
    Ok(format!("{whole}.{fraction}"))
}

pub fn parse_date(input: &str) -> Result<NaiveDate, &'static str> {
    let input = input.trim();
    let format = if input.len() == 10 && input.as_bytes()[4] == b'-' {
        "%Y-%m-%d"
    } else {
        "%d/%m/%Y"
    };
    if input.len() != 10 {
        return Err("Use AAAA-MM-DD ou DD/MM/AAAA.");
    }
    NaiveDate::parse_from_str(input, format)
        .map_err(|_| "Data inválida. Use AAAA-MM-DD ou DD/MM/AAAA.")
}

pub fn format_amount(amount: &str, tx_type: &str) -> String {
    let absolute = amount.trim().trim_start_matches(['-', '+']);
    let normalized = if matches!(absolute, "0" | "0.0" | "0.00") {
        "0.00".to_string()
    } else if let Ok(value) = parse_amount(absolute) {
        value
    } else {
        return amount.to_string();
    };
    let (whole, fraction) = normalized.split_once('.').unwrap_or((&normalized, "00"));
    let grouped: String = whole
        .chars()
        .enumerate()
        .flat_map(|(index, ch)| {
            if index > 0 && (whole.len() - index).is_multiple_of(3) {
                vec!['.', ch]
            } else {
                vec![ch]
            }
        })
        .collect();
    let sign = if tx_type.eq_ignore_ascii_case("credit") {
        "+"
    } else {
        "-"
    };
    format!("{sign}R$ {grouped},{fraction}")
}
