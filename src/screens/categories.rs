use hqtui::prelude::*;
use hqtui::color::Color;
use hqtui::input::InputEvent;
use hqtui::widgets::{
    ButtonOptions, ButtonVariant, ModalButton, ModalOptions, TableOptions, TableColumn, TableRow,
    TextStyle,
};
use hqtui::unicode::Align;

use crate::api::ApiClient;
use crate::models::Category;

const COLOR_NEGATIVE: Color = Color::rgb(239, 68, 68);
const COLOR_NEUTRAL: Color = Color::rgb(148, 163, 184);

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
        match api.categories(self.include_hidden) {
            Ok(cats) => {
                self.categories = cats;
                self.loading = false;
            }
            Err(e) => {
                self.error = Some(format!("Falha ao carregar categorias: {e}"));
                self.loading = false;
            }
        }
    }

    pub fn handle_input(&mut self, app: &App, api: &ApiClient, events: &[InputEvent]) {
        if self.modal_state != CategoryModalState::None {
            self.handle_modal_input(app, api, events);
            return;
        }

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
        if app.pressed("categories:new") {
            self.open_create_modal();
        }
        if app.pressed("categories:edit") {
            self.open_edit_modal();
        }
        if app.pressed("categories:delete") {
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
            && idx < self.categories.len() {
                let cat = &self.categories[idx];
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
            && idx < self.categories.len() {
                let cat = &self.categories[idx];
                self.delete_target_id = Some(cat.id.clone());
                self.delete_target_name = Some(cat.name.clone());
                self.modal_state = CategoryModalState::DeleteConfirm;
            }
    }

    fn handle_modal_input(&mut self, app: &App, api: &ApiClient, events: &[InputEvent]) {
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
                        if k.name == "enter" {
                            self.submit_form(api);
                            return;
                        }
                        // Type field: left/right to cycle
                        if self.form_field == CategoryFormField::Type {
                            if k.name == "right" || k.name == "l" {
                                self.form_type_index = (self.form_type_index + 1) % CATEGORY_TYPES.len();
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
                            && (k.name == "space" || k.name == "y" || k.name == "n") {
                                self.form_hidden = !self.form_hidden;
                                return;
                            }
                        // Text fields: backspace, ctrl+u, char input
                        if k.key == "backspace" {
                            match self.form_field {
                                CategoryFormField::Name => { self.form_name.pop(); }
                                CategoryFormField::Parent => { self.form_parent_id.pop(); }
                                CategoryFormField::Kind => { self.form_kind.pop(); }
                                _ => {}
                            }
                            return;
                        }
                        if k.key == "ctrl+u" {
                            match self.form_field {
                                CategoryFormField::Name => { self.form_name.clear(); }
                                CategoryFormField::Parent => { self.form_parent_id.clear(); }
                                CategoryFormField::Kind => { self.form_kind.clear(); }
                                _ => {}
                            }
                            return;
                        }
                        if let Some(ch) = &k.char
                            && !k.ctrl && !k.alt {
                                match self.form_field {
                                    CategoryFormField::Name => { self.form_name.push_str(ch); }
                                    CategoryFormField::Parent => { self.form_parent_id.push_str(ch); }
                                    CategoryFormField::Kind => { self.form_kind.push_str(ch); }
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
                if app.pressed("cat_delete_cancel") {
                    self.modal_state = CategoryModalState::None;
                    self.delete_target_id = None;
                    self.delete_target_name = None;
                }
                if app.pressed("cat_delete_confirm") {
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
                    self.fetch_categories(api);
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
            && !self.categories.iter().any(|c| c.id == pid) {
                self.form_error = Some(format!("Categoria pai nao encontrada: {pid}"));
                return;
            }

        let kind = if self.form_kind.trim().is_empty() {
            None
        } else {
            Some(self.form_kind.trim())
        };

        let result = match self.modal_state {
            CategoryModalState::Create => {
                api.create_category(name, category_type, parent_id, kind)
            }
            CategoryModalState::Edit => {
                if let Some(idx) = self.selected_index {
                    if idx < self.categories.len() {
                        let cat_id = self.categories[idx].id.clone();
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
                self.fetch_categories(api);
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

/// Build a flat display list with indentation based on `parent_id` hierarchy.
fn build_tree_rows(categories: &[Category]) -> Vec<TableRow> {
    // Build adjacency: parent_id -> children indices
    let mut children_map: std::collections::HashMap<Option<&str>, Vec<usize>> =
        std::collections::HashMap::new();
    for (i, cat) in categories.iter().enumerate() {
        children_map
            .entry(cat.parent_id.as_deref())
            .or_default()
            .push(i);
    }

    let mut rows = Vec::new();
    let mut stack: Vec<(usize, usize)> = Vec::new(); // (category index, depth)

    // Start with root categories (no parent)
    if let Some(roots) = children_map.get(&None) {
        for &idx in roots {
            stack.push((idx, 0));
        }
    }

    while let Some((idx, depth)) = stack.pop() {
        let cat = &categories[idx];
        let indent = "  ".repeat(depth);
        let type_label = cat.category_type.as_deref().unwrap_or("—");
        let hidden_mark = if cat.hidden { " (oculta)" } else { "" };
        let display_name = format!("{indent}{}{hidden_mark}", cat.name);

        rows.push(TableRow::new([
            display_name,
            type_label.to_string(),
            cat.kind.clone().unwrap_or_else(|| "—".to_string()),
        ]));

        // Push children in reverse order so they pop in original order
        if let Some(children) = children_map.get(&Some(cat.id.as_str())) {
            for &child_idx in children.iter().rev() {
                stack.push((child_idx, depth + 1));
            }
        }
    }

    rows
}

pub fn render_with_state(ui: &mut Container<'_>, screen: &mut CategoriesScreen, api: &ApiClient) {
    screen.load_initial(api);

    let table_rows = build_tree_rows(&screen.categories);

    let columns = vec![
        TableColumn::new("Nome").width(Size::Fill),
        TableColumn::new("Tipo").width(Size::Cells(12)).align(Align::Left),
        TableColumn::new("Kind").width(Size::Cells(12)).align(Align::Left),
    ];

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

    ui.column(Column::new().gap(1), move |col| {
        // Action buttons row
        col.row(Row::new().gap(1), move |r| {
            r.button(
                ButtonOptions::new("Nova Categoria (n)").variant(ButtonVariant::Primary),
                "categories:new",
            );
            r.button(
                ButtonOptions::new("Editar (e)").variant(ButtonVariant::Ghost),
                "categories:edit",
            );
            r.button(
                ButtonOptions::new("Excluir (d)").variant(ButtonVariant::Danger),
                "categories:delete",
            );
        });

        // Error display
        if let Some(err) = error_msg {
            col.panel(Panel::new().title("Erro"), move |p| {
                p.styled_text(&err, TextStyle::new().fg(COLOR_NEGATIVE));
            });
        }

        // Categories table
        col.panel(Panel::new().title(panel_title), move |p| {
            if loading {
                p.text("Carregando...");
            } else if has_rows {
                p.table(
                    TableOptions {
                        columns: columns.clone(),
                        rows: table_rows.clone(),
                        header: true,
                        zebra: true,
                        selected,
                        ..Default::default()
                    },
                    "",
                );
            } else {
                p.styled_text(
                    "Nenhuma categoria encontrada. Pressione 'n' para criar.",
                    TextStyle::new().fg(COLOR_NEUTRAL),
                );
            }
        });

        // Key hints
        col.row(Row::new(), move |r| {
            r.styled_text(
                "n: Nova | e: Editar | d: Excluir | ↑↓: Navegar",
                TextStyle::new().fg(COLOR_NEUTRAL),
            );
        });
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

            ui.modal(ModalOptions {
                title: Some(title.to_string()),
                message: Some(format!(
                    "Nome: {name_display}\nTipo: {type_display}\nPai (ID): {parent_display}\nKind: {kind_display}\nOculta: {hidden_display}\n\nTab: proximo campo | Enter: salvar | Esc: cancelar{}",
                    err.map(|e| format!("\n\n{e}")).unwrap_or_default()
                )),
                buttons: vec![
                    ModalButton::new("Cancelar"),
                    ModalButton::new("Salvar"),
                ],
                ..Default::default()
            });
        }
        CategoryModalState::DeleteConfirm => {
            let target = delete_target_name.unwrap_or_default();
            ui.modal(ModalOptions {
                title: Some("Confirmar Exclusao".to_string()),
                message: Some(format!(
                    "Tem certeza que deseja excluir a categoria \"{target}\"?\n\nEsta acao nao pode ser desfeita."
                )),
                buttons: vec![
                    ModalButton::new("Cancelar"),
                    ModalButton::new("Excluir"),
                ],
                ..Default::default()
            });
        }
        CategoryModalState::None => {}
    }
}
