use hqtui::prelude::*;
use hqtui::color::Color;
use hqtui::input::InputEvent;
use hqtui::widgets::{
    ButtonOptions, ButtonVariant, ModalButton, ModalOptions, SelectOptions,
    TableOptions, TableColumn, TableRow, TextInputOptions,
};
use hqtui::unicode::Align;

use crate::api::{ApiClient, ApiError};
use crate::models::{Account, Category, Tag, Transaction};

const COLOR_CREDIT: Color = Color::rgb(0x22, 0xc5, 0x5e);
const COLOR_DEBIT: Color = Color::rgb(0xef, 0x44, 0x44);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxTypeFilter {
    All,
    Credit,
    Debit,
}

impl TxTypeFilter {
    fn label(self) -> &'static str {
        match self {
            Self::All => "Todos",
            Self::Credit => "Receitas",
            Self::Debit => "Despesas",
        }
    }

    fn api_value(self) -> Option<&'static str> {
        match self {
            Self::All => None,
            Self::Credit => Some("credit"),
            Self::Debit => Some("debit"),
        }
    }

    fn next(self) -> Self {
        match self {
            Self::All => Self::Credit,
            Self::Credit => Self::Debit,
            Self::Debit => Self::All,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgnoredFilter {
    All,
    OnlyIgnored,
    NotIgnored,
}

impl IgnoredFilter {
    fn label(self) -> &'static str {
        match self {
            Self::All => "Todos",
            Self::OnlyIgnored => "Ignorados",
            Self::NotIgnored => "Nao ignorados",
        }
    }

    fn api_value(self) -> Option<bool> {
        match self {
            Self::All => None,
            Self::OnlyIgnored => Some(true),
            Self::NotIgnored => Some(false),
        }
    }

    fn next(self) -> Self {
        match self {
            Self::All => Self::OnlyIgnored,
            Self::OnlyIgnored => Self::NotIgnored,
            Self::NotIgnored => Self::All,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalState {
    None,
    Detail,
    NewTransaction,
    EditTransaction,
    DeleteConfirm,
    ExportSuccess,
}

/// Which form field is focused inside the `NewTransaction` modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    fn next(self) -> Self {
        match self {
            Self::Amount => Self::Description,
            Self::Description => Self::Type,
            Self::Type => Self::Account,
            Self::Account => Self::Category,
            Self::Category => Self::Tags,
            Self::Tags => Self::Merchant,
            Self::Merchant => Self::Notes,
            Self::Notes => Self::Date,
            Self::Date => Self::Ignored,
            Self::Ignored => Self::Amount,
        }
    }

    fn prev(self) -> Self {
        match self {
            Self::Amount => Self::Ignored,
            Self::Description => Self::Amount,
            Self::Type => Self::Description,
            Self::Account => Self::Type,
            Self::Category => Self::Account,
            Self::Tags => Self::Category,
            Self::Merchant => Self::Tags,
            Self::Notes => Self::Merchant,
            Self::Date => Self::Notes,
            Self::Ignored => Self::Date,
        }
    }
}

// This struct holds all UI state for the transactions screen including filter,
// form, and export state. The bool count reflects independent UI flags that
// cannot be meaningfully grouped without adding indirection.
#[allow(clippy::struct_excessive_bools)]
pub struct TransactionsScreen {
    pub transactions: Vec<Transaction>,
    pub next_cursor: Option<String>,
    pub loading: bool,
    pub error: Option<String>,
    pub selected_index: Option<usize>,
    pub scroll_offset: usize,
    pub search_query: String,
    pub date_from: String,
    pub date_to: String,
    pub type_filter: TxTypeFilter,
    pub ignored_filter: IgnoredFilter,
    pub account_filter_index: usize,
    pub category_filter_index: usize,
    pub tag_filter_index: usize,
    pub accounts: Vec<Account>,
    pub categories: Vec<Category>,
    pub tags: Vec<Tag>,
    pub modal_state: ModalState,
    pub detail_transaction: Option<Transaction>,
    pub delete_target_id: Option<String>,
    pub loaded: bool,

    // New transaction form state
    pub new_tx_amount: String,
    pub new_tx_description: String,
    pub new_tx_type: TxTypeFilter,
    pub new_tx_account_index: usize,
    pub new_tx_category_index: usize,
    pub new_tx_tag_indices: Vec<usize>,
    pub new_tx_merchant: String,
    pub new_tx_notes: String,
    pub new_tx_date: String,
    pub new_tx_ignored: bool,
    pub new_tx_error: Option<String>,
    pub new_tx_field: FormField,

    // Edit transaction form state
    pub edit_tx_id: String,
    pub edit_tx_amount: String,
    pub edit_tx_description: String,
    pub edit_tx_type: TxTypeFilter,
    pub edit_tx_category_index: usize,
    pub edit_tx_tag_indices: Vec<usize>,
    pub edit_tx_merchant: String,
    pub edit_tx_notes: String,
    pub edit_tx_date: String,
    pub edit_tx_ignored: bool,
    pub edit_tx_error: Option<String>,
    pub edit_tx_field: FormField,
    pub edit_tx_source: Option<String>,

    // CSV export state
    pub exporting: bool,
    pub export_success_path: Option<String>,
}

impl TransactionsScreen {
    #[must_use]
    pub fn new() -> Self {
        Self {
            transactions: Vec::new(),
            next_cursor: None,
            loading: false,
            error: None,
            selected_index: None,
            scroll_offset: 0,
            search_query: String::new(),
            date_from: String::new(),
            date_to: String::new(),
            type_filter: TxTypeFilter::All,
            ignored_filter: IgnoredFilter::All,
            account_filter_index: 0,
            category_filter_index: 0,
            tag_filter_index: 0,
            accounts: Vec::new(),
            categories: Vec::new(),
            tags: Vec::new(),
            modal_state: ModalState::None,
            detail_transaction: None,
            delete_target_id: None,
            loaded: false,

            // New transaction form defaults
            new_tx_amount: String::new(),
            new_tx_description: String::new(),
            new_tx_type: TxTypeFilter::Credit,
            new_tx_account_index: 0,
            new_tx_category_index: 0,
            new_tx_tag_indices: Vec::new(),
            new_tx_merchant: String::new(),
            new_tx_notes: String::new(),
            new_tx_date: chrono::Local::now().format("%Y-%m-%d").to_string(),
            new_tx_ignored: false,
            new_tx_error: None,
            new_tx_field: FormField::Amount,

            // Edit transaction form defaults
            edit_tx_id: String::new(),
            edit_tx_amount: String::new(),
            edit_tx_description: String::new(),
            edit_tx_type: TxTypeFilter::Credit,
            edit_tx_category_index: 0,
            edit_tx_tag_indices: Vec::new(),
            edit_tx_merchant: String::new(),
            edit_tx_notes: String::new(),
            edit_tx_date: String::new(),
            edit_tx_ignored: false,
            edit_tx_error: None,
            edit_tx_field: FormField::Amount,
            edit_tx_source: None,

            // CSV export defaults
            exporting: false,
            export_success_path: None,
        }
    }

    pub fn load_initial(&mut self, api: &ApiClient) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        self.loading = true;
        self.error = None;

        match api.accounts(false) {
            Ok(accounts) => self.accounts = accounts,
            Err(e) => {
                self.error = Some(format!("Failed to load accounts: {e}"));
            }
        }
        match api.categories(false) {
            Ok(categories) => self.categories = categories,
            Err(e) => {
                self.error = Some(format!("Failed to load categories: {e}"));
            }
        }
        match api.tags() {
            Ok(tags) => self.tags = tags,
            Err(e) => {
                self.error = Some(format!("Failed to load tags: {e}"));
            }
        }

        self.fetch_transactions(api, false);
    }

    pub fn fetch_transactions(&mut self, api: &ApiClient, append: bool) {
        self.loading = true;
        self.error = None;

        let after = if append {
            self.next_cursor.as_deref()
        } else {
            None
        };

        let account_id = if self.account_filter_index > 0
            && self.account_filter_index <= self.accounts.len()
        {
            Some(self.accounts[self.account_filter_index - 1].id.as_str())
        } else {
            None
        };
        let category_id = if self.category_filter_index > 0
            && self.category_filter_index <= self.categories.len()
        {
            Some(self.categories[self.category_filter_index - 1].id.as_str())
        } else {
            None
        };
        let tag_id = if self.tag_filter_index > 0 && self.tag_filter_index <= self.tags.len() {
            Some(self.tags[self.tag_filter_index - 1].id.as_str())
        } else {
            None
        };
        let tx_type = self.type_filter.api_value();
        let search = if self.search_query.is_empty() {
            None
        } else {
            Some(self.search_query.as_str())
        };
        let from_date = if self.date_from.is_empty() {
            None
        } else {
            Some(self.date_from.as_str())
        };
        let to_date = if self.date_to.is_empty() {
            None
        } else {
            Some(self.date_to.as_str())
        };

        let ignored = self.ignored_filter.api_value();

        match api.transactions(
            from_date, to_date, account_id, category_id, tag_id, tx_type, ignored, search,
            Some(50), after,
        ) {
            Ok((txns, cursor)) => {
                if append {
                    self.transactions.extend(txns);
                } else {
                    self.transactions = txns;
                    self.selected_index = None;
                    self.scroll_offset = 0;
                }
                self.next_cursor = cursor;
                self.loading = false;
            }
            Err(e) => {
                self.error = Some(format_api_error(&e));
                self.loading = false;
            }
        }
    }

    pub fn handle_input(&mut self, app: &App, api: &ApiClient, events: &[InputEvent]) {
        if self.modal_state != ModalState::None {
            self.handle_modal_input(app, api, events);
            return;
        }

        if app.pressed("tx:type_toggle") {
            self.type_filter = self.type_filter.next();
            self.fetch_transactions(api, false);
        }

        if app.pressed("tx:ignored_toggle") {
            self.ignored_filter = self.ignored_filter.next();
            self.fetch_transactions(api, false);
        }

        if app.pressed("tx:load_more") && self.next_cursor.is_some() {
            self.fetch_transactions(api, true);
        }

        if app.pressed("tx:new") {
            self.modal_state = ModalState::NewTransaction;
        }

        if app.pressed("tx:refresh") {
            self.fetch_transactions(api, false);
        }

        if app.pressed("tx:search") {
            self.fetch_transactions(api, false);
        }

        if app.pressed("tx:account_filter") {
            self.account_filter_index =
                (self.account_filter_index + 1) % (self.accounts.len() + 1);
            self.fetch_transactions(api, false);
        }
        if app.pressed("tx:category_filter") {
            self.category_filter_index =
                (self.category_filter_index + 1) % (self.categories.len() + 1);
            self.fetch_transactions(api, false);
        }
        if app.pressed("tx:tag_filter") {
            self.tag_filter_index = (self.tag_filter_index + 1) % (self.tags.len() + 1);
            self.fetch_transactions(api, false);
        }

        let scroll_delta = app.scrolled("tx:table");
        if scroll_delta != 0 {
            let max_offset = self.transactions.len().saturating_sub(1);
            if scroll_delta > 0 {
                let delta = usize::try_from(scroll_delta).unwrap_or(0);
                self.scroll_offset =
                    (self.scroll_offset + delta).min(max_offset);
            } else {
                let delta = usize::try_from(-scroll_delta).unwrap_or(0);
                self.scroll_offset =
                    self.scroll_offset.saturating_sub(delta);
            }
            self.selected_index = Some(self.scroll_offset);
        }

        if let Some(row) = app.clicked_row("tx:table") {
            let actual_index = row + self.scroll_offset;
            if actual_index < self.transactions.len() {
                self.selected_index = Some(actual_index);
                let tx_id = &self.transactions[actual_index].id;
                match api.transaction(tx_id) {
                    Ok(tx) => {
                        self.detail_transaction = Some(tx);
                        self.modal_state = ModalState::Detail;
                    }
                    Err(e) => {
                        self.error = Some(format_api_error(&e));
                    }
                }
            }
        }

        if app.pressed("tx:delete")
            && let Some(idx) = self.selected_index
                && idx < self.transactions.len() {
                    self.delete_target_id = Some(self.transactions[idx].id.clone());
                    self.modal_state = ModalState::DeleteConfirm;
                }

        // Edit transaction trigger: 'e' key when a transaction is selected
        // Only trigger if not already exporting (to avoid conflict with export 'e')
        let edit_triggered = events.iter().any(|ev| {
            matches!(ev, InputEvent::Key(k) if k.key == "e" && !k.ctrl && !k.alt && !k.shift)
        });
        if edit_triggered && !self.exporting
            && let Some(idx) = self.selected_index
                && idx < self.transactions.len() {
                    let tx = &self.transactions[idx];
                    self.edit_tx_id = tx.id.clone();
                    self.edit_tx_amount = tx.amount.clone();
                    self.edit_tx_description = tx.description.clone().unwrap_or_default();
                    self.edit_tx_type = match tx.tx_type.as_str() {
                        "debit" => TxTypeFilter::Debit,
                        _ => TxTypeFilter::Credit,
                    };
                    // Find category index
                    self.edit_tx_category_index = if let Some(ref cat_id) = tx.category_id {
                        self.categories.iter().position(|c| &c.id == cat_id).unwrap_or(0)
                    } else {
                        0
                    };
                    self.edit_tx_merchant = tx.merchant_name.clone().unwrap_or_default();
                    self.edit_tx_notes = tx.notes.clone().unwrap_or_default();
                    self.edit_tx_date = tx.transacted_at.clone().unwrap_or_default();
                    self.edit_tx_ignored = tx.ignored;
                    self.edit_tx_error = None;
                    self.edit_tx_field = FormField::Amount;
                    self.edit_tx_source = tx.source.clone();
                    self.modal_state = ModalState::EditTransaction;
                }

        // CSV export trigger: button or 'e' key
        let export_triggered = app.pressed("tx:export")
            || events.iter().any(|ev| {
                matches!(ev, InputEvent::Key(k) if k.key == "e" && !k.ctrl && !k.alt && !k.shift)
            });
        if export_triggered && !self.exporting {
            self.exporting = true;
            self.error = None;

            let account_id = if self.account_filter_index > 0
                && self.account_filter_index <= self.accounts.len()
            {
                Some(self.accounts[self.account_filter_index - 1].id.as_str())
            } else {
                None
            };
            let category_id = if self.category_filter_index > 0
                && self.category_filter_index <= self.categories.len()
            {
                Some(self.categories[self.category_filter_index - 1].id.as_str())
            } else {
                None
            };
            let tag_id = if self.tag_filter_index > 0 && self.tag_filter_index <= self.tags.len() {
                Some(self.tags[self.tag_filter_index - 1].id.as_str())
            } else {
                None
            };
            let tx_type = self.type_filter.api_value();
            let search = if self.search_query.is_empty() {
                None
            } else {
                Some(self.search_query.as_str())
            };
            let from_date = if self.date_from.is_empty() {
                None
            } else {
                Some(self.date_from.as_str())
            };
            let to_date = if self.date_to.is_empty() {
                None
            } else {
                Some(self.date_to.as_str())
            };

            match api.export_transactions_csv(
                from_date, to_date, account_id, category_id, tag_id, tx_type, None, search,
                None, None,
            ) {
                Ok(csv_data) => {
                    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
                    let filename = format!("transactions_{today}.csv");
                    let path = dirs::home_dir().map_or_else(|| std::path::PathBuf::from(&filename), |h| h.join("Downloads").join(&filename));
                    match std::fs::write(&path, csv_data.as_bytes()) {
                        Ok(()) => {
                            self.export_success_path = Some(path.display().to_string());
                            self.modal_state = ModalState::ExportSuccess;
                        }
                        Err(e) => {
                            self.error = Some(format!("Erro ao salvar arquivo: {e}"));
                        }
                    }
                }
                Err(e) => {
                    self.error = Some(format_api_error(&e));
                }
            }
            self.exporting = false;
        }
    }

    fn handle_modal_input(&mut self, app: &App, api: &ApiClient, events: &[InputEvent]) {
        match self.modal_state {
            ModalState::Detail => {
                if app.pressed("tx:detail_close") || app.pressed("tx:escape") {
                    self.modal_state = ModalState::None;
                    self.detail_transaction = None;
                }
            }
            ModalState::NewTransaction => {
                // Handle escape / cancel
                for event in events {
                    if let InputEvent::Key(k) = event {
                        if k.key == "escape" {
                            self.modal_state = ModalState::None;
                            self.new_tx_error = None;
                            return;
                        }
                        // Tab / Shift+Tab to cycle form fields
                        if k.name == "tab" && !k.shift {
                            self.new_tx_field = self.new_tx_field.next();
                            return;
                        }
                        if k.name == "tab" && k.shift {
                            self.new_tx_field = self.new_tx_field.prev();
                            return;
                        }
                        // Type toggle on Enter when Type field is focused
                        if k.name == "enter" && self.new_tx_field == FormField::Type {
                            self.new_tx_type = match self.new_tx_type {
                                TxTypeFilter::Credit => TxTypeFilter::Debit,
                                _ => TxTypeFilter::Credit,
                            };
                            return;
                        }
                        // Account / Category select cycling on Enter
                        if k.name == "enter" && self.new_tx_field == FormField::Account {
                            if !self.accounts.is_empty() {
                                self.new_tx_account_index =
                                    (self.new_tx_account_index + 1) % self.accounts.len();
                            }
                            return;
                        }
                        if k.name == "enter" && self.new_tx_field == FormField::Category {
                            if !self.categories.is_empty() {
                                self.new_tx_category_index =
                                    (self.new_tx_category_index + 1) % self.categories.len();
                            }
                            return;
                        }
                        // Tag multi-select: Enter cycles through tags, toggling selection
                        if k.name == "enter" && self.new_tx_field == FormField::Tags {
                            if !self.tags.is_empty() {
                                // Find next unselected tag index to add, or remove first selected
                                let mut found_unselected = false;
                                for i in 0..self.tags.len() {
                                    if !self.new_tx_tag_indices.contains(&i) {
                                        self.new_tx_tag_indices.push(i);
                                        found_unselected = true;
                                        break;
                                    }
                                }
                                if !found_unselected {
                                    // All selected, clear all
                                    self.new_tx_tag_indices.clear();
                                }
                            }
                            return;
                        }
                        // Ignored toggle on Enter
                        if k.name == "enter" && self.new_tx_field == FormField::Ignored {
                            self.new_tx_ignored = !self.new_tx_ignored;
                            return;
                        }
                        // Text input for the currently focused text field
                        if let Some(ch) = &k.char
                            && !k.ctrl && !k.alt {
                                match self.new_tx_field {
                                    FormField::Amount => self.new_tx_amount.push_str(ch),
                                    FormField::Description => self.new_tx_description.push_str(ch),
                                    FormField::Merchant => self.new_tx_merchant.push_str(ch),
                                    FormField::Notes => self.new_tx_notes.push_str(ch),
                                    FormField::Date => self.new_tx_date.push_str(ch),
                                    _ => {}
                                }
                                return;
                            }
                        if k.key == "backspace" {
                            match self.new_tx_field {
                                FormField::Amount => { self.new_tx_amount.pop(); }
                                FormField::Description => { self.new_tx_description.pop(); }
                                FormField::Merchant => { self.new_tx_merchant.pop(); }
                                FormField::Notes => { self.new_tx_notes.pop(); }
                                FormField::Date => { self.new_tx_date.pop(); }
                                _ => {}
                            }
                            return;
                        }
                        if k.key == "ctrl+u" {
                            match self.new_tx_field {
                                FormField::Amount => self.new_tx_amount.clear(),
                                FormField::Description => self.new_tx_description.clear(),
                                FormField::Merchant => self.new_tx_merchant.clear(),
                                FormField::Notes => self.new_tx_notes.clear(),
                                FormField::Date => self.new_tx_date.clear(),
                                _ => {}
                            }
                            return;
                        }
                    }
                }

                if app.pressed("tx:new_cancel") {
                    self.modal_state = ModalState::None;
                    self.new_tx_error = None;
                }
                if app.pressed("tx:new_save") {
                    // Validate amount
                    let amount_trimmed = self.new_tx_amount.trim();
                    if amount_trimmed.is_empty() {
                        self.new_tx_error = Some("Valor e obrigatorio.".to_string());
                        return;
                    }
                    if amount_trimmed.parse::<f64>().is_err() {
                        self.new_tx_error = Some("Valor invalido. Use formato numerico.".to_string());
                        return;
                    }
                    if self.accounts.is_empty() {
                        self.new_tx_error = Some("Nenhuma conta disponivel.".to_string());
                        return;
                    }

                    let account_id = &self.accounts[self.new_tx_account_index].id;
                    let category_id = if self.new_tx_category_index < self.categories.len() {
                        Some(self.categories[self.new_tx_category_index].id.as_str())
                    } else {
                        None
                    };
                    let tx_type_str = match self.new_tx_type {
                        TxTypeFilter::Credit | TxTypeFilter::All => "CREDIT",
                        TxTypeFilter::Debit => "DEBIT",
                    };
                    let description = if self.new_tx_description.is_empty() {
                        ""
                    } else {
                        self.new_tx_description.as_str()
                    };
                    let merchant = if self.new_tx_merchant.is_empty() {
                        None
                    } else {
                        Some(self.new_tx_merchant.as_str())
                    };
                    let notes = if self.new_tx_notes.is_empty() {
                        None
                    } else {
                        Some(self.new_tx_notes.as_str())
                    };
                    let transacted_at = if self.new_tx_date.is_empty() {
                        None
                    } else {
                        Some(self.new_tx_date.as_str())
                    };

                    let selected_tag_ids: Vec<String> = self.new_tx_tag_indices
                        .iter()
                        .filter_map(|&i| self.tags.get(i).map(|t| t.id.clone()))
                        .collect();

                    match api.create_transaction(
                        amount_trimmed,
                        description,
                        tx_type_str,
                        account_id,
                        category_id,
                        &selected_tag_ids,
                        merchant,
                        notes,
                        transacted_at,
                        self.new_tx_ignored,
                    ) {
                        Ok(_tx) => {
                            self.modal_state = ModalState::None;
                            self.new_tx_error = None;
                            self.new_tx_amount.clear();
                            self.new_tx_description.clear();
                            self.new_tx_merchant.clear();
                            self.new_tx_notes.clear();
                            self.new_tx_date = chrono::Local::now().format("%Y-%m-%d").to_string();
                            self.new_tx_type = TxTypeFilter::Credit;
                            self.new_tx_account_index = 0;
                            self.new_tx_category_index = 0;
                            self.new_tx_tag_indices.clear();
                            self.new_tx_ignored = false;
                            self.new_tx_field = FormField::Amount;
                            self.fetch_transactions(api, false);
                        }
                        Err(e) => {
                            self.new_tx_error = Some(format_api_error(&e));
                        }
                    }
                }
            }
            ModalState::EditTransaction => {
                // Determine if fields are read-only based on source
                let is_immutable = matches!(
                    self.edit_tx_source.as_deref(),
                    Some("connected" | "apple_pay")
                );

                for event in events {
                    if let InputEvent::Key(k) = event {
                        if k.key == "escape" {
                            self.modal_state = ModalState::None;
                            self.edit_tx_error = None;
                            return;
                        }
                        // Tab / Shift+Tab to cycle form fields
                        if k.name == "tab" && !k.shift {
                            self.edit_tx_field = self.edit_tx_field.next();
                            return;
                        }
                        if k.name == "tab" && k.shift {
                            self.edit_tx_field = self.edit_tx_field.prev();
                            return;
                        }
                        // Type toggle on Enter when Type field is focused (only if mutable)
                        if k.name == "enter" && self.edit_tx_field == FormField::Type && !is_immutable {
                            self.edit_tx_type = match self.edit_tx_type {
                                TxTypeFilter::Credit => TxTypeFilter::Debit,
                                _ => TxTypeFilter::Credit,
                            };
                            return;
                        }
                        // Category select cycling on Enter
                        if k.name == "enter" && self.edit_tx_field == FormField::Category {
                            if !self.categories.is_empty() {
                                self.edit_tx_category_index =
                                    (self.edit_tx_category_index + 1) % self.categories.len();
                            }
                            return;
                        }
                        // Ignored toggle on Enter when Notes field is focused (repurpose for ignored)
                        // Actually, let's use a separate mechanism - toggle ignored with 'i' key
                        if k.key == "i" && !k.ctrl && !k.alt && !k.shift {
                            self.edit_tx_ignored = !self.edit_tx_ignored;
                            return;
                        }
                        // Text input for the currently focused text field (skip immutable fields)
                        if let Some(ch) = &k.char
                            && !k.ctrl && !k.alt {
                                let can_edit = match self.edit_tx_field {
                                    FormField::Amount | FormField::Description | FormField::Date => !is_immutable,
                                    FormField::Type | FormField::Account | FormField::Category => false,
                                    _ => true,
                                };
                                if can_edit {
                                    match self.edit_tx_field {
                                        FormField::Amount => self.edit_tx_amount.push_str(ch),
                                        FormField::Description => self.edit_tx_description.push_str(ch),
                                        FormField::Merchant => self.edit_tx_merchant.push_str(ch),
                                        FormField::Notes => self.edit_tx_notes.push_str(ch),
                                        FormField::Date => self.edit_tx_date.push_str(ch),
                                        _ => {}
                                    }
                                }
                                return;
                            }
                        if k.key == "backspace" {
                            let can_edit = match self.edit_tx_field {
                                FormField::Amount | FormField::Description | FormField::Date => !is_immutable,
                                FormField::Type | FormField::Account | FormField::Category => false,
                                _ => true,
                            };
                            if can_edit {
                                match self.edit_tx_field {
                                    FormField::Amount => { self.edit_tx_amount.pop(); }
                                    FormField::Description => { self.edit_tx_description.pop(); }
                                    FormField::Merchant => { self.edit_tx_merchant.pop(); }
                                    FormField::Notes => { self.edit_tx_notes.pop(); }
                                    FormField::Date => { self.edit_tx_date.pop(); }
                                    _ => {}
                                }
                            }
                            return;
                        }
                        if k.key == "ctrl+u" {
                            let can_edit = match self.edit_tx_field {
                                FormField::Amount | FormField::Description | FormField::Date => !is_immutable,
                                FormField::Type | FormField::Account | FormField::Category => false,
                                _ => true,
                            };
                            if can_edit {
                                match self.edit_tx_field {
                                    FormField::Amount => self.edit_tx_amount.clear(),
                                    FormField::Description => self.edit_tx_description.clear(),
                                    FormField::Merchant => self.edit_tx_merchant.clear(),
                                    FormField::Notes => self.edit_tx_notes.clear(),
                                    FormField::Date => self.edit_tx_date.clear(),
                                    _ => {}
                                }
                            }
                            return;
                        }
                    }
                }

                if app.pressed("tx:new_cancel") {
                    self.modal_state = ModalState::None;
                    self.edit_tx_error = None;
                }
                if app.pressed("tx:new_save") {
                    // Validate amount
                    let amount_trimmed = self.edit_tx_amount.trim();
                    if amount_trimmed.is_empty() {
                        self.edit_tx_error = Some("Valor e obrigatorio.".to_string());
                        return;
                    }
                    if amount_trimmed.parse::<f64>().is_err() {
                        self.edit_tx_error = Some("Valor invalido. Use formato numerico.".to_string());
                        return;
                    }

                    let category_id = if self.edit_tx_category_index < self.categories.len() {
                        Some(self.categories[self.edit_tx_category_index].id.as_str())
                    } else {
                        None
                    };
                    let tx_type_str = match self.edit_tx_type {
                        TxTypeFilter::Credit | TxTypeFilter::All => "credit",
                        TxTypeFilter::Debit => "debit",
                    };
                    let description = if self.edit_tx_description.is_empty() {
                        None
                    } else {
                        Some(self.edit_tx_description.as_str())
                    };
                    let merchant = if self.edit_tx_merchant.is_empty() {
                        None
                    } else {
                        Some(self.edit_tx_merchant.as_str())
                    };
                    let notes = if self.edit_tx_notes.is_empty() {
                        None
                    } else {
                        Some(self.edit_tx_notes.as_str())
                    };
                    let transacted_at = if self.edit_tx_date.is_empty() {
                        None
                    } else {
                        Some(self.edit_tx_date.as_str())
                    };

                    match api.update_transaction(
                        &self.edit_tx_id,
                        Some(amount_trimmed),
                        description,
                        Some(tx_type_str),
                        category_id,
                        None, // tag_ids - not editing tags in this form
                        merchant,
                        notes,
                        transacted_at,
                        Some(self.edit_tx_ignored),
                        false, // clear_tags
                    ) {
                        Ok(_tx) => {
                            self.modal_state = ModalState::None;
                            self.edit_tx_error = None;
                            self.fetch_transactions(api, false);
                        }
                        Err(e) => {
                            self.edit_tx_error = Some(format_api_error(&e));
                        }
                    }
                }
            }
            ModalState::DeleteConfirm => {
                if app.pressed("tx:delete_cancel") || app.pressed("tx:escape") {
                    self.modal_state = ModalState::None;
                    self.delete_target_id = None;
                }
                if app.pressed("tx:delete_confirm")
                    && let Some(ref id) = self.delete_target_id.clone() {
                        match api.delete_transaction(id) {
                            Ok(()) => {
                                self.transactions.retain(|tx| &tx.id != id);
                                self.modal_state = ModalState::None;
                                self.delete_target_id = None;
                                self.selected_index = None;
                            }
                            Err(e) => {
                                self.error = Some(format_api_error(&e));
                                self.modal_state = ModalState::None;
                                self.delete_target_id = None;
                            }
                        }
                    }
            }
            ModalState::ExportSuccess => {
                if app.pressed("tx:export_ok") || app.pressed("tx:escape")
                    || events.iter().any(|ev| matches!(ev, InputEvent::Key(k) if k.key == "enter" || k.key == "escape"))
                {
                    self.modal_state = ModalState::None;
                    self.export_success_path = None;
                }
            }
            ModalState::None => {}
        }
    }

    pub fn render(&self, ui: &mut Container<'_>) {
        // Clone everything needed by closures into owned values before entering the UI tree.
        let error_msg = self.error.clone();
        let search_query = self.search_query.clone();
        let date_from = self.date_from.clone();
        let date_to = self.date_to.clone();
        let type_filter = self.type_filter;
        let account_filter_index = self.account_filter_index;
        let category_filter_index = self.category_filter_index;
        let tag_filter_index = self.tag_filter_index;
        let accounts = self.accounts.clone();
        let categories = self.categories.clone();
        let tags = self.tags.clone();
        let transactions = self.transactions.clone();
        let selected_index = self.selected_index;
        let scroll_offset = self.scroll_offset;
        let has_next_cursor = self.next_cursor.is_some();
        let loading = self.loading;
        let modal_state = self.modal_state;
        let detail_transaction = self.detail_transaction.clone();
        let has_selected = self.selected_index.is_some();
        let tx_count = self.transactions.len();

        // New transaction form state (cloned for closure capture)
        let new_tx_amount = self.new_tx_amount.clone();
        let new_tx_description = self.new_tx_description.clone();
        let new_tx_type = self.new_tx_type;
        let new_tx_account_index = self.new_tx_account_index;
        let new_tx_category_index = self.new_tx_category_index;
        let new_tx_merchant = self.new_tx_merchant.clone();
        let new_tx_notes = self.new_tx_notes.clone();
        let new_tx_date = self.new_tx_date.clone();
        let new_tx_error = self.new_tx_error.clone();
        let new_tx_field = self.new_tx_field;
        // Edit transaction form state (cloned for closure capture)
        let edit_tx_amount = self.edit_tx_amount.clone();
        let edit_tx_description = self.edit_tx_description.clone();
        let edit_tx_type = self.edit_tx_type;
        let edit_tx_category_index = self.edit_tx_category_index;
        let edit_tx_merchant = self.edit_tx_merchant.clone();
        let edit_tx_notes = self.edit_tx_notes.clone();
        let edit_tx_date = self.edit_tx_date.clone();
        let edit_tx_ignored = self.edit_tx_ignored;
        let edit_tx_error = self.edit_tx_error.clone();
        let edit_tx_field = self.edit_tx_field;
        let edit_tx_source = self.edit_tx_source.clone();

        let exporting = self.exporting;
        let export_success_path = self.export_success_path.clone();

        // Pre-compute table rows outside closures to avoid borrow issues
        let colored_rows: Vec<TableRow> = transactions
            .iter()
            .map(|tx| {
                let date = tx
                    .transacted_at
                    .as_deref()
                    .unwrap_or("-")
                    .split('T')
                    .next()
                    .unwrap_or("-")
                    .to_string();
                let description = tx
                    .description
                    .as_deref()
                    .or(tx.merchant_name.as_deref())
                    .unwrap_or("-")
                    .to_string();
                let category = resolve_category_name(&categories, tx.category_id.as_deref());
                let amount_str = format_amount(tx.amount.as_str(), &tx.tx_type);
                let account = resolve_account_name(&accounts, &tx.account_id);

                let amount_color = if tx.tx_type == "credit" {
                    Some(COLOR_CREDIT)
                } else {
                    Some(COLOR_DEBIT)
                };

                let mut row = TableRow::new([date, description, category, amount_str, account]);
                row.cell_colors = vec![None, None, None, amount_color, None];
                row
            })
            .collect();

        // Pre-compute modal message if needed
        let detail_message = if modal_state == ModalState::Detail {
            detail_transaction.as_ref().map(|tx| {
                let date = tx.transacted_at.as_deref().unwrap_or("-").split('T').next().unwrap_or("-").to_string();
                let desc = tx.description.as_deref().or(tx.merchant_name.as_deref()).unwrap_or("-").to_string();
                let amount = format_amount(tx.amount.as_str(), &tx.tx_type);
                let category = resolve_category_name(&categories, tx.category_id.as_deref());
                let account = resolve_account_name(&accounts, &tx.account_id);
                let notes = tx.notes.as_deref().unwrap_or("-").to_string();
                let status = tx.status.as_deref().unwrap_or("-").to_string();
                let source = tx.source.as_deref().unwrap_or("-").to_string();
                format!(
                    "Data: {date}\nDescricao: {desc}\nValor: {amount}\nCategoria: {category}\nConta: {account}\nStatus: {status}\nOrigem: {source}\nNotas: {notes}"
                )
            })
        } else {
            None
        };

        // Pre-compute filter options
        let account_options = build_filter_options(&accounts, |a| &a.name);
        let account_value = if account_filter_index == 0 {
            "Todas".to_string()
        } else if account_filter_index <= accounts.len() {
            accounts[account_filter_index - 1].name.clone()
        } else {
            "Todas".to_string()
        };

        let cat_options = build_filter_options(&categories, |cat| &cat.name);
        let cat_value = if category_filter_index == 0 {
            "Todas".to_string()
        } else if category_filter_index <= categories.len() {
            categories[category_filter_index - 1].name.clone()
        } else {
            "Todas".to_string()
        };

        let tag_options = build_filter_options(&tags, |t| &t.name);
        let tag_value = if tag_filter_index == 0 {
            "Todas".to_string()
        } else if tag_filter_index <= tags.len() {
            tags[tag_filter_index - 1].name.clone()
        } else {
            "Todas".to_string()
        };

        let type_variant = match type_filter {
            TxTypeFilter::All => ButtonVariant::Ghost,
            TxTypeFilter::Credit => ButtonVariant::Success,
            TxTypeFilter::Debit => ButtonVariant::Danger,
        };
        let type_label = type_filter.label().to_string();

        let ignored_filter = self.ignored_filter;
        let ignored_variant = match ignored_filter {
            IgnoredFilter::All => ButtonVariant::Ghost,
            IgnoredFilter::OnlyIgnored => ButtonVariant::Danger,
            IgnoredFilter::NotIgnored => ButtonVariant::Success,
        };
        let ignored_label = ignored_filter.label().to_string();

        ui.column(Column::new().gap(1), move |col| {
            // Filter bar
            col.panel(Panel::new().title("Filtros"), move |p| {
                p.row(Row::new().gap(1), move |r| {
                    r.column(Column::new().size(Size::Cells(14)).gap(0), move |c| {
                        c.label("De:");
                        c.text_input(
                            TextInputOptions {
                                placeholder: Some("AAAA-MM-DD".to_string()),
                                width: Some(14),
                                ..TextInputOptions::new(date_from.clone())
                            },
                            "tx:date_from",
                        );
                    });
                    r.column(Column::new().size(Size::Cells(14)).gap(0), move |c| {
                        c.label("Ate:");
                        c.text_input(
                            TextInputOptions {
                                placeholder: Some("AAAA-MM-DD".to_string()),
                                width: Some(14),
                                ..TextInputOptions::new(date_to.clone())
                            },
                            "tx:date_to",
                        );
                    });

                    r.column(Column::new().size(Size::Cells(16)).gap(0), move |c| {
                        c.label("Conta:");
                        c.select(
                            SelectOptions {
                                options: account_options.clone(),
                                selected_index: Some(account_filter_index),
                                width: Some(16),
                                ..SelectOptions::new(account_value.clone())
                            },
                            "tx:account_filter",
                        );
                    });

                    r.column(Column::new().size(Size::Cells(16)).gap(0), move |c| {
                        c.label("Categoria:");
                        c.select(
                            SelectOptions {
                                options: cat_options.clone(),
                                selected_index: Some(category_filter_index),
                                width: Some(16),
                                ..SelectOptions::new(cat_value.clone())
                            },
                            "tx:category_filter",
                        );
                    });

                    r.column(Column::new().size(Size::Cells(14)).gap(0), move |c| {
                        c.label("Tag:");
                        c.select(
                            SelectOptions {
                                options: tag_options.clone(),
                                selected_index: Some(tag_filter_index),
                                width: Some(14),
                                ..SelectOptions::new(tag_value.clone())
                            },
                            "tx:tag_filter",
                        );
                    });

                    r.column(Column::new().size(Size::Cells(12)).gap(0), move |c| {
                        c.label("Tipo:");
                        c.button(
                            ButtonOptions {
                                width: Some(12),
                                ..ButtonOptions::new(type_label.clone()).variant(type_variant)
                            },
                            "tx:type_toggle",
                        );
                    });

                    r.column(Column::new().size(Size::Cells(14)).gap(0), move |c| {
                        c.label("Ignorado:");
                        c.button(
                            ButtonOptions {
                                width: Some(14),
                                ..ButtonOptions::new(ignored_label.clone()).variant(ignored_variant)
                            },
                            "tx:ignored_toggle",
                        );
                    });

                    r.column(Column::new().size(Size::Fill).gap(0), move |c| {
                        c.label("Busca:");
                        c.text_input(
                            TextInputOptions {
                                placeholder: Some("Buscar...".to_string()),
                                ..TextInputOptions::new(search_query.clone())
                            },
                            "tx:search",
                        );
                    });
                });
            });

            // Action bar
            col.row(Row::new().gap(1), move |r| {
                r.button(
                    ButtonOptions::new("+ Nova Transacao").variant(ButtonVariant::Primary),
                    "tx:new",
                );
                r.button(
                    ButtonOptions::new("Atualizar").variant(ButtonVariant::Ghost),
                    "tx:refresh",
                );
                if has_selected {
                    r.button(
                        ButtonOptions::new("Excluir").variant(ButtonVariant::Danger),
                        "tx:delete",
                    );
                }
                r.button(
                    ButtonOptions::new("Exportar CSV").variant(ButtonVariant::Ghost),
                    "tx:export",
                );
                if loading || exporting {
                    let status = if exporting { " Exportando..." } else { " Carregando..." };
                    r.text(status);
                }
            });

            // Error display
            if let Some(ref err) = error_msg {
                col.styled_text(err, TextStyle::new().fg(COLOR_DEBIT));
            }

            // Transactions table
            let columns = vec![
                TableColumn::new("Data").width(Size::Cells(12)).align(Align::Left),
                TableColumn::new("Descricao").width(Size::Fr(1.0)).align(Align::Left),
                TableColumn::new("Categoria").width(Size::Cells(16)).align(Align::Left),
                TableColumn::new("Valor").width(Size::Cells(16)).align(Align::Right),
                TableColumn::new("Conta").width(Size::Cells(16)).align(Align::Left),
            ];

            col.table(
                TableOptions {
                    rows: colored_rows.clone(),
                    columns,
                    header: true,
                    selected: selected_index,
                    offset: Some(scroll_offset),
                    follow_selection: true,
                    zebra: true,
                    scrollbar: true,
                    gap: 1,
                    ..Default::default()
                },
                "tx:table",
            );

            // Pagination
            if has_next_cursor {
                col.row(Row::new().gap(1), move |r| {
                    r.button(
                        ButtonOptions::new("Carregar mais").variant(ButtonVariant::Ghost),
                        "tx:load_more",
                    );
                });
            }

            // Summary
            if tx_count > 0 {
                let summary = if has_next_cursor {
                    format!("Mostrando {tx_count} transacoes (mais disponiveis)")
                } else {
                    format!("{tx_count} transacoes")
                };
                col.label(&summary);
            }
        });

        // Draw modals outside the column closure
        match modal_state {
            ModalState::Detail => {
                if let Some(message) = detail_message {
                    ui.modal(ModalOptions {
                        title: Some("Detalhes da Transacao".to_string()),
                        message: Some(message),
                        buttons: vec![ModalButton::new("Fechar")],
                        ..Default::default()
                    });
                }
            }
            ModalState::NewTransaction => {
                // Build a text-based form display for the modal message.
                // Users navigate fields with Tab/Shift+Tab and edit via keyboard.
                let field_marker = |f: FormField| -> &'static str {
                    if f == new_tx_field { " > " } else { "   " }
                };
                let type_label = match new_tx_type {
                    TxTypeFilter::Credit | TxTypeFilter::All => "CREDITO",
                    TxTypeFilter::Debit => "DEBITO",
                };
                let account_name = if !accounts.is_empty() && new_tx_account_index < accounts.len() {
                    &accounts[new_tx_account_index].name
                } else {
                    "(nenhuma)"
                };
                let category_name = if !categories.is_empty() && new_tx_category_index < categories.len() {
                    &categories[new_tx_category_index].name
                } else {
                    "(nenhuma)"
                };
                let new_tx_tag_indices = self.new_tx_tag_indices.clone();
                let new_tx_ignored = self.new_tx_ignored;
                let tags_for_form = tags.clone();
                let selected_tag_names: Vec<String> = new_tx_tag_indices
                    .iter()
                    .filter_map(|&i| tags_for_form.get(i).map(|t| t.name.clone()))
                    .collect();
                let tags_display = if selected_tag_names.is_empty() {
                    "(nenhuma)".to_string()
                } else {
                    selected_tag_names.join(", ")
                };
                let ignored_display = if new_tx_ignored { "SIM" } else { "NAO" };
                let error_line = new_tx_error.as_ref().map(|e| format!("\nERRO: {e}")).unwrap_or_default();
                let form_message = format!(
                    "{m_amt}Valor:       {amount}\n\
                     {m_desc}Descricao:   {desc}\n\
                     {m_type}Tipo:        {type_lbl}  [Enter para alternar]\n\
                     {m_acct}Conta:       {acct}  [Enter para ciclar]\n\
                     {m_cat}Categoria:   {cat}  [Enter para ciclar]\n\
                     {m_tags}Tags:        {tags_disp}  [Enter para selecionar]\n\
                     {m_merch}Estabelec.:  {merch}\n\
                     {m_notes}Notas:       {notes}\n\
                     {m_date}Data:        {date}\n\
                     {m_ign}Ignorado:    {ign}  [Enter para alternar]{err}",
                    m_amt = field_marker(FormField::Amount),
                    amount = if new_tx_amount.is_empty() { "_" } else { &new_tx_amount },
                    m_desc = field_marker(FormField::Description),
                    desc = if new_tx_description.is_empty() { "_" } else { &new_tx_description },
                    m_type = field_marker(FormField::Type),
                    type_lbl = type_label,
                    m_acct = field_marker(FormField::Account),
                    acct = account_name,
                    m_cat = field_marker(FormField::Category),
                    cat = category_name,
                    m_tags = field_marker(FormField::Tags),
                    tags_disp = tags_display,
                    m_merch = field_marker(FormField::Merchant),
                    merch = if new_tx_merchant.is_empty() { "_" } else { &new_tx_merchant },
                    m_notes = field_marker(FormField::Notes),
                    notes = if new_tx_notes.is_empty() { "_" } else { &new_tx_notes },
                    m_date = field_marker(FormField::Date),
                    date = if new_tx_date.is_empty() { "_" } else { &new_tx_date },
                    m_ign = field_marker(FormField::Ignored),
                    ign = ignored_display,
                    err = error_line,
                );
                ui.modal(ModalOptions {
                    title: Some("Nova Transacao".to_string()),
                    message: Some(form_message),
                    width: Some(60),
                    height: Some(16),
                    buttons: vec![
                        ModalButton::new("Cancelar"),
                        ModalButton::new("Salvar"),
                    ],
                    ..Default::default()
                });
            }
            ModalState::DeleteConfirm => {
                ui.modal(ModalOptions {
                    title: Some("Confirmar Exclusao".to_string()),
                    message: Some("Tem certeza que deseja excluir esta transacao?".to_string()),
                    buttons: vec![
                        ModalButton::new("Cancelar"),
                        ModalButton::new("Excluir"),
                    ],
                    ..Default::default()
                });
            }
            ModalState::ExportSuccess => {
                let path = export_success_path.as_deref().unwrap_or("arquivo exportado");
                ui.modal(ModalOptions {
                    title: Some("Exportacao Concluida".to_string()),
                    message: Some(format!("Arquivo CSV salvo em:\n{path}")),
                    buttons: vec![ModalButton::new("OK")],
                    ..Default::default()
                });
            }
            ModalState::EditTransaction => {
                let is_immutable = matches!(
                    edit_tx_source.as_deref(),
                    Some("connected" | "apple_pay")
                );
                let field_marker = |f: FormField| -> &'static str {
                    if f == edit_tx_field { " > " } else { "   " }
                };
                let readonly_tag = if is_immutable { " [somente leitura]" } else { "" };
                let type_label = match edit_tx_type {
                    TxTypeFilter::Credit | TxTypeFilter::All => "CREDITO",
                    TxTypeFilter::Debit => "DEBITO",
                };
                let category_name = if !categories.is_empty() && edit_tx_category_index < categories.len() {
                    &categories[edit_tx_category_index].name
                } else {
                    "(nenhuma)"
                };
                let edit_tx_tag_indices = self.edit_tx_tag_indices.clone();
                let tags_for_edit = tags.clone();
                let selected_edit_tag_names: Vec<String> = edit_tx_tag_indices
                    .iter()
                    .filter_map(|&i| tags_for_edit.get(i).map(|t| t.name.clone()))
                    .collect();
                let edit_tags_display = if selected_edit_tag_names.is_empty() {
                    "(nenhuma)".to_string()
                } else {
                    selected_edit_tag_names.join(", ")
                };
                let ignored_label = if edit_tx_ignored { "SIM" } else { "NAO" };
                let error_line = edit_tx_error.as_ref().map(|e| format!("\nERRO: {e}")).unwrap_or_default();
                let form_message = format!(
                    "{m_amt}Valor:       {amount}{ro}\n\
                     {m_desc}Descricao:   {desc}{ro}\n\
                     {m_type}Tipo:        {type_lbl}  [Enter para alternar]{ro}\n\
                     {m_cat}Categoria:   {cat}  [Enter para ciclar]\n\
                     {m_tags}Tags:        {tags_disp}  [Enter para selecionar]\n\
                     {m_merch}Estabelec.:  {merch}\n\
                     {m_notes}Notas:       {notes}\n\
                     {m_date}Data:        {date}{ro}\n\
                     {m_ign}Ignorado:    {ign}  [Enter para alternar]{err}",
                    m_amt = field_marker(FormField::Amount),
                    amount = if edit_tx_amount.is_empty() { "_" } else { &edit_tx_amount },
                    ro = readonly_tag,
                    m_desc = field_marker(FormField::Description),
                    desc = if edit_tx_description.is_empty() { "_" } else { &edit_tx_description },
                    m_type = field_marker(FormField::Type),
                    type_lbl = type_label,
                    m_cat = field_marker(FormField::Category),
                    cat = category_name,
                    m_tags = field_marker(FormField::Tags),
                    tags_disp = edit_tags_display,
                    m_merch = field_marker(FormField::Merchant),
                    merch = if edit_tx_merchant.is_empty() { "_" } else { &edit_tx_merchant },
                    m_notes = field_marker(FormField::Notes),
                    notes = if edit_tx_notes.is_empty() { "_" } else { &edit_tx_notes },
                    m_date = field_marker(FormField::Date),
                    date = if edit_tx_date.is_empty() { "_" } else { &edit_tx_date },
                    m_ign = field_marker(FormField::Ignored),
                    ign = ignored_label,
                    err = error_line,
                );
                ui.modal(ModalOptions {
                    title: Some("Editar Transacao".to_string()),
                    message: Some(form_message),
                    width: Some(60),
                    height: Some(16),
                    buttons: vec![
                        ModalButton::new("Cancelar"),
                        ModalButton::new("Salvar"),
                    ],
                    ..Default::default()
                });
            }
            ModalState::None => {}
        }
    }
}

