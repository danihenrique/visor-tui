use hqtui::input::InputEvent;
use hqtui::prelude::*;
use hqtui::widgets::{ButtonOptions, ButtonVariant, ModalButton, ModalOptions};

use crate::api::ApiClient;
use crate::models::RecurringExpense;
use crate::presentation::{Metric, Tone, format_currency, metric_strip, money_total};

const FREQUENCY_OPTIONS: &[&str] = &["monthly", "yearly", "weekly", "biweekly"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Subscriptions,
    FixedBills,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecurringModalState {
    None,
    CreateSubscription,
    EditSubscription,
    DeleteConfirm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecurringFormField {
    Name,
    Amount,
    Frequency,
    AccountId,
    CategoryId,
    ServiceName,
    Active,
}

impl RecurringFormField {
    fn next(self, is_subscription: bool) -> Self {
        if is_subscription {
            match self {
                Self::Name => Self::Amount,
                Self::Amount => Self::Frequency,
                Self::Frequency => Self::AccountId,
                Self::AccountId => Self::CategoryId,
                Self::CategoryId => Self::ServiceName,
                Self::ServiceName => Self::Active,
                Self::Active => Self::Name,
            }
        } else {
            match self {
                Self::Name => Self::Amount,
                Self::Amount => Self::Frequency,
                Self::Frequency => Self::AccountId,
                Self::AccountId => Self::CategoryId,
                Self::CategoryId | Self::ServiceName => Self::Active,
                Self::Active => Self::Name,
            }
        }
    }

    fn prev(self, is_subscription: bool) -> Self {
        if is_subscription {
            match self {
                Self::Name => Self::Active,
                Self::Amount => Self::Name,
                Self::Frequency => Self::Amount,
                Self::AccountId => Self::Frequency,
                Self::CategoryId => Self::AccountId,
                Self::ServiceName => Self::CategoryId,
                Self::Active => Self::ServiceName,
            }
        } else {
            match self {
                Self::Name => Self::Active,
                Self::Amount => Self::Name,
                Self::Frequency => Self::Amount,
                Self::AccountId => Self::Frequency,
                Self::CategoryId => Self::AccountId,
                Self::Active | Self::ServiceName => Self::CategoryId,
            }
        }
    }
}

pub struct RecurringScreen {
    pub active_tab: Tab,
    pub subscriptions: Vec<RecurringExpense>,
    pub fixed_bills: Vec<RecurringExpense>,
    pub loading: bool,
    pub error: Option<String>,
    pub selected_index: Option<usize>,
    pub modal_state: RecurringModalState,
    pub form_name: String,
    pub form_amount: String,
    pub form_frequency_index: usize,
    pub form_account_id: String,
    pub form_category_id: String,
    pub form_service_name: String,
    pub form_active: bool,
    pub form_field: RecurringFormField,
    pub form_error: Option<String>,
    pub delete_target_id: Option<String>,
    pub delete_target_name: Option<String>,
    pub loaded: bool,
    dialog: Option<ModalOptions>,
}

impl RecurringScreen {
    #[must_use]
    pub fn new() -> Self {
        Self {
            active_tab: Tab::Subscriptions,
            subscriptions: Vec::new(),
            fixed_bills: Vec::new(),
            loading: false,
            error: None,
            selected_index: None,
            modal_state: RecurringModalState::None,
            form_name: String::new(),
            form_amount: String::new(),
            form_frequency_index: 0,
            form_account_id: String::new(),
            form_category_id: String::new(),
            form_service_name: String::new(),
            form_active: true,
            form_field: RecurringFormField::Name,
            form_error: None,
            delete_target_id: None,
            delete_target_name: None,
            loaded: false,
            dialog: None,
        }
    }

    pub fn load_initial(&mut self, api: &ApiClient) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        self.fetch_data(api);
    }

    pub fn fetch_data(&mut self, api: &ApiClient) {
        self.loading = true;
        self.error = None;
        match api.subscriptions() {
            Ok(subs) => {
                self.subscriptions = subs;
            }
            Err(e) => {
                self.error = Some(format!("Falha ao carregar assinaturas: {e}"));
                self.loading = false;
                return;
            }
        }
        match api.fixed_bills() {
            Ok(bills) => {
                self.fixed_bills = bills;
                self.selected_index = (!self.current_items().is_empty()).then_some(
                    self.selected_index
                        .unwrap_or(0)
                        .min(self.current_items().len().saturating_sub(1)),
                );
                self.loading = false;
            }
            Err(e) => {
                self.error = Some(format!("Falha ao carregar contas fixas: {e}"));
                self.loading = false;
            }
        }
    }

    fn current_items(&self) -> &[RecurringExpense] {
        match self.active_tab {
            Tab::Subscriptions => &self.subscriptions,
            Tab::FixedBills => &self.fixed_bills,
        }
    }

    fn is_subscription_tab(&self) -> bool {
        self.active_tab == Tab::Subscriptions
    }

    pub fn paste_text(&mut self, text: &str) {
        if !matches!(
            self.modal_state,
            RecurringModalState::CreateSubscription | RecurringModalState::EditSubscription
        ) {
            return;
        }
        let text: String = text
            .chars()
            .filter(|character| !character.is_control())
            .collect();
        match self.form_field {
            RecurringFormField::Name => self.form_name.push_str(&text),
            RecurringFormField::Amount => self.form_amount.push_str(&text),
            RecurringFormField::AccountId => self.form_account_id.push_str(&text),
            RecurringFormField::CategoryId => self.form_category_id.push_str(&text),
            RecurringFormField::ServiceName => self.form_service_name.push_str(&text),
            _ => {}
        }
    }

    pub fn handle_input(&mut self, app: &App, api: &ApiClient, events: &[InputEvent]) {
        if self.modal_state != RecurringModalState::None {
            let modal_events = crate::presentation::modal_events(
                events,
                app.terminal.size(),
                self.dialog.as_ref(),
            );
            self.handle_modal_input(app, api, &modal_events);
            return;
        }

        for event in events {
            if let InputEvent::Key(k) = event {
                if k.name == "left" || k.name == "h" {
                    if self.active_tab == Tab::FixedBills {
                        self.active_tab = Tab::Subscriptions;
                        self.selected_index = None;
                    }
                    return;
                }
                if k.name == "right" || k.name == "l" {
                    if self.active_tab == Tab::Subscriptions {
                        self.active_tab = Tab::FixedBills;
                        self.selected_index = None;
                    }
                    return;
                }
                if k.name == "down" || k.name == "j" {
                    let items = self.current_items();
                    if !items.is_empty() {
                        let next = match self.selected_index {
                            Some(i) => (i + 1).min(items.len() - 1),
                            None => 0,
                        };
                        self.selected_index = Some(next);
                    }
                    return;
                }
                if k.name == "up" || k.name == "k" {
                    let items = self.current_items();
                    if !items.is_empty() {
                        let prev = match self.selected_index {
                            Some(i) => i.saturating_sub(1),
                            None => 0,
                        };
                        self.selected_index = Some(prev);
                    }
                    return;
                }
                if k.name == "n" {
                    self.open_create_modal();
                    return;
                }
                if k.name == "e" {
                    if let Some(idx) = self.selected_index
                        && idx < self.current_items().len()
                    {
                        self.populate_edit_form(idx);
                    }
                    return;
                }
                if k.name == "d" || k.name == "delete" {
                    if let Some(idx) = self.selected_index
                        && idx < self.current_items().len()
                    {
                        let id = self.current_items()[idx].id.clone();
                        let name = self.current_items()[idx].name.clone();
                        self.delete_target_id = Some(id);
                        self.delete_target_name = Some(name);
                        self.modal_state = RecurringModalState::DeleteConfirm;
                    }
                    return;
                }
            }
        }

        if app.pressed("tab_subscriptions") && self.active_tab != Tab::Subscriptions {
            self.active_tab = Tab::Subscriptions;
            self.selected_index = None;
        }
        if app.pressed("tab_fixed_bills") && self.active_tab != Tab::FixedBills {
            self.active_tab = Tab::FixedBills;
            self.selected_index = None;
        }
        if app.pressed("recurring:new") {
            self.open_create_modal();
        }
        if app.pressed("recurring:edit")
            && let Some(idx) = self.selected_index
            && idx < self.current_items().len()
        {
            self.populate_edit_form(idx);
        }
        if app.pressed("recurring:delete")
            && let Some(idx) = self.selected_index
            && idx < self.current_items().len()
        {
            let id = self.current_items()[idx].id.clone();
            let name = self.current_items()[idx].name.clone();
            self.delete_target_id = Some(id);
            self.delete_target_name = Some(name);
            self.modal_state = RecurringModalState::DeleteConfirm;
        }
    }

    fn open_create_modal(&mut self) {
        self.form_name.clear();
        self.form_amount.clear();
        self.form_frequency_index = 0;
        self.form_account_id.clear();
        self.form_category_id.clear();
        self.form_service_name.clear();
        self.form_active = true;
        self.form_field = RecurringFormField::Name;
        self.form_error = None;
        self.modal_state = RecurringModalState::CreateSubscription;
    }

    fn populate_edit_form(&mut self, idx: usize) {
        let (name, amount, frequency, account_id, category_id, service_name, active) = {
            let item = &self.current_items()[idx];
            (
                item.name.clone(),
                item.amount.clone().unwrap_or_default(),
                item.frequency.clone(),
                item.account_id.clone().unwrap_or_default(),
                item.category_id.clone().unwrap_or_default(),
                item.service_name.clone().unwrap_or_default(),
                item.active,
            )
        };
        self.form_name = name;
        self.form_amount = amount;
        self.form_frequency_index = frequency
            .as_deref()
            .and_then(|f| FREQUENCY_OPTIONS.iter().position(|&o| o == f))
            .unwrap_or(0);
        self.form_account_id = account_id;
        self.form_category_id = category_id;
        self.form_service_name = service_name;
        self.form_active = active;
        self.form_field = RecurringFormField::Name;
        self.form_error = None;
        self.modal_state = RecurringModalState::EditSubscription;
    }

    fn handle_modal_input(&mut self, app: &App, api: &ApiClient, events: &[InputEvent]) {
        match self.modal_state {
            RecurringModalState::CreateSubscription | RecurringModalState::EditSubscription => {
                let is_sub = self.is_subscription_tab();
                for event in events {
                    if let InputEvent::Key(k) = event {
                        if k.name == "escape" {
                            self.modal_state = RecurringModalState::None;
                            return;
                        }
                        if k.name == "tab" && !k.shift {
                            self.form_field = self.form_field.next(is_sub);
                            return;
                        }
                        if k.name == "tab" && k.shift {
                            self.form_field = self.form_field.prev(is_sub);
                            return;
                        }
                        if k.name == "enter" || k.key == "ctrl+s" {
                            self.submit_form(api);
                            return;
                        }
                        if self.form_field == RecurringFormField::Frequency {
                            if k.name == "right" || k.name == "l" || k.name == "space" {
                                self.form_frequency_index =
                                    (self.form_frequency_index + 1) % FREQUENCY_OPTIONS.len();
                                return;
                            }
                            if k.name == "left" || k.name == "h" {
                                self.form_frequency_index = if self.form_frequency_index == 0 {
                                    FREQUENCY_OPTIONS.len() - 1
                                } else {
                                    self.form_frequency_index - 1
                                };
                                return;
                            }
                        }
                        if self.form_field == RecurringFormField::Active
                            && (k.name == "space" || k.name == "right" || k.name == "left")
                        {
                            self.form_active = !self.form_active;
                            return;
                        }
                        if k.key == "backspace" {
                            match self.form_field {
                                RecurringFormField::Name => {
                                    self.form_name.pop();
                                }
                                RecurringFormField::Amount => {
                                    self.form_amount.pop();
                                }
                                RecurringFormField::AccountId => {
                                    self.form_account_id.pop();
                                }
                                RecurringFormField::CategoryId => {
                                    self.form_category_id.pop();
                                }
                                RecurringFormField::ServiceName => {
                                    self.form_service_name.pop();
                                }
                                _ => {}
                            }
                            return;
                        }
                        if k.key == "ctrl+u" {
                            match self.form_field {
                                RecurringFormField::Name => {
                                    self.form_name.clear();
                                }
                                RecurringFormField::Amount => {
                                    self.form_amount.clear();
                                }
                                RecurringFormField::AccountId => {
                                    self.form_account_id.clear();
                                }
                                RecurringFormField::CategoryId => {
                                    self.form_category_id.clear();
                                }
                                RecurringFormField::ServiceName => {
                                    self.form_service_name.clear();
                                }
                                _ => {}
                            }
                            return;
                        }
                        if let Some(ch) = &k.char
                            && !k.ctrl
                            && !k.alt
                        {
                            match self.form_field {
                                RecurringFormField::Name => {
                                    self.form_name.push_str(ch);
                                }
                                RecurringFormField::Amount => {
                                    self.form_amount.push_str(ch);
                                }
                                RecurringFormField::AccountId => {
                                    self.form_account_id.push_str(ch);
                                }
                                RecurringFormField::CategoryId => {
                                    self.form_category_id.push_str(ch);
                                }
                                RecurringFormField::ServiceName => {
                                    self.form_service_name.push_str(ch);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                if app.pressed("recurring_form_cancel") {
                    self.modal_state = RecurringModalState::None;
                }
                if app.pressed("recurring_form_save") {
                    self.submit_form(api);
                }
            }
            RecurringModalState::DeleteConfirm => {
                for event in events {
                    if let InputEvent::Key(k) = event {
                        if k.name == "escape" || k.name == "n" {
                            self.modal_state = RecurringModalState::None;
                            self.delete_target_id = None;
                            self.delete_target_name = None;
                            return;
                        }
                        if k.name == "y" || k.name == "enter" {
                            if let Some(ref id) = self.delete_target_id {
                                let result = if self.is_subscription_tab() {
                                    api.delete_subscription(id)
                                } else {
                                    api.delete_fixed_bill(id)
                                };
                                match result {
                                    Ok(()) => {
                                        self.fetch_data(api);
                                    }
                                    Err(e) => {
                                        self.form_error = Some(format!("Erro ao excluir: {e}"));
                                    }
                                }
                            }
                            self.modal_state = RecurringModalState::None;
                            self.delete_target_id = None;
                            self.delete_target_name = None;
                            return;
                        }
                    }
                }
                if app.pressed("recurring_delete_cancel") {
                    self.modal_state = RecurringModalState::None;
                    self.delete_target_id = None;
                    self.delete_target_name = None;
                }
                if app.pressed("recurring_delete_confirm") {
                    if let Some(ref id) = self.delete_target_id {
                        let result = if self.is_subscription_tab() {
                            api.delete_subscription(id)
                        } else {
                            api.delete_fixed_bill(id)
                        };
                        match result {
                            Ok(()) => {
                                self.fetch_data(api);
                            }
                            Err(e) => {
                                self.form_error = Some(format!("Erro ao excluir: {e}"));
                            }
                        }
                    }
                    self.modal_state = RecurringModalState::None;
                    self.delete_target_id = None;
                    self.delete_target_name = None;
                }
            }
            RecurringModalState::None => {}
        }
    }

    fn submit_form(&mut self, api: &ApiClient) {
        let name = self.form_name.trim();
        if name.is_empty() {
            self.form_error = Some("Nome \u{e9} obrigat\u{f3}rio.".to_string());
            return;
        }
        let amount = self.form_amount.trim();
        if amount.is_empty() {
            self.form_error = Some("Valor \u{e9} obrigat\u{f3}rio.".to_string());
            return;
        }
        let frequency = FREQUENCY_OPTIONS
            .get(self.form_frequency_index)
            .copied()
            .unwrap_or("monthly");
        let account_id = {
            let v = self.form_account_id.trim();
            if v.is_empty() { None } else { Some(v) }
        };
        let category_id = {
            let v = self.form_category_id.trim();
            if v.is_empty() { None } else { Some(v) }
        };

        let result = match self.modal_state {
            RecurringModalState::CreateSubscription => {
                if self.is_subscription_tab() {
                    let service_name = {
                        let v = self.form_service_name.trim();
                        if v.is_empty() { None } else { Some(v) }
                    };
                    api.create_subscription(
                        name,
                        amount,
                        frequency,
                        account_id,
                        category_id,
                        service_name,
                    )
                    .map(|_| ())
                } else {
                    api.create_fixed_bill(name, amount, frequency, account_id, category_id)
                        .map(|_| ())
                }
            }
            RecurringModalState::EditSubscription => {
                if let Some(idx) = self.selected_index {
                    if idx < self.current_items().len() {
                        let item_id = self.current_items()[idx].id.clone();
                        if self.is_subscription_tab() {
                            api.update_subscription(
                                &item_id,
                                Some(name),
                                Some(amount),
                                Some(frequency),
                                Some(self.form_active),
                            )
                            .map(|_| ())
                        } else {
                            api.update_fixed_bill(
                                &item_id,
                                Some(name),
                                Some(amount),
                                Some(frequency),
                                Some(self.form_active),
                            )
                            .map(|_| ())
                        }
                    } else {
                        self.form_error = Some("Item n\u{e3}o encontrado.".to_string());
                        return;
                    }
                } else {
                    self.form_error = Some("Nenhum item selecionado.".to_string());
                    return;
                }
            }
            _ => return,
        };
        match result {
            Ok(()) => {
                self.modal_state = RecurringModalState::None;
                self.fetch_data(api);
            }
            Err(e) => {
                self.form_error = Some(format!("Erro: {e}"));
            }
        }
    }
}

impl Default for RecurringScreen {
    fn default() -> Self {
        Self::new()
    }
}

fn format_frequency(freq: Option<&str>) -> String {
    match freq {
        Some("monthly") => "Mensal".to_string(),
        Some("yearly") => "Anual".to_string(),
        Some("weekly") => "Semanal".to_string(),
        Some("biweekly") => "Quinzenal".to_string(),
        Some(f) => f.to_string(),
        None => "—".to_string(),
    }
}

fn format_date(date_str: Option<&str>) -> String {
    match date_str {
        Some(d) if !d.is_empty() => d.to_string(),
        _ => "—".to_string(),
    }
}

pub fn render_with_state(ui: &mut Container<'_>, screen: &mut RecurringScreen, api: &ApiClient) {
    use std::fmt::Write;

    screen.load_initial(api);

    let active_tab = screen.active_tab;
    let loading = screen.loading;
    let error_msg = screen.error.clone();
    let selected = screen.selected_index;
    let modal_state = screen.modal_state;
    let form_name = screen.form_name.clone();
    let form_amount = screen.form_amount.clone();
    let form_frequency_index = screen.form_frequency_index;
    let form_account_id = screen.form_account_id.clone();
    let form_category_id = screen.form_category_id.clone();
    let form_service_name = screen.form_service_name.clone();
    let form_active = screen.form_active;
    let form_field = screen.form_field;
    let form_error = screen.form_error.clone();
    let delete_target_name = screen.delete_target_name.clone();

    let theme = ui.theme().clone();
    let current_items = screen.current_items();
    let active_count = current_items.iter().filter(|item| item.active).count();
    let monthly_total = money_total(
        current_items
            .iter()
            .filter(|item| item.active && item.frequency.as_deref() == Some("monthly"))
            .map(|item| (item.amount.as_deref(), item.currency_code.as_deref())),
    );
    let next_due = current_items
        .iter()
        .filter(|item| item.active)
        .filter_map(|item| item.next_due_date.as_deref())
        .filter(|date| !date.is_empty())
        .min()
        .unwrap_or("—")
        .to_string();
    let metrics = vec![
        Metric::new(
            "Ativas",
            format!("{active_count} / {}", current_items.len()),
            Tone::Accent,
        ),
        Metric::new("Mensais ativas", monthly_total, Tone::Negative),
        Metric::new("Próximo vencimento", next_due, Tone::Neutral),
    ];
    let items: Vec<TableRow> = current_items
        .iter()
        .map(|item| {
            let mut row = TableRow::new([
                item.name.clone(),
                format_currency(item.amount.as_deref(), item.currency_code.as_deref()),
                format_frequency(item.frequency.as_deref()),
                format_date(item.next_due_date.as_deref()),
            ]);
            row.cell_colors = vec![
                Some(if item.active {
                    theme.foreground
                } else {
                    theme.muted
                }),
                Some(if item.active {
                    theme.danger
                } else {
                    theme.muted
                }),
                Some(theme.muted),
                None,
            ];
            row
        })
        .collect();
    let detail = selected
        .and_then(|index| current_items.get(index))
        .map(|item| {
            format!(
                "{} · {} · Último pagamento {}",
                if item.active { "Ativa" } else { "Inativa" },
                item.service_name
                    .as_deref()
                    .unwrap_or("Sem serviço associado"),
                format_date(item.last_paid_date.as_deref())
            )
        });
    let tab_label = match active_tab {
        Tab::Subscriptions => "Assinaturas",
        Tab::FixedBills => "Contas fixas",
    };
    let create_label = match active_tab {
        Tab::Subscriptions => "Nova assinatura (n)",
        Tab::FixedBills => "Nova conta (n)",
    };
    let panel_title = format!("{tab_label} · {} registros", items.len());
    let is_sub_tab = active_tab == Tab::Subscriptions;
    ui.column(Column::new().gap(1), move |col| {
        metric_strip(col, metrics);
        col.row(Row::new().size(Size::Cells(1)).gap(1), move |row| {
            row.button(
                ButtonOptions::new("Assinaturas ←").variant(if is_sub_tab {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Ghost
                }),
                "tab_subscriptions",
            );
            row.button(
                ButtonOptions::new("Contas fixas →").variant(if is_sub_tab {
                    ButtonVariant::Ghost
                } else {
                    ButtonVariant::Primary
                }),
                "tab_fixed_bills",
            );
            row.button(
                ButtonOptions::new(create_label).variant(ButtonVariant::Ghost),
                "recurring:new",
            );
        });
        col.panel(
            Panel::new()
                .title(panel_title)
                .border(BorderStyle::Rounded)
                .footer("←/→ Tipo  j/k Selecionar  n Nova  e Editar  d Excluir"),
            move |panel| {
                if loading {
                    panel.label("Carregando seus compromissos…");
                } else if let Some(error) = error_msg {
                    panel.styled_text(&error, TextStyle::new().fg(theme.danger));
                } else if items.is_empty() {
                    panel.text("Nenhum compromisso recorrente.");
                    panel.label("Pressione n para cadastrar o primeiro.");
                } else {
                    panel.column(Column::new().gap(1), move |body| {
                        body.table(
                            TableOptions {
                                columns: vec![
                                    TableColumn::new("Nome").width(Size::Fill),
                                    TableColumn::new("Valor")
                                        .width(Size::Cells(16))
                                        .align(Align::Right),
                                    TableColumn::new("Frequência").width(Size::Cells(10)),
                                    TableColumn::new("Próxima").width(Size::Cells(10)),
                                ],
                                rows: items,
                                selected,
                                follow_selection: true,
                                scrollbar: true,
                                header_color: Some(theme.muted),
                                ..Default::default()
                            },
                            "recurring:table",
                        );
                        if let Some(detail) = detail {
                            body.label(&detail);
                        }
                    });
                }
            },
        );
    });

    // Render modals
    match modal_state {
        RecurringModalState::CreateSubscription | RecurringModalState::EditSubscription => {
            let is_create = modal_state == RecurringModalState::CreateSubscription;
            let title = if is_create {
                if is_sub_tab {
                    "Nova Assinatura"
                } else {
                    "Nova Conta Fixa"
                }
            } else {
                if is_sub_tab {
                    "Editar Assinatura"
                } else {
                    "Editar Conta Fixa"
                }
            };

            let name_display = if form_field == RecurringFormField::Name {
                format!("{form_name}_")
            } else {
                form_name.clone()
            };
            let amount_display = if form_field == RecurringFormField::Amount {
                format!("{form_amount}_")
            } else {
                form_amount.clone()
            };
            let freq_label = FREQUENCY_OPTIONS
                .get(form_frequency_index)
                .copied()
                .unwrap_or("monthly");
            let freq_display = if form_field == RecurringFormField::Frequency {
                format!("< {freq_label} >")
            } else {
                freq_label.to_string()
            };
            let account_display = if form_field == RecurringFormField::AccountId {
                format!("{form_account_id}_")
            } else {
                form_account_id.clone()
            };
            let category_display = if form_field == RecurringFormField::CategoryId {
                format!("{form_category_id}_")
            } else {
                form_category_id.clone()
            };
            let active_display = if form_field == RecurringFormField::Active {
                format!("< {} >", if form_active { "Sim" } else { "N\u{e3}o" })
            } else if form_active {
                "Sim".to_string()
            } else {
                "N\u{e3}o".to_string()
            };

            let mut msg = format!(
                "Nome: {name_display}\nValor: {amount_display}\nFrequ\u{ea}ncia: {freq_display}\nConta (ID): {account_display}\nCategoria (ID): {category_display}"
            );
            if is_sub_tab {
                let svc_display = if form_field == RecurringFormField::ServiceName {
                    format!("{form_service_name}_")
                } else {
                    form_service_name.clone()
                };
                let _ = write!(msg, "\nServi\u{e7}o: {svc_display}");
            }
            let _ = write!(msg, "\nAtivo: {active_display}");
            msg.push_str("\n\nTab: pr\u{f3}ximo campo | Enter: salvar | Esc: cancelar");
            if let Some(ref err) = form_error {
                let _ = write!(msg, "\n\n{err}");
            }

            let options = ModalOptions {
                title: Some(title.to_string()),
                message: Some(msg),
                buttons: vec![ModalButton::new("Cancelar"), ModalButton::new("Salvar")],
                width: Some(68),
                align: Some(Align::Left),
                ..Default::default()
            };
            screen.dialog = Some(options.clone());
            ui.modal(options);
        }
        RecurringModalState::DeleteConfirm => {
            let target = delete_target_name.unwrap_or_default();
            let options = ModalOptions {
                title: Some("Confirmar Exclus\u{e3}o".to_string()),
                message: Some(format!(
                    "Tem certeza que deseja excluir \"{target}\"?\n\nEsta a\u{e7}\u{e3}o n\u{e3}o pode ser desfeita."
                )),
                buttons: vec![ModalButton::new("Cancelar"), ModalButton::new("Excluir")],
                width: Some(68),
                align: Some(Align::Left),
                ..Default::default()
            };
            screen.dialog = Some(options.clone());
            ui.modal(options);
        }
        RecurringModalState::None => {}
    }
}

/// Legacy stateless render for backward compatibility.
pub fn render(ui: &mut Container<'_>, api: &ApiClient) {
    let mut screen = RecurringScreen::new();
    render_with_state(ui, &mut screen, api);
}

#[cfg(test)]
mod tests {
    use super::*;
    use hqtui::testing::render_to_text;

    #[test]
    fn monthly_summary_excludes_inactive_and_nonmonthly_expenses() {
        let api = ApiClient::for_test(
            "http://127.0.0.1:1",
            crate::auth::Credential::PersonalToken {
                token: "test-only".into(),
            },
            "space",
        )
        .unwrap();
        let mut screen = RecurringScreen::new();
        screen.loaded = true;
        screen.subscriptions = serde_json::from_value(serde_json::json!([
            {"id":"one", "name":"Monthly", "amount":"10.25", "frequency":"monthly", "active":true},
            {"id":"two", "name":"Monthly two", "amount":"20.10", "frequency":"monthly", "active":true},
            {"id":"year", "name":"Annual", "amount":"800", "frequency":"yearly", "active":true},
            {"id":"off", "name":"Inactive", "amount":"900", "frequency":"monthly", "active":false}
        ])).unwrap();
        let text = render_to_text(80, 24, "dark", |ui| {
            render_with_state(ui, &mut screen, &api);
        });
        assert!(text.contains("30,35 BRL"), "{text}");
        assert!(text.contains("Mensais ativas"), "{text}");
        screen.subscriptions[1].currency_code = Some("USD".into());
        let mixed = render_to_text(80, 24, "dark", |ui| {
            render_with_state(ui, &mut screen, &api);
        });
        assert!(mixed.contains("Várias moedas"), "{mixed}");
        screen.subscriptions[1].currency_code = None;
        screen.subscriptions[1].amount = None;
        let missing = render_to_text(80, 24, "dark", |ui| {
            render_with_state(ui, &mut screen, &api);
        });
        assert!(missing.contains("Indisponível"), "{missing}");
    }
}
