use hqtui::prelude::*;
use hqtui::color::Color;
use hqtui::widgets::{
    ButtonOptions, ButtonVariant, ModalButton, ModalOptions, SelectOptions,
    TableOptions, TableColumn, TableRow, TextInputOptions,
};
use hqtui::unicode::Align;

use crate::api::{ApiClient, ApiError};
use crate::models::{Account, Category, Tag, Transaction};

const COLOR_CREDIT: Color = Color::rgb(0x22, 0xc5, 0x5e);
const COLOR_DEBIT: Color = Color::rgb(0xef, 0x44, 0x44);
const COLOR_MUTED: Color = Color::rgb(0x94, 0xa3, 0xb8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TxTypeFilter {
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
enum ModalState {
    None,
    Detail,
    NewTransaction,
    DeleteConfirm,
}

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
}

impl TransactionsScreen {
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

        match api.transactions(
            from_date, to_date, account_id, category_id, tag_id, tx_type, None, search,
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

    pub fn handle_input(&mut self, app: &App, api: &ApiClient) {
        if self.modal_state != ModalState::None {
            self.handle_modal_input(app);
            return;
        }

        if app.pressed("tx:type_toggle") {
            self.type_filter = self.type_filter.next();
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
                self.scroll_offset =
                    (self.scroll_offset + scroll_delta as usize).min(max_offset);
            } else {
                self.scroll_offset =
                    self.scroll_offset.saturating_sub((-scroll_delta) as usize);
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

        if app.pressed("tx:delete") {
            if let Some(idx) = self.selected_index {
                if idx < self.transactions.len() {
                    self.delete_target_id = Some(self.transactions[idx].id.clone());
                    self.modal_state = ModalState::DeleteConfirm;
                }
            }
        }
    }

    fn handle_modal_input(&mut self, app: &App) {
        match self.modal_state {
            ModalState::Detail => {
                if app.pressed("tx:detail_close") || app.pressed("tx:escape") {
                    self.modal_state = ModalState::None;
                    self.detail_transaction = None;
                }
            }
            ModalState::NewTransaction => {
                if app.pressed("tx:new_cancel") || app.pressed("tx:escape") {
                    self.modal_state = ModalState::None;
                }
                if app.pressed("tx:new_save") {
                    self.modal_state = ModalState::None;
                }
            }
            ModalState::DeleteConfirm => {
                if app.pressed("tx:delete_cancel") || app.pressed("tx:escape") {
                    self.modal_state = ModalState::None;
                    self.delete_target_id = None;
                }
                if app.pressed("tx:delete_confirm") {
                    self.modal_state = ModalState::None;
                    self.delete_target_id = None;
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
                if loading {
                    r.text(" Carregando...");
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
                ui.modal(ModalOptions {
                    title: Some("Nova Transacao".to_string()),
                    message: Some("Formulario de nova transacao (em breve).".to_string()),
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
            ModalState::None => {}
        }
    }
}

fn format_amount(amount: &str, tx_type: &str) -> String {
    let raw = if !amount.is_empty() { amount } else { return "—".to_string(); };
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
            .find(|c| c.id == cid)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| "-".to_string()),
        None => "-".to_string(),
    }
}

fn resolve_account_name(accounts: &[Account], id: &str) -> String {
    accounts
        .iter()
        .find(|a| a.id == id)
        .map(|a| a.name.clone())
        .unwrap_or_else(|| "-".to_string())
}

fn format_api_error(err: &ApiError) -> String {
    format!("Erro: {err}")
}