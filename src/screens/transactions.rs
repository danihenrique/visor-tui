use std::{cell::RefCell, rc::Rc};

use hqtui::{
    input::{InputEvent, KeyEvent, MouseAction, MouseButton},
    prelude::*,
};
use serde_json::Value;

use crate::{
    api::ApiClient,
    models::{Account, Category, Tag, Transaction},
};

mod model;
#[cfg(test)]
mod tests;
mod view;
use model::{Choice, FilterField, FilterForm, Picker, PickerKind, TransactionForm};
pub use model::{FormField, IgnoredFilter, TransactionFilters, TxTypeFilter};

#[derive(Debug, Clone, Default)]
pub enum TransactionMode {
    #[default]
    List,
    Form(Box<TransactionForm>),
    Filters(FilterForm),
    Search(String),
    Detail(Transaction),
    DeleteConfirm {
        transaction: Transaction,
        confirmed: bool,
    },
    ExportSuccess(String),
}

#[derive(Debug, Clone, PartialEq)]
enum Effect {
    None,
    Fetch { append: bool },
    Save(Value),
    Delete(String),
    Export,
}

#[derive(Clone, Default)]
pub struct TransactionsScreen {
    pub transactions: Vec<Transaction>,
    pub next_cursor: Option<String>,
    pub loading: bool,
    pub error: Option<String>,
    pub selected_index: Option<usize>,
    pub scroll_offset: usize,
    pub accounts: Vec<Account>,
    pub categories: Vec<Category>,
    pub tags: Vec<Tag>,
    pub loaded: bool,
    pub filters: TransactionFilters,
    pub mode: TransactionMode,
    hitboxes: Rc<RefCell<Vec<view::Hitbox>>>,
}

impl TransactionsScreen {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn is_modal_open(&self) -> bool {
        !matches!(self.mode, TransactionMode::List)
    }

    pub fn on_enter(&mut self, api: &ApiClient) {
        self.load_initial(api);
    }

    pub fn load_initial(&mut self, api: &ApiClient) {
        self.loading = true;
        let mut errors = Vec::new();
        match api.accounts(false) {
            Ok(items) => self.accounts = items,
            Err(error) => errors.push(error.to_string()),
        }
        match api.categories(false) {
            Ok(items) => self.categories = items,
            Err(error) => errors.push(error.to_string()),
        }
        match api.tags() {
            Ok(items) => self.tags = items,
            Err(error) => errors.push(error.to_string()),
        }
        self.fetch_transactions(api, false);
        if !errors.is_empty() {
            self.error = Some(errors.join(" · "));
        }
        self.loaded = true;
    }

    pub fn refresh(&mut self, api: &ApiClient) {
        self.load_initial(api);
    }