impl Default for TransactionsScreen {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn format_amount(amount: &str, tx_type: &str) -> String {
    let raw = if amount.is_empty() { return "—".to_string(); } else { amount };
    match raw.parse::<f64>() {
        Ok(v) => {
            let abs = v.abs();
            let integer_part = abs.trunc() as u64;
            let decimal_part = ((abs.fract()) * 100.0).round() as u64;
            let int_str = integer_part.to_string();
            let grouped: String = int_str
                .chars()
                .rev()
                .enumerate()
                .flat_map(|(i, c)| {
                    if i > 0 && i % 3 == 0 {
                        vec!['.', c]
                    } else {
                        vec![c]
                    }
                })
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            let prefix = if tx_type == "credit" { "+" } else { "-" };
            format!("{prefix}{grouped},{decimal_part:02}")
        }
        Err(_) => raw.to_string(),
    }
}

fn build_filter_options<T>(items: &[T], label_fn: impl Fn(&T) -> &str) -> Vec<String> {
    let mut opts = vec!["Todas".to_string()];
    opts.extend(items.iter().map(|item| label_fn(item).to_string()));
    opts
}

fn resolve_category_name(categories: &[Category], id: Option<&str>) -> String {
    match id {
        Some(cid) => categories
            .iter()
            .find(|c| c.id == cid).map_or_else(|| "-".to_string(), |c| c.name.clone()),
        None => "-".to_string(),
    }
}

fn resolve_account_name(accounts: &[Account], id: &str) -> String {
    accounts
        .iter()
        .find(|a| a.id == id).map_or_else(|| "-".to_string(), |a| a.name.clone())
}

fn format_api_error(err: &ApiError) -> String {
    format!("Erro: {err}")
}

/// Render entry point that uses a persistent screen instance stored in app.rs.
/// Input handling must be called separately via `handle_input` before rendering.
pub fn render_with_state(
    ui: &mut Container<'_>,
    screen: &mut TransactionsScreen,
    api: &ApiClient,
) {
    if !screen.loaded {
        screen.load_initial(api);
    }
    screen.render(ui);
}

/// Legacy stateless render entry point (no form support).
pub fn render(ui: &mut Container<'_>, api: &ApiClient) {
    let mut screen = TransactionsScreen::new();
    screen.load_initial(api);
    screen.render(ui);
}