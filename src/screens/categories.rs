use crate::interaction::{Interactions, Interactive};
use hqtui::input::InputEvent;
use hqtui::prelude::*;
use hqtui::unicode::Align;
use hqtui::widgets::{
    ButtonOptions, ButtonVariant, ModalButton, ModalOptions, TableColumn, TableOptions, TableRow,
    TextStyle,
};

use crate::api::ApiClient;
use crate::models::Category;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CategoryModalState {
    None,
    Create,
    Edit,
    DeleteConfirm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CategoryFormField {
    Name,
    Type,
    Parent,
    Kind,
    Hidden,
}

impl CategoryFormField {
    fn next(self) -> Self {
        match self {
            Self::Name => Self::Type,
            Self::Type => Self::Parent,
            Self::Parent => Self::Kind,
            Self::Kind => Self::Hidden,
            Self::Hidden => Self::Name,
        }
    }

    fn prev(self) -> Self {
        match self {
            Self::Name => Self::Hidden,
            Self::Type => Self::Name,
            Self::Parent => Self::Type,
            Self::Kind => Self::Parent,
            Self::Hidden => Self::Kind,
        }
    }
}

const CATEGORY_TYPES: &[&str] = &["income", "expense", "transfer"];

#[allow(clippy::struct_excessive_bools)]
pub struct CategoriesScreen {
    pub categories: Vec<Category>,
    pub loading: bool,
    pub error: Option<String>,
    pub selected_index: Option<usize>,
    pub modal_state: CategoryModalState,
    pub form_name: String,
    pub form_type_index: usize,
    pub form_parent_id: String,
    pub form_kind: String,
    pub form_hidden: bool,
    pub form_field: CategoryFormField,
    pub form_error: Option<String>,
    pub delete_target_id: Option<String>,
    pub delete_target_name: Option<String>,
    pub include_hidden: bool,
    pub loaded: bool,
    pub reload_requested: bool,
    interactions: Interactions,
}

impl CategoriesScreen {
    #[must_use]
    pub fn new() -> Self {
        Self {
            categories: Vec::new(),
            loading: false,
            error: None,
            selected_index: None,
            modal_state: CategoryModalState::None,
            form_name: String::new(),
            form_type_index: 0,
            form_parent_id: String::new(),
            form_kind: String::new(),
            form_hidden: false,
            form_field: CategoryFormField::Name,
            form_error: None,
            delete_target_id: None,
            delete_target_name: None,
            include_hidden: false,
            loaded: false,
            reload_requested: false,
            interactions: Interactions::default(),
        }
    }

    pub fn load_initial(&mut self, api: &ApiClient) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        self.fetch_categories(api);
    }

    pub fn fetch_categories(&mut self, api: &ApiClient) {
        self.loading = true;
        self.error = None;
        self.reload_requested = false;
        match api.categories(self.include_hidden) {
            Ok(cats) => {
                self.categories = cats;
                self.selected_index = (!self.categories.is_empty()).then_some(
                    self.selected_index
                        .unwrap_or(0)
                        .min(self.categories.len().saturating_sub(1)),
                );
                self.loading = false;
            }
            Err(e) => {
                self.error = Some(format!("Falha ao carregar categorias: {e}"));
                self.loading = false;
            }
        }
    }

    pub fn paste_text(&mut self, text: &str) {
        if !matches!(
            self.modal_state,
            CategoryModalState::Create | CategoryModalState::Edit
        ) {
            return;
        }
        let text: String = text
            .chars()
            .filter(|character| !character.is_control())
            .collect();
        match self.form_field {
            CategoryFormField::Name => self.form_name.push_str(&text),
            CategoryFormField::Parent => self.form_parent_id.push_str(&text),
            CategoryFormField::Kind => self.form_kind.push_str(&text),
            _ => {}
        }
    }

    pub fn handle_input(&mut self, api: &ApiClient, events: &[InputEvent]) {
        if self.modal_state != CategoryModalState::None {
            if let Some(index) = self.interactions.field(events) {
                self.form_field = [
                    CategoryFormField::Name,
                    CategoryFormField::Type,
                    CategoryFormField::Parent,
                    CategoryFormField::Kind,
                    CategoryFormField::Hidden,
                ][index];
            }
            let modal_events = self.interactions.modal_events(events);
            self.handle_modal_input(api, &modal_events);
            return;
        }
        let mapped_events = self
            .interactions
            .list_events(events, &mut self.selected_index);
        let events = mapped_events.as_slice();

        for event in events {
            if let InputEvent::Key(k) = event {
                if k.name == "down" || k.name == "j" {
                    if !self.categories.is_empty() {
                        let next = match self.selected_index {
                            Some(i) => (i + 1).min(self.categories.len() - 1),
                            None => 0,
                        };
                        self.selected_index = Some(next);
                    }
                    return;
                }
                if k.name == "up" || k.name == "k" {
                    if !self.categories.is_empty() {
                        let prev = match self.selected_index {
                            Some(i) => i.saturating_sub(1),
                            None => 0,
                        };
                        self.selected_index = Some(prev);
                    }
                    return;
                }
                if k.name == "h" {
                    self.include_hidden = !self.include_hidden;
                    self.reload_requested = true;
                    return;
                }
                if k.name == "n" {
                    self.open_create_modal();
                    return;
                }
                if k.name == "e" {
                    self.open_edit_modal();
                    return;
                }
                if k.name == "d" || k.name == "delete" {
                    self.open_delete_confirm();
                    return;
                }
            }
        }

        // Button presses
        if self.interactions.pressed(events, "categories:new") {
            self.open_create_modal();
        }
        if self.interactions.pressed(events, "categories:edit") {
            self.open_edit_modal();
        }
        if self.interactions.pressed(events, "categories:delete") {
            self.open_delete_confirm();
        }
    }

    fn open_create_modal(&mut self) {
        self.form_name.clear();
        self.form_type_index = 0;
        self.form_parent_id.clear();
        self.form_kind.clear();
        self.form_hidden = false;
        self.form_field = CategoryFormField::Name;
        self.form_error = None;
        self.modal_state = CategoryModalState::Create;
    }

    fn open_edit_modal(&mut self) {
        if let Some(idx) = self.selected_index
            && idx < self.categories.len()
        {
            let cat = &self.categories[category_order(&self.categories)[idx].0];
            self.form_name = cat.name.clone();
            self.form_type_index = CATEGORY_TYPES
                .iter()
                .position(|t| cat.category_type.as_deref() == Some(t))
                .unwrap_or(0);
            self.form_parent_id = cat.parent_id.clone().unwrap_or_default();
            self.form_kind = cat.kind.clone().unwrap_or_default();
            self.form_hidden = cat.hidden;
            self.form_field = CategoryFormField::Name;
            self.form_error = None;
            self.modal_state = CategoryModalState::Edit;
        }
    }

    fn open_delete_confirm(&mut self) {
        if let Some(idx) = self.selected_index
            && idx < self.categories.len()
        {
            let cat = &self.categories[category_order(&self.categories)[idx].0];
            self.delete_target_id = Some(cat.id.clone());
            self.delete_target_name = Some(cat.name.clone());
            self.modal_state = CategoryModalState::DeleteConfirm;
        }
    }

    fn handle_modal_input(&mut self, api: &ApiClient, events: &[InputEvent]) {
        match self.modal_state {
            CategoryModalState::Create | CategoryModalState::Edit => {
                for event in events {
                    if let InputEvent::Key(k) = event {
                        if k.name == "escape" {
                            self.modal_state = CategoryModalState::None;
                            return;
                        }
                        if k.name == "tab" && !k.shift {
                            self.form_field = self.form_field.next();
                            return;
                        }
                        if k.name == "tab" && k.shift {
                            self.form_field = self.form_field.prev();
                            return;
                        }
                        if k.name == "enter" || k.key == "ctrl+s" {
                            self.submit_form(api);
                            return;
                        }
                        // Type field: left/right to cycle
                        if self.form_field == CategoryFormField::Type {
                            if k.name == "right" || k.name == "l" {
                                self.form_type_index =
                                    (self.form_type_index + 1) % CATEGORY_TYPES.len();
                                return;
                            }
                            if k.name == "left" || k.name == "h" {
                                self.form_type_index = if self.form_type_index == 0 {
                                    CATEGORY_TYPES.len() - 1
                                } else {
                                    self.form_type_index - 1
                                };
                                return;
                            }
                        }
                        // Hidden field: space or y/n to toggle
                        if self.form_field == CategoryFormField::Hidden
                            && (k.name == "space" || k.name == "y" || k.name == "n")
                        {
                            self.form_hidden = !self.form_hidden;
                            return;
                        }
                        // Text fields: backspace, ctrl+u, char input
                        if k.key == "backspace" {
                            match self.form_field {
                                CategoryFormField::Name => {
                                    self.form_name.pop();
                                }
                                CategoryFormField::Parent => {
                                    self.form_parent_id.pop();
                                }
                                CategoryFormField::Kind => {
                                    self.form_kind.pop();
                                }
                                _ => {}
                            }
                            return;
                        }
                        if k.key == "ctrl+u" {
                            match self.form_field {
                                CategoryFormField::Name => {
                                    self.form_name.clear();
                                }
                                CategoryFormField::Parent => {
                                    self.form_parent_id.clear();
                                }
                                CategoryFormField::Kind => {
                                    self.form_kind.clear();
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
                                CategoryFormField::Name => {
                                    self.form_name.push_str(ch);
                                }
                                CategoryFormField::Parent => {
                                    self.form_parent_id.push_str(ch);
                                }
                                CategoryFormField::Kind => {
                                    self.form_kind.push_str(ch);
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            CategoryModalState::DeleteConfirm => {
                for event in events {
                    if let InputEvent::Key(k) = event {
                        if k.name == "escape" || k.name == "n" {
                            self.modal_state = CategoryModalState::None;
                            self.delete_target_id = None;
                            self.delete_target_name = None;
                            return;
                        }
                        if k.name == "y" || k.name == "enter" {
                            self.execute_delete(api);
                            return;
                        }
                    }
                }
                if self.interactions.pressed(events, "cat_delete_cancel") {
                    self.modal_state = CategoryModalState::None;
                    self.delete_target_id = None;
                    self.delete_target_name = None;
                }
                if self.interactions.pressed(events, "cat_delete_confirm") {
                    self.execute_delete(api);
                }
            }
            CategoryModalState::None => {}
        }
    }

    fn execute_delete(&mut self, api: &ApiClient) {
        if let Some(ref id) = self.delete_target_id {
            match api.delete_category(id) {
                Ok(()) => {
                    self.reload_requested = true;
                }
                Err(e) => {
                    self.form_error = Some(format!("Erro ao excluir: {e}"));
                }
            }
        }
        self.modal_state = CategoryModalState::None;
        self.delete_target_id = None;
        self.delete_target_name = None;
    }

    fn submit_form(&mut self, api: &ApiClient) {
        let name = self.form_name.trim();
        if name.is_empty() {
            self.form_error = Some("Nome e obrigatorio.".to_string());
            return;
        }

        let category_type = CATEGORY_TYPES[self.form_type_index];

        let parent_id = if self.form_parent_id.trim().is_empty() {
            None
        } else {
            Some(self.form_parent_id.trim())
        };

        // Validate parent_id references an existing category
        if let Some(pid) = parent_id
            && !self.categories.iter().any(|c| c.id == pid)
        {
            self.form_error = Some(format!("Categoria pai nao encontrada: {pid}"));
            return;
        }

        let kind = if self.form_kind.trim().is_empty() {
            None
        } else {
            Some(self.form_kind.trim())
        };

        let result = match self.modal_state {
            CategoryModalState::Create => api.create_category(name, category_type, parent_id, kind),
            CategoryModalState::Edit => {
                if let Some(idx) = self.selected_index {
                    if idx < self.categories.len() {
                        let cat_id = self.categories[category_order(&self.categories)[idx].0]
                            .id
                            .clone();
                        api.update_category(&cat_id, Some(name), parent_id, Some(self.form_hidden))
                    } else {
                        self.form_error = Some("Categoria nao encontrada.".to_string());
                        return;
                    }
                } else {
                    self.form_error = Some("Nenhuma categoria selecionada.".to_string());
                    return;
                }
            }
            _ => return,
        };

        match result {
            Ok(_) => {
                self.modal_state = CategoryModalState::None;
                self.reload_requested = true;
            }
            Err(e) => {
                self.form_error = Some(format!("Erro: {e}"));
            }
        }
    }
}

impl Default for CategoriesScreen {
    fn default() -> Self {
        Self::new()
    }
}

fn category_order(categories: &[Category]) -> Vec<(usize, usize)> {
    let mut result = Vec::with_capacity(categories.len());
    let mut visited = std::collections::HashSet::new();
    let roots = categories
        .iter()
        .enumerate()
        .filter(|(_, category)| {
            category
                .parent_id
                .as_deref()
                .is_none_or(|parent| !categories.iter().any(|candidate| candidate.id == parent))
        })
        .map(|(index, _)| index)
        .chain(0..categories.len());
    for root in roots {
        let mut stack = vec![(root, 0usize)];
        while let Some((index, depth)) = stack.pop() {
            if !visited.insert(index) {
                continue;
            }
            result.push((index, depth));
            let parent = &categories[index].id;
            for (child, category) in categories.iter().enumerate().rev() {
                if category.parent_id.as_ref() == Some(parent) {
                    stack.push((child, depth + 1));
                }
            }
        }
    }
    result
}

fn category_type(value: Option<&str>) -> &str {
    match value {
        Some("income") => "Receita",
        Some("expense") => "Despesa",
        Some("transfer") => "Transferência",
        Some(value) => value,
        None => "—",
    }
}

fn build_tree_rows(categories: &[Category], theme: &Theme) -> Vec<TableRow> {
    category_order(categories)
        .into_iter()
        .map(|(index, depth)| {
            let category = &categories[index];
            let prefix = if depth == 0 {
                String::new()
            } else {
                format!("{}└ ", "  ".repeat(depth.saturating_sub(1).min(7)))
            };
            let mut row = TableRow::new([
                format!("{prefix}{}", category.name),
                category_type(category.category_type.as_deref()).into(),
                match category.kind.as_deref() {
                    Some("system") => "Padrão",
                    Some("custom") => "Pessoal",
                    Some(kind) => kind,
                    None => "—",
                }
                .into(),
                if category.hidden {
                    "Oculta".into()
                } else {
                    "Visível".into()
                },
            ]);
            row.cell_colors = vec![
                Some(if category.hidden {
                    theme.muted
                } else {
                    theme.foreground
                }),
                Some(match category.category_type.as_deref() {
                    Some("income") => theme.success,
                    Some("expense") => theme.danger,
                    _ => theme.muted,
                }),
                Some(theme.muted),
                Some(theme.muted),
            ];
            row
        })
        .collect()
}

pub fn render_with_state(ui: &mut Container<'_>, screen: &mut CategoriesScreen, api: &ApiClient) {
    screen.load_initial(api);
    render_with_period(ui, screen, &Ok(Vec::new().into()));
}

pub fn render_with_period(
    ui: &mut Container<'_>,
    screen: &mut CategoriesScreen,
    transactions: &Result<std::sync::Arc<[crate::models::Transaction]>, String>,
) {
    screen.interactions.clear();
    let hits = screen.interactions.clone();
    let summary = transactions
        .as_ref()
        .map(|rows| crate::period::summarize(rows));
    let theme = ui.theme().clone();
    let table_rows = build_tree_rows(&screen.categories, &theme);
    let mut table_rows = table_rows;
    let order = category_order(&screen.categories);
    for (row, (index, _)) in table_rows.iter_mut().zip(order) {
        let spending = summary
            .as_ref()
            .ok()
            .and_then(|summary| summary.categories.get(&screen.categories[index].id));
        let values = spending.map(|currencies| {
            currencies
                .iter()
                .filter(|(_, flow)| flow.expenses > 0)
                .map(|(currency, flow)| crate::period::money_cents(flow.expenses, currency))
                .collect::<Vec<_>>()
                .join(" / ")
        });
        row.cells
            .push(values.filter(|value| !value.is_empty()).unwrap_or_else(|| {
                if summary.is_ok() {
                    crate::period::money_cents(0, "BRL")
                } else {
                    "Indisponível".into()
                }
            }));
        row.cell_colors.push(Some(theme.danger));
    }
    let columns = vec![
        TableColumn::new("Nome").width(Size::Fill),
        TableColumn::new("Tipo")
            .width(Size::Cells(12))
            .align(Align::Left),
        TableColumn::new("Grupo").width(Size::Cells(9)),
        TableColumn::new("Exibição").width(Size::Cells(8)),
        TableColumn::new("Gastos diretos")
            .width(Size::Cells(21))
            .align(Align::Right),
    ];
    let selected_spending = screen
        .selected_index
        .and_then(|selected| category_order(&screen.categories).get(selected).copied())
        .and_then(|(index, _)| {
            summary.as_ref().ok().map(|summary| {
                let category = &screen.categories[index];
                let values = summary
                    .categories
                    .get(&category.id)
                    .map(|currencies| {
                        currencies
                            .iter()
                            .filter(|(_, flow)| flow.expenses > 0)
                            .map(|(currency, flow)| {
                                crate::period::money_cents(flow.expenses, currency)
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                (category.name.clone(), values)
            })
        });
    let period_error = transactions.as_ref().err().cloned();
    let period_summary = summary.ok();
    let transaction_count = transactions.as_ref().map_or(0, |rows| rows.len());
    let has_rows = !table_rows.is_empty();
    let cat_count = screen.categories.len();
    let panel_title = format!("Categorias ({cat_count})");
    let error_msg = screen.error.clone();
    let loading = screen.loading;
    let selected = screen.selected_index;
    let modal_state = screen.modal_state;
    let form_name = screen.form_name.clone();
    let form_type_index = screen.form_type_index;
    let form_parent_id = screen.form_parent_id.clone();
    let form_kind = screen.form_kind.clone();
    let form_hidden = screen.form_hidden;
    let form_field = screen.form_field;
    let form_error = screen.form_error.clone();
    let delete_target_name = screen.delete_target_name.clone();

    ui.column(Column::new().gap(1), {
        let hits = hits.clone();
        move |col| {
            if let Some(summary) = &period_summary {
                crate::presentation::period_summary(col, summary, transaction_count);
            }
            if let Some(error) = &period_error {
                col.label(&format!("Período indisponível. {error}"));
            }
            col.row(Row::new().size(Size::Cells(1)).gap(1), {
                let hits = hits.clone();
                move |row| {
                    row.mouse_button(
                        ButtonOptions::new("Nova categoria (n)").variant(ButtonVariant::Primary),
                        "categories:new",
                        &hits,
                    );
                    row.mouse_button(
                        ButtonOptions::new("Editar (e)").variant(ButtonVariant::Ghost),
                        "categories:edit",
                        &hits,
                    );
                    row.mouse_button(
                        ButtonOptions::new("Excluir (d)").variant(ButtonVariant::Ghost),
                        "categories:delete",
                        &hits,
                    );
                }
            });
            col.panel(
                Panel::new()
                    .title(panel_title)
                    .border(BorderStyle::Rounded)
                    .footer("j/k Selecionar  n Nova  e Editar  d Excluir"),
                {
                    let hits = hits.clone();
                    move |panel| {
                        if loading {
                            panel.label("Carregando suas categorias…");
                        } else if let Some(error) = error_msg {
                            panel.styled_text(&error, TextStyle::new().fg(theme.danger));
                        } else if has_rows {
                            panel.mouse_table(
                                TableOptions {
                                    columns,
                                    rows: table_rows,
                                    selected,
                                    follow_selection: true,
                                    scrollbar: true,
                                    header_color: Some(theme.muted),
                                    ..Default::default()
                                },
                                &hits,
                            );
                        } else {
                            panel.text("Organize seus lançamentos por categoria.");
                            panel.label("Pressione n para criar sua primeira categoria.");
                        }
                    }
                },
            );
            if let Some((name, values)) = selected_spending
                && !values.is_empty()
            {
                col.sized(
                    Size::Cells(i64::try_from(values.len() + 1).unwrap_or(i64::MAX)),
                    move |detail| {
                        detail.column(Column::new(), move |body| {
                            body.label(&format!("Gastos diretos · {name}"));
                            for value in values {
                                body.text(&value);
                            }
                        });
                    },
                );
            }
        }
    });

    // Draw modals
    match modal_state {
        CategoryModalState::Create | CategoryModalState::Edit => {
            let title = if modal_state == CategoryModalState::Create {
                "Nova Categoria"
            } else {
                "Editar Categoria"
            };

            let name_display = if form_field == CategoryFormField::Name {
                format!("{form_name}_")
            } else {
                form_name.clone()
            };

            let type_display = {
                let t = CATEGORY_TYPES[form_type_index];
                if form_field == CategoryFormField::Type {
                    format!("< {t} >")
                } else {
                    t.to_string()
                }
            };

            let parent_display = if form_field == CategoryFormField::Parent {
                format!("{form_parent_id}_")
            } else if form_parent_id.is_empty() {
                "(nenhuma)".to_string()
            } else {
                form_parent_id.clone()
            };

            let kind_display = if form_field == CategoryFormField::Kind {
                format!("{form_kind}_")
            } else if form_kind.is_empty() {
                "(nenhum)".to_string()
            } else {
                form_kind.clone()
            };

            let hidden_display = if form_field == CategoryFormField::Hidden {
                if form_hidden { "[X] Sim" } else { "[ ] Nao" }.to_string()
            } else {
                if form_hidden { "Sim" } else { "Nao" }.to_string()
            };

            let err = form_error.clone();

            let options = ModalOptions {
                title: Some(title.to_string()),
                message: Some(format!(
                    "Nome: {name_display}\nTipo: {type_display}\nPai (ID): {parent_display}\nKind: {kind_display}\nOculta: {hidden_display}\n\nTab: proximo campo | Enter: salvar | Esc: cancelar{}",
                    err.map(|e| format!("\n\n{e}")).unwrap_or_default()
                )),
                buttons: vec![ModalButton::new("Cancelar"), ModalButton::new("Salvar")],
                width: Some(68),
                align: Some(Align::Left),
                ..Default::default()
            };
            ui.mouse_modal(
                options,
                &["Nome:", "Tipo:", "Pai (ID):", "Kind:", "Oculta:"],
                &screen.interactions,
            );
        }
        CategoryModalState::DeleteConfirm => {
            let target = delete_target_name.unwrap_or_default();
            let options = ModalOptions {
                title: Some("Confirmar Exclusao".to_string()),
                message: Some(format!(
                    "Tem certeza que deseja excluir a categoria \"{target}\"?\n\nEsta acao nao pode ser desfeita."
                )),
                buttons: vec![ModalButton::new("Cancelar"), ModalButton::new("Excluir")],
                width: Some(68),
                align: Some(Align::Left),
                ..Default::default()
            };
            ui.mouse_modal(
                options,
                &["Nome:", "Tipo:", "Pai (ID):", "Kind:", "Oculta:"],
                &screen.interactions,
            );
        }
        CategoryModalState::None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_category_exposes_every_original_currency_at_compact_width() {
        let mut screen = CategoriesScreen::new();
        screen.categories = serde_json::from_value(
            serde_json::json!([{"id":"food", "name":"Mercado", "type":"expense"}]),
        )
        .unwrap();
        screen.selected_index = Some(0);
        let rows: std::sync::Arc<[crate::models::Transaction]> = serde_json::from_value::<Vec<crate::models::Transaction>>(serde_json::json!([
            {"id":"brl", "account_id":"a", "category_id":"food", "type":"debit", "amount":"1347.50", "currency_code":"BRL"},
            {"id":"usd", "account_id":"a", "category_id":"food", "type":"debit", "amount":"12.50", "currency_code":"USD"}
        ])).unwrap().into();
        let rendered = hqtui::testing::render_to_text(80, 20, "dark", |ui| {
            render_with_period(ui, &mut screen, &Ok(rows.clone()));
        });
        let detail = rendered
            .split("Gastos diretos · Mercado")
            .nth(1)
            .expect("selected category detail");
        assert!(detail.contains("1.347,50 BRL"), "{rendered}");
        assert!(detail.contains("12,50 USD"), "{rendered}");
        assert!(rendered.contains("Nova categoria"));
    }

    #[test]
    fn edit_and_delete_follow_the_displayed_hierarchy() {
        let mut screen = CategoriesScreen::new();
        screen.categories = serde_json::from_value(serde_json::json!([
            {"id": "child", "name": "Child", "parent_id": "alpha"},
            {"id": "alpha", "name": "Alpha"},
            {"id": "beta", "name": "Beta"},
            {"id": "orphan", "name": "Orphan", "parent_id": "missing"}
        ]))
        .unwrap();
        let order = category_order(&screen.categories);
        assert_eq!(order, vec![(1, 0), (0, 1), (2, 0), (3, 0)]);
        for (row, id) in ["alpha", "child", "beta", "orphan"].iter().enumerate() {
            screen.selected_index = Some(row);
            screen.open_edit_modal();
            assert_eq!(
                screen.form_name,
                screen
                    .categories
                    .iter()
                    .find(|category| &category.id == id)
                    .unwrap()
                    .name
            );
            screen.open_delete_confirm();
            assert_eq!(screen.delete_target_id.as_deref(), Some(*id));
        }
    }

    #[test]
    fn malformed_category_cycles_remain_selectable_once() {
        let categories: Vec<Category> = serde_json::from_value(serde_json::json!([
            {"id": "a", "name": "A", "parent_id": "b"},
            {"id": "b", "name": "B", "parent_id": "a"}
        ]))
        .unwrap();
        assert_eq!(category_order(&categories), vec![(0, 0), (1, 1)]);
    }
}
