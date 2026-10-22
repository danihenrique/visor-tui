use hqtui::color::Color;
use hqtui::input::InputEvent;
use hqtui::prelude::*;
use hqtui::unicode::Align;
use hqtui::widgets::{
    ButtonOptions, ButtonVariant, ModalButton, ModalOptions, TableColumn, TableOptions, TableRow,
    TextStyle,
};

use crate::api::ApiClient;
use crate::models::Tag;
use crate::presentation::{Metric, Tone, metric_strip};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagModalState {
    None,
    Create,
    Edit,
    DeleteConfirm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagFormField {
    Name,
    Color,
}

impl TagFormField {
    fn next(self) -> Self {
        match self {
            Self::Name => Self::Color,
            Self::Color => Self::Name,
        }
    }

    fn prev(self) -> Self {
        self.next()
    }
}

pub struct TagsScreen {
    pub tags: Vec<Tag>,
    pub loading: bool,
    pub error: Option<String>,
    pub selected_index: Option<usize>,
    pub modal_state: TagModalState,
    pub form_name: String,
    pub form_color: String,
    pub form_field: TagFormField,
    pub form_error: Option<String>,
    pub delete_target_id: Option<String>,
    pub delete_target_name: Option<String>,
    pub loaded: bool,
    dialog: Option<ModalOptions>,
}

impl TagsScreen {
    #[must_use]
    pub fn new() -> Self {
        Self {
            tags: Vec::new(),
            loading: false,
            error: None,
            selected_index: None,
            modal_state: TagModalState::None,
            form_name: String::new(),
            form_color: String::new(),
            form_field: TagFormField::Name,
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
        self.fetch_tags(api);
    }

    pub fn fetch_tags(&mut self, api: &ApiClient) {
        self.loading = true;
        self.error = None;
        match api.tags() {
            Ok(tags) => {
                self.tags = tags;
                self.selected_index = (!self.tags.is_empty()).then_some(
                    self.selected_index
                        .unwrap_or(0)
                        .min(self.tags.len().saturating_sub(1)),
                );
                self.loading = false;
            }
            Err(e) => {
                self.error = Some(format!("Falha ao carregar tags: {e}"));
                self.loading = false;
            }
        }
    }

    pub fn paste_text(&mut self, text: &str) {
        if !matches!(
            self.modal_state,
            TagModalState::Create | TagModalState::Edit
        ) {
            return;
        }
        let text: String = text
            .chars()
            .filter(|character| !character.is_control())
            .collect();
        match self.form_field {
            TagFormField::Name => self.form_name.push_str(&text),
            TagFormField::Color => self.form_color.push_str(&text),
        }
    }

    pub fn handle_input(&mut self, app: &App, api: &ApiClient, events: &[InputEvent]) {
        if self.modal_state != TagModalState::None {
            let modal_events = crate::presentation::modal_events(
                events,
                app.terminal.size(),
                self.dialog.as_ref(),
            );
            self.handle_modal_input(app, api, &modal_events);
            return;
        }

        // Navigation within tag list
        for event in events {
            if let InputEvent::Key(k) = event {
                if k.name == "down" || k.name == "j" {
                    if !self.tags.is_empty() {
                        let next = match self.selected_index {
                            Some(i) => (i + 1).min(self.tags.len() - 1),
                            None => 0,
                        };
                        self.selected_index = Some(next);
                    }
                    return;
                }
                if k.name == "up" || k.name == "k" {
                    if !self.tags.is_empty() {
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
                        && idx < self.tags.len()
                    {
                        let name = self.tags[idx].name.clone();
                        let color = self.tags[idx].color.clone().unwrap_or_default();
                        self.form_name = name;
                        self.form_color = color;
                        self.form_field = TagFormField::Name;
                        self.form_error = None;
                        self.modal_state = TagModalState::Edit;
                    }
                    return;
                }
                if k.name == "d" || k.name == "delete" {
                    if let Some(idx) = self.selected_index
                        && idx < self.tags.len()
                    {
                        let tag = &self.tags[idx];
                        self.delete_target_id = Some(tag.id.clone());
                        self.delete_target_name = Some(tag.name.clone());
                        self.modal_state = TagModalState::DeleteConfirm;
                    }
                    return;
                }
            }
        }

        // Button presses
        if app.pressed("tags:new") {
            self.open_create_modal();
        }
        if app.pressed("tags:edit")
            && let Some(idx) = self.selected_index
            && idx < self.tags.len()
        {
            let name = self.tags[idx].name.clone();
            let color = self.tags[idx].color.clone().unwrap_or_default();
            self.form_name = name;
            self.form_color = color;
            self.form_field = TagFormField::Name;
            self.form_error = None;
            self.modal_state = TagModalState::Edit;
        }
        if app.pressed("tags:delete")
            && let Some(idx) = self.selected_index
            && idx < self.tags.len()
        {
            let tag = &self.tags[idx];
            self.delete_target_id = Some(tag.id.clone());
            self.delete_target_name = Some(tag.name.clone());
            self.modal_state = TagModalState::DeleteConfirm;
        }
    }

    fn open_create_modal(&mut self) {
        self.form_name.clear();
        self.form_color.clear();
        self.form_field = TagFormField::Name;
        self.form_error = None;
        self.modal_state = TagModalState::Create;
    }

    fn handle_modal_input(&mut self, app: &App, api: &ApiClient, events: &[InputEvent]) {
        match self.modal_state {
            TagModalState::Create | TagModalState::Edit => {
                for event in events {
                    if let InputEvent::Key(k) = event {
                        if k.name == "escape" {
                            self.modal_state = TagModalState::None;
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
                        if k.key == "backspace" {
                            match self.form_field {
                                TagFormField::Name => {
                                    self.form_name.pop();
                                }
                                TagFormField::Color => {
                                    self.form_color.pop();
                                }
                            }
                            return;
                        }
                        if k.key == "ctrl+u" {
                            match self.form_field {
                                TagFormField::Name => {
                                    self.form_name.clear();
                                }
                                TagFormField::Color => {
                                    self.form_color.clear();
                                }
                            }
                            return;
                        }
                        if let Some(ch) = &k.char
                            && !k.ctrl
                            && !k.alt
                        {
                            match self.form_field {
                                TagFormField::Name => {
                                    self.form_name.push_str(ch);
                                }
                                TagFormField::Color => {
                                    self.form_color.push_str(ch);
                                }
                            }
                        }
                    }
                }
            }
            TagModalState::DeleteConfirm => {
                for event in events {
                    if let InputEvent::Key(k) = event {
                        if k.name == "escape" || k.name == "n" {
                            self.modal_state = TagModalState::None;
                            self.delete_target_id = None;
                            self.delete_target_name = None;
                            return;
                        }
                        if k.name == "y" || k.name == "enter" {
                            if let Some(ref id) = self.delete_target_id {
                                match api.delete_tag(id) {
                                    Ok(()) => {
                                        self.fetch_tags(api);
                                    }
                                    Err(e) => {
                                        self.form_error = Some(format!("Erro ao excluir: {e}"));
                                    }
                                }
                            }
                            self.modal_state = TagModalState::None;
                            self.delete_target_id = None;
                            self.delete_target_name = None;
                            return;
                        }
                    }
                }
                if app.pressed("tag_delete_cancel") {
                    self.modal_state = TagModalState::None;
                    self.delete_target_id = None;
                    self.delete_target_name = None;
                }
                if app.pressed("tag_delete_confirm") {
                    if let Some(ref id) = self.delete_target_id {
                        match api.delete_tag(id) {
                            Ok(()) => {
                                self.fetch_tags(api);
                            }
                            Err(e) => {
                                self.form_error = Some(format!("Erro ao excluir: {e}"));
                            }
                        }
                    }
                    self.modal_state = TagModalState::None;
                    self.delete_target_id = None;
                    self.delete_target_name = None;
                }
            }
            TagModalState::None => {}
        }
    }

    fn submit_form(&mut self, api: &ApiClient) {
        let name = self.form_name.trim();
        if name.is_empty() {
            self.form_error = Some("Nome é obrigatório.".to_string());
            return;
        }

        let color = if self.form_color.trim().is_empty() {
            None
        } else {
            Some(self.form_color.trim())
        };

        let result = match self.modal_state {
            TagModalState::Create => api.create_tag(name, color),
            TagModalState::Edit => {
                if let Some(idx) = self.selected_index {
                    if idx < self.tags.len() {
                        let tag_id = self.tags[idx].id.clone();
                        api.update_tag(&tag_id, Some(name), color)
                    } else {
                        self.form_error = Some("Tag não encontrada.".to_string());
                        return;
                    }
                } else {
                    self.form_error = Some("Nenhuma tag selecionada.".to_string());
                    return;
                }
            }
            _ => return,
        };

        match result {
            Ok(_) => {
                self.modal_state = TagModalState::None;
                self.fetch_tags(api);
            }
            Err(e) => {
                self.form_error = Some(format!("Erro: {e}"));
            }
        }
    }
}

impl Default for TagsScreen {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_color(color: &str) -> Option<Color> {
    let color = color.trim().strip_prefix('#').unwrap_or(color.trim());
    if color.len() != 6 || !color.is_ascii() {
        return None;
    }
    u32::from_str_radix(color, 16).ok().map(Color::hex)
}

fn tag_row(tag: &Tag, theme: &Theme) -> TableRow {
    let color = tag.color.as_deref().and_then(parse_color);
    let mut row = TableRow::new([
        if color.is_some() {
            "●".into()
        } else {
            "○".into()
        },
        tag.name.clone(),
        tag.color.clone().unwrap_or_else(|| "Padrão".into()),
    ]);
    row.cell_colors = vec![Some(color.unwrap_or(theme.muted)), None, Some(theme.muted)];
    row
}

pub fn render_with_state(ui: &mut Container<'_>, screen: &mut TagsScreen, api: &ApiClient) {
    screen.load_initial(api);

    let theme = ui.theme().clone();
    let table_rows: Vec<TableRow> = screen.tags.iter().map(|tag| tag_row(tag, &theme)).collect();
    let colored = screen
        .tags
        .iter()
        .filter(|tag| tag.color.as_deref().and_then(parse_color).is_some())
        .count();
    let metrics = vec![
        Metric::new("Tags", screen.tags.len().to_string(), Tone::Accent),
        Metric::new("Com cor", colored.to_string(), Tone::Neutral),
        Metric::new(
            "Cor padrão",
            (screen.tags.len() - colored).to_string(),
            Tone::Neutral,
        ),
    ];

    let columns = vec![
        TableColumn::new("")
            .width(Size::Cells(3))
            .align(Align::Center),
        TableColumn::new("Nome").width(Size::Fill),
        TableColumn::new("Cor")
            .width(Size::Cells(12))
            .align(Align::Left),
    ];

    let has_rows = !table_rows.is_empty();
    let tag_count = screen.tags.len();
    let panel_title = format!("Tags ({tag_count})");
    let error_msg = screen.error.clone();
    let loading = screen.loading;
    let selected = screen.selected_index;
    let modal_state = screen.modal_state;
    let form_name = screen.form_name.clone();
    let form_color = screen.form_color.clone();
    let form_field = screen.form_field;
    let form_error = screen.form_error.clone();
    let delete_target_name = screen.delete_target_name.clone();

    ui.column(Column::new().gap(1), move |col| {
        metric_strip(col, metrics);
        col.row(Row::new().size(Size::Cells(1)).gap(1), move |row| {
            row.button(
                ButtonOptions::new("Nova tag (n)").variant(ButtonVariant::Primary),
                "tags:new",
            );
            row.button(
                ButtonOptions::new("Editar (e)").variant(ButtonVariant::Ghost),
                "tags:edit",
            );
            row.button(
                ButtonOptions::new("Excluir (d)").variant(ButtonVariant::Ghost),
                "tags:delete",
            );
        });
        col.panel(
            Panel::new()
                .title(panel_title)
                .border(BorderStyle::Rounded)
                .footer("j/k Selecionar  n Nova  e Editar  d Excluir"),
            move |panel| {
                if loading {
                    panel.label("Carregando suas tags…");
                } else if let Some(error) = error_msg {
                    panel.styled_text(&error, TextStyle::new().fg(theme.danger));
                } else if has_rows {
                    panel.table(
                        TableOptions {
                            columns,
                            rows: table_rows,
                            selected,
                            follow_selection: true,
                            scrollbar: true,
                            header_color: Some(theme.muted),
                            ..Default::default()
                        },
                        "tags:table",
                    );
                } else {
                    panel.text("Marque lançamentos para encontrá-los depois.");
                    panel.label("Pressione n para criar sua primeira tag.");
                }
            },
        );
    });

    // Draw modals
    match modal_state {
        TagModalState::Create => {
            let name_display = if form_field == TagFormField::Name {
                format!("{form_name}_")
            } else {
                form_name.clone()
            };
            let color_display = if form_field == TagFormField::Color {
                format!("{form_color}_")
            } else {
                form_color.clone()
            };
            let err = form_error.clone();

            let options = ModalOptions {
                title: Some("Nova Tag".to_string()),
                message: Some(format!(
                    "Nome: {name_display}\nCor (hex): {color_display}\n\nTab: próximo campo | Enter: salvar | Esc: cancelar{}",
                    err.map(|e| format!("\n\n{e}")).unwrap_or_default()
                )),
                buttons: vec![ModalButton::new("Cancelar"), ModalButton::new("Salvar")],
                width: Some(68),
                align: Some(Align::Left),
                ..Default::default()
            };
            screen.dialog = Some(options.clone());
            ui.modal(options);
        }
        TagModalState::Edit => {
            let name_display = if form_field == TagFormField::Name {
                format!("{form_name}_")
            } else {
                form_name.clone()
            };
            let color_display = if form_field == TagFormField::Color {
                format!("{form_color}_")
            } else {
                form_color.clone()
            };
            let err = form_error.clone();

            let options = ModalOptions {
                title: Some("Editar Tag".to_string()),
                message: Some(format!(
                    "Nome: {name_display}\nCor (hex): {color_display}\n\nTab: próximo campo | Enter: salvar | Esc: cancelar{}",
                    err.map(|e| format!("\n\n{e}")).unwrap_or_default()
                )),
                buttons: vec![ModalButton::new("Cancelar"), ModalButton::new("Salvar")],
                width: Some(68),
                align: Some(Align::Left),
                ..Default::default()
            };
            screen.dialog = Some(options.clone());
            ui.modal(options);
        }
        TagModalState::DeleteConfirm => {
            let target = delete_target_name.unwrap_or_default();
            let options = ModalOptions {
                title: Some("Confirmar Exclusão".to_string()),
                message: Some(format!(
                    "Tem certeza que deseja excluir a tag \"{target}\"?\n\nEsta ação não pode ser desfeita."
                )),
                buttons: vec![ModalButton::new("Cancelar"), ModalButton::new("Excluir")],
                width: Some(68),
                align: Some(Align::Left),
                ..Default::default()
            };
            screen.dialog = Some(options.clone());
            ui.modal(options);
        }
        TagModalState::None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_colors_are_native_cells_without_escape_sequences() {
        let tag = Tag {
            id: "t".into(),
            name: "Viagem".into(),
            color: Some("#34d399".into()),
        };
        let row = tag_row(&tag, &crate::presentation::theme());
        assert!(row.cells.iter().all(|cell| !cell.contains('\u{1b}')));
        assert_eq!(row.cells[0], "●");
        assert_eq!(row.cell_colors[0], Some(Color::hex(0x0034_d399)));
    }

    #[test]
    fn non_ascii_or_invalid_tag_colors_fall_back_without_panicking() {
        assert_eq!(parse_color("aéaaa"), None);
        assert_eq!(parse_color("#zzzzzz"), None);
        assert_eq!(parse_color("#fff"), None);
        assert_eq!(parse_color("#112233"), Some(Color::hex(0x0011_2233)));
    }
}