    pub fn fetch_transactions(&mut self, api: &ApiClient, append: bool) {
        self.loading = true;
        self.error = None;
        let f = &self.filters;
        let after = if append {
            self.next_cursor.as_deref()
        } else {
            None
        };
        match api.transactions(
            nonempty(&f.date_from),
            nonempty(&f.date_to),
            f.account_id.as_deref(),
            f.category_id.as_deref(),
            f.tag_id.as_deref(),
            f.tx_type.api_value(),
            f.ignored.api_value(),
            nonempty(&f.search),
            Some(50),
            after,
        ) {
            Ok((transactions, cursor)) => {
                if append {
                    self.transactions.extend(transactions);
                } else {
                    self.transactions = transactions;
                    self.scroll_offset = 0;
                    self.selected_index = (!self.transactions.is_empty()).then_some(0);
                }
                self.next_cursor = cursor;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
        self.loading = false;
    }

    pub fn handle_input(&mut self, _app: &App, api: &ApiClient, events: &[InputEvent]) {
        for event in events {
            let effect = match event {
                InputEvent::Key(key) => self.reduce_key(key),
                InputEvent::Paste(paste) => {
                    self.insert_text(&paste.text);
                    Effect::None
                }
                InputEvent::Mouse(mouse)
                    if mouse.action == MouseAction::Press && mouse.button == MouseButton::Left =>
                {
                    let hit = self
                        .hitboxes
                        .borrow()
                        .iter()
                        .rev()
                        .find(|hit| hit.contains(mouse.x, mouse.y))
                        .cloned();
                    hit.map_or(Effect::None, |hit| self.click(hit.action))
                }
                InputEvent::Mouse(mouse)
                    if mouse.action == MouseAction::Scroll && !self.is_modal_open() =>
                {
                    self.move_selection(if mouse.scroll > 0 { 3 } else { -3 });
                    Effect::None
                }
                _ => Effect::None,
            };
            self.execute(effect, api);
        }
    }

    fn execute(&mut self, effect: Effect, api: &ApiClient) {
        match effect {
            Effect::None => {}
            Effect::Fetch { append } => self.fetch_transactions(api, append),
            Effect::Save(body) => self.save(api, &body),
            Effect::Delete(id) => match api.delete_transaction(&id) {
                Ok(()) => {
                    self.mode = TransactionMode::List;
                    self.fetch_transactions(api, false);
                }
                Err(error) => self.error = Some(error.to_string()),
            },
            Effect::Export => self.export(api),
        }
    }

    fn save(&mut self, api: &ApiClient, body: &Value) {
        let TransactionMode::Form(form) = &mut self.mode else {
            return;
        };
        let result = if let Some(original) = &form.original {
            api.patch_transaction(&original.id, body)
        } else {
            api.create_transaction(
                body["amount"].as_str().unwrap_or_default(),
                body["description"].as_str().unwrap_or_default(),
                body["type"].as_str().unwrap_or_default(),
                body["account_id"].as_str().unwrap_or_default(),
                body["category_id"].as_str(),
                &form.tag_ids,
                body["merchant_name"].as_str(),
                body["notes"].as_str(),
                body["transacted_at"].as_str(),
                form.ignored,
            )
        };
        match result {
            Ok(_) => {
                self.mode = TransactionMode::List;
                self.fetch_transactions(api, false);
            }
            Err(error) => form.api_error(&error),
        }
    }

    fn export(&mut self, api: &ApiClient) {
        let f = &self.filters;
        match api.export_transactions_csv(
            nonempty(&f.date_from),
            nonempty(&f.date_to),
            f.account_id.as_deref(),
            f.category_id.as_deref(),
            f.tag_id.as_deref(),
            f.tx_type.api_value(),
            f.ignored.api_value(),
            nonempty(&f.search),
            None,
            None,
        ) {
            Ok(csv) => {
                let name = format!(
                    "transactions_{}.csv",
                    chrono::Local::now().format("%Y-%m-%d_%H%M%S_%f")
                );
                let result = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&name)
                    .and_then(|mut file| {
                        use std::io::Write;
                        file.write_all(csv.as_bytes())
                    });
                match result {
                    Ok(()) => self.mode = TransactionMode::ExportSuccess(name),
                    Err(error) => {
                        self.error = Some(format!("Não foi possível salvar o CSV: {error}"));
                    }
                }
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn reduce_key(&mut self, key: &KeyEvent) -> Effect {
        if self.picker().is_some() {
            return self.picker_key(key);
        }
        match &mut self.mode {
            TransactionMode::List => self.list_key(key),
            TransactionMode::Form(form) => {
                if key.name == "escape" {
                    self.mode = TransactionMode::List;
                    return Effect::None;
                }
                if key.name == "tab" {
                    form.move_focus(key.shift);
                    return Effect::None;
                }
                if key.key == "ctrl+s" {
                    return form.payload().map_or(Effect::None, Effect::Save);
                }
                if key.name == "enter" || key.name == "space" && form.text_mut().is_none() {
                    if activate_form_field(form) {
                        return Effect::None;
                    }
                    return form.payload().map_or(Effect::None, Effect::Save);
                }
                form.errors.remove(&form.focused);
                if let Some(text) = form.text_mut() {
                    edit_text(text, key);
                }
                Effect::None
            }
            TransactionMode::Filters(form) => {
                if key.name == "escape" {
                    self.mode = TransactionMode::List;
                    return Effect::None;
                }
                if key.name == "tab" {
                    form.move_focus(key.shift);
                    return Effect::None;
                }
                if key.key == "ctrl+l" {
                    form.draft = TransactionFilters::default();
                    return Effect::None;
                }
                if key.key == "ctrl+s" {
                    return self.apply_filters();
                }
                if key.name == "enter" || key.name == "space" && form.text_mut().is_none() {
                    if activate_filter_field(form) {
                        return Effect::None;
                    }
                    return self.apply_filters();
                }
                if let Some(text) = form.text_mut() {
                    edit_text(text, key);
                }
                Effect::None
            }
            TransactionMode::Search(text) => {
                if key.name == "escape" {
                    self.mode = TransactionMode::List;
                } else if key.name == "enter" {
                    self.filters.search = text.trim().to_string();
                    self.mode = TransactionMode::List;
                    return Effect::Fetch { append: false };
                } else {
                    edit_text(text, key);
                }
                Effect::None
            }
            TransactionMode::DeleteConfirm {
                transaction,
                confirmed,
            } => {
                if key.name == "escape" || key.key == "n" {
                    self.mode = TransactionMode::List;
                } else if matches!(key.name.as_str(), "tab" | "left" | "right") {
                    *confirmed = !*confirmed;
                } else if key.key == "y" || key.name == "enter" && *confirmed {
                    return Effect::Delete(transaction.id.clone());
                } else if key.name == "enter" {
                    self.mode = TransactionMode::List;
                }
                Effect::None
            }
            TransactionMode::Detail(tx) => {
                if key.name == "escape" || key.name == "enter" {
                    self.mode = TransactionMode::List;
                } else if key.key == "e" {
                    self.mode = TransactionMode::Form(Box::new(TransactionForm::edit(tx.clone())));
                } else if key.key == "d" {
                    self.mode = TransactionMode::DeleteConfirm {
                        transaction: tx.clone(),
                        confirmed: false,
                    };
                }
                Effect::None
            }
            TransactionMode::ExportSuccess(_) => {
                if matches!(key.name.as_str(), "escape" | "enter") {
                    self.mode = TransactionMode::List;
                }
                Effect::None
            }
        }
    }

    fn list_key(&mut self, key: &KeyEvent) -> Effect {
        match key.key.as_str() {
            "j" | "down" => self.move_selection(1),
            "k" | "up" => self.move_selection(-1),
            "pageup" => self.move_selection(-10),
            "pagedown" => self.move_selection(10),
            "home" => self.selected_index = (!self.transactions.is_empty()).then_some(0),
            "end" => self.selected_index = self.transactions.len().checked_sub(1),
            "n" => self.mode = TransactionMode::Form(Box::new(TransactionForm::new())),
            "f" => self.mode = TransactionMode::Filters(FilterForm::new(self.filters.clone())),
            "/" => self.mode = TransactionMode::Search(self.filters.search.clone()),
            "enter" | "e" | "d" => {
                if let Some(tx) = self
                    .selected_index
                    .and_then(|index| self.transactions.get(index))
                    .cloned()
                {
                    self.mode = match key.key.as_str() {
                        "e" => TransactionMode::Form(Box::new(TransactionForm::edit(tx))),
                        "d" => TransactionMode::DeleteConfirm {
                            transaction: tx,
                            confirmed: false,
                        },
                        _ => TransactionMode::Detail(tx),
                    };
                }
            }
            "c" => {
                self.filters = TransactionFilters::default();
                return Effect::Fetch { append: false };
            }
            "r" => return Effect::Fetch { append: false },
            "m" if self.next_cursor.is_some() => return Effect::Fetch { append: true },
            "E" | "ctrl+e" => return Effect::Export,
            _ => {}
        }
        Effect::None
    }

    fn move_selection(&mut self, delta: isize) {
        let Some(last) = self.transactions.len().checked_sub(1) else {
            self.selected_index = None;
            return;
        };
        self.selected_index = Some(
            self.selected_index
                .unwrap_or(0)
                .saturating_add_signed(delta)
                .min(last),
        );
    }

    fn apply_filters(&mut self) -> Effect {
        let TransactionMode::Filters(form) = &mut self.mode else {
            return Effect::None;
        };
        match form.draft.validate() {
            Ok(()) => {
                self.filters = form.draft.clone();
                self.mode = TransactionMode::List;
                Effect::Fetch { append: false }
            }
            Err(error) => {
                form.error = Some(error);
                Effect::None
            }
        }
    }

    fn picker(&self) -> Option<&Picker> {
        match &self.mode {
            TransactionMode::Form(form) => form.picker.as_ref(),
            TransactionMode::Filters(form) => form.picker.as_ref(),
            _ => None,
        }
    }

    fn picker_mut(&mut self) -> Option<&mut Picker> {
        match &mut self.mode {
            TransactionMode::Form(form) => form.picker.as_mut(),
            TransactionMode::Filters(form) => form.picker.as_mut(),
            _ => None,
        }
    }

    fn close_picker(&mut self) {
        match &mut self.mode {
            TransactionMode::Form(form) => form.picker = None,
            TransactionMode::Filters(form) => form.picker = None,
            _ => {}
        }
    }

    fn choices(&self, picker: &Picker) -> Vec<Choice> {
        let labels: Vec<Choice> = match picker.kind {
            PickerKind::Account => self
                .accounts
                .iter()
                .map(|a| Choice {
                    id: Some(a.id.clone()),
                    label: a.name.clone(),
                })
                .collect(),
            PickerKind::Category => self
                .categories
                .iter()
                .map(|c| Choice {
                    id: Some(c.id.clone()),
                    label: c.name.clone(),
                })
                .collect(),
            PickerKind::Tags => self
                .tags
                .iter()
                .map(|t| Choice {
                    id: Some(t.id.clone()),
                    label: t.name.clone(),
                })
                .collect(),
        };
        let placeholder = if matches!(self.mode, TransactionMode::Filters(_)) {
            "Todos"
        } else if picker.kind == PickerKind::Tags {
            "Limpar todas as tags"
        } else {
            "Selecione uma opção"
        };
        std::iter::once(Choice {
            id: None,
            label: placeholder.into(),
        })
        .chain(labels)
        .filter(|choice| {
            picker.query.is_empty()
                || choice
                    .label
                    .to_lowercase()
                    .contains(&picker.query.to_lowercase())
        })
        .collect()
    }

    fn picker_key(&mut self, key: &KeyEvent) -> Effect {
        if key.name == "escape" || key.name == "tab" {
            self.close_picker();
            return Effect::None;
        }
        let choices = self
            .picker()
            .map(|picker| self.choices(picker))
            .unwrap_or_default();
        let Some(picker) = self.picker_mut() else {
            return Effect::None;
        };
        match key.name.as_str() {
            "up" => picker.selected = picker.selected.saturating_sub(1),
            "down" => picker.selected = (picker.selected + 1).min(choices.len().saturating_sub(1)),
            "enter" => {
                if let Some(choice) = choices.get(picker.selected).cloned() {
                    self.select_choice(choice);
                }
                return Effect::None;
            }
            _ => {
                edit_text(&mut picker.query, key);
                picker.selected = 0;
            }
        }
        Effect::None
    }

    fn select_choice(&mut self, choice: Choice) {
        let Some(kind) = self.picker().map(|picker| picker.kind) else {
            return;
        };
        match &mut self.mode {
            TransactionMode::Form(form) => {
                match kind {
                    PickerKind::Account => form.account_id = choice.id,
                    PickerKind::Category => {
                        if choice.id.is_none()
                            && form
                                .original
                                .as_ref()
                                .is_some_and(|tx| tx.category_id.is_some())
                        {
                            form.error = Some(
                                "A API exige uma categoria. Selecione outra para substituir."
                                    .into(),
                            );
                        } else {
                            form.category_id = choice.id;
                        }
                    }
                    PickerKind::Tags => {
                        if let Some(id) = choice.id {
                            if let Some(index) = form.tag_ids.iter().position(|tag| tag == &id) {
                                form.tag_ids.remove(index);
                            } else {
                                form.tag_ids.push(id);
                            }
                        } else {
                            form.tag_ids.clear();
                        }
                    }
                }
                if kind != PickerKind::Tags {
                    form.picker = None;
                }
            }
            TransactionMode::Filters(form) => {
                match kind {
                    PickerKind::Account => form.draft.account_id = choice.id,
                    PickerKind::Category => form.draft.category_id = choice.id,
                    PickerKind::Tags => form.draft.tag_id = choice.id,
                }
                form.picker = None;
            }
            _ => {}
        }
    }

    fn insert_text(&mut self, text: &str) {
        let cleaned: String = text.chars().filter(|ch| !ch.is_control()).collect();
        if let Some(picker) = self.picker_mut() {
            picker.query.push_str(&cleaned);
            picker.selected = 0;
            return;
        }
        let target = match &mut self.mode {
            TransactionMode::Form(form) => form.text_mut(),
            TransactionMode::Filters(form) => form.text_mut(),
            TransactionMode::Search(query) => Some(query),
            _ => None,
        };
        if let Some(target) = target {
            target.push_str(&cleaned);
        }
    }

    fn click(&mut self, action: view::Action) -> Effect {
        match action {
            view::Action::Key(key) => self.reduce_key(&KeyEvent {
                name: key.clone(),
                key,
                ctrl: false,
                alt: false,
                shift: false,
                char: None,
                raw: String::new(),
            }),
            view::Action::Row(index) => {
                self.selected_index = Some(index);
                Effect::None
            }
            view::Action::Field(field) => {
                if let TransactionMode::Form(form) = &mut self.mode
                    && form.editable(field)
                {
                    form.focused = field;
                    activate_form_field(form);
                }
                Effect::None
            }
            view::Action::Filter(field) => {
                if let TransactionMode::Filters(form) = &mut self.mode {
                    form.focused = field;
                    activate_filter_field(form);
                }
                Effect::None
            }
            view::Action::Choice(choice) => {
                self.select_choice(choice);
                Effect::None
            }
        }
    }

    pub fn render(&self, ui: &mut Container<'_>) {
        let snapshot = self.clone();
        ui.draw(move |surface| view::draw(surface, &snapshot));
    }
}

fn activate_form_field(form: &mut TransactionForm) -> bool {
    if !form.editable(form.focused) {
        return true;
    }
    match form.focused {
        FormField::Type => {
            form.tx_type = if form.tx_type == TxTypeFilter::Credit {
                TxTypeFilter::Debit
            } else {
                TxTypeFilter::Credit
            }
        }
        FormField::Ignored => form.ignored = !form.ignored,
        FormField::Account => form.picker = Some(Picker::new(PickerKind::Account)),
        FormField::Category => form.picker = Some(Picker::new(PickerKind::Category)),
        FormField::Tags => form.picker = Some(Picker::new(PickerKind::Tags)),
        _ => return false,
    }
    true
}

fn activate_filter_field(form: &mut FilterForm) -> bool {
    match form.focused {
        FilterField::Type => form.draft.tx_type = form.draft.tx_type.next(),
        FilterField::Ignored => form.draft.ignored = form.draft.ignored.next(),
        FilterField::Account => form.picker = Some(Picker::new(PickerKind::Account)),
        FilterField::Category => form.picker = Some(Picker::new(PickerKind::Category)),
        FilterField::Tag => form.picker = Some(Picker::new(PickerKind::Tags)),
        _ => return false,
    }
    true
}

fn edit_text(text: &mut String, key: &KeyEvent) {
    match key.key.as_str() {
        "backspace" => {
            text.pop();
        }
        "ctrl+u" => text.clear(),
        _ if !key.ctrl && !key.alt => {
            if let Some(value) = &key.char {
                text.extend(value.chars().filter(|ch| !ch.is_control()));
            }
        }
        _ => {}
    }
}

fn nonempty(value: &str) -> Option<&str> {
    if value.is_empty() { None } else { Some(value) }
}

pub fn render_with_state(
    ui: &mut Container<'_>,
    screen: &mut TransactionsScreen,
    _api: &ApiClient,
) {
    screen.render(ui);
}
