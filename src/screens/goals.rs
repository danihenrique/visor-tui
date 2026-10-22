use hqtui::prelude::*;
use hqtui::color::Color;
use hqtui::input::InputEvent;
use hqtui::widgets::{
    ButtonOptions, ButtonVariant, KeyValueOptions, KeyValueRow, ModalButton, ModalOptions,
    TextStyle,
};

use crate::api::ApiClient;
use crate::models::Goal;

const COLOR_NEGATIVE: Color = Color::rgb(239, 68, 68);

const KIND_OPTIONS: &[&str] = &["savings", "debt", "investment", "custom"];
const STATUS_OPTIONS: &[&str] = &["active", "archived", "completed"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalModalState {
    None,
    Create,
    Edit,
    DeleteConfirm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalFormField {
    Name,
    TargetAmount,
    Kind,
    TargetDate,
    Status,
}

impl GoalFormField {
    fn next(self, is_create: bool) -> Self {
        if is_create {
            match self {
                Self::Name => Self::TargetAmount,
                Self::TargetAmount => Self::Kind,
                Self::Kind => Self::TargetDate,
                Self::TargetDate | Self::Status => Self::Name,
            }
        } else {
            match self {
                Self::Name => Self::TargetAmount,
                Self::TargetAmount => Self::Status,
                Self::Status => Self::TargetDate,
                Self::TargetDate | Self::Kind => Self::Name,
            }
        }
    }
    fn prev(self, is_create: bool) -> Self {
        if is_create {
            match self {
                Self::Name | Self::Status => Self::TargetDate,
                Self::TargetAmount => Self::Name,
                Self::Kind => Self::TargetAmount,
                Self::TargetDate => Self::Kind,
            }
        } else {
            match self {
                Self::Name | Self::Kind => Self::TargetDate,
                Self::TargetAmount => Self::Name,
                Self::Status => Self::TargetAmount,
                Self::TargetDate => Self::Status,
            }
        }
    }
}

pub struct GoalsScreen {
    pub goals: Vec<Goal>,
    pub loading: bool,
    pub error: Option<String>,
    pub selected_index: Option<usize>,
    pub modal_state: GoalModalState,
    pub form_name: String,
    pub form_target_amount: String,
    pub form_kind_index: usize,
    pub form_status_index: usize,
    pub form_target_date: String,
    pub form_field: GoalFormField,
    pub form_error: Option<String>,
    pub delete_target_id: Option<String>,
    pub delete_target_name: Option<String>,
    pub delete_target_progress: Option<String>,
    pub loaded: bool,
}

impl GoalsScreen {
    #[must_use]
    pub fn new() -> Self {
        Self {
            goals: Vec::new(),
            loading: false,
            error: None,
            selected_index: None,
            modal_state: GoalModalState::None,
            form_name: String::new(),
            form_target_amount: String::new(),
            form_kind_index: 0,
            form_status_index: 0,
            form_target_date: String::new(),
            form_field: GoalFormField::Name,
            form_error: None,
            delete_target_id: None,
            delete_target_name: None,
            delete_target_progress: None,
            loaded: false,
        }
    }

    pub fn load_initial(&mut self, api: &ApiClient) {
        if self.loaded { return; }
        self.loaded = true;
        self.fetch_goals(api);
    }

    pub fn fetch_goals(&mut self, api: &ApiClient) {
        self.loading = true;
        self.error = None;
        match api.goals(None) {
            Ok(goals) => { self.goals = goals; self.loading = false; }
            Err(e) => { self.error = Some(format!("Falha ao carregar metas: {e}")); self.loading = false; }
        }
    }

    pub fn handle_input(&mut self, app: &App, api: &ApiClient, events: &[InputEvent]) {
        if self.modal_state != GoalModalState::None {
            self.handle_modal_input(app, api, events);
            return;
        }
        for event in events {
            if let InputEvent::Key(k) = event {
                if k.name == "down" || k.name == "j" {
                    if !self.goals.is_empty() {
                        let next = match self.selected_index {
                            Some(i) => (i + 1).min(self.goals.len() - 1),
                            None => 0,
                        };
                        self.selected_index = Some(next);
                    }
                    return;
                }
                if k.name == "up" || k.name == "k" {
                    if !self.goals.is_empty() {
                        let prev = match self.selected_index {
                            Some(i) => i.saturating_sub(1),
                            None => 0,
                        };
                        self.selected_index = Some(prev);
                    }
                    return;
                }
                if k.name == "n" { self.open_create_modal(); return; }
                if k.name == "e" {
                    if let Some(idx) = self.selected_index
                        && idx < self.goals.len() { self.populate_edit_form(idx); }
                    return;
                }
                if k.name == "d" || k.name == "delete" {
                    if let Some(idx) = self.selected_index
                        && idx < self.goals.len() {
                            let goal = &self.goals[idx];
                            self.delete_target_id = Some(goal.id.clone());
                            self.delete_target_name = Some(goal.name.clone());
                            let pct = goal.progress_percent.unwrap_or(0.0);
                            self.delete_target_progress = Some(format!("{pct:.1}%"));
                            self.modal_state = GoalModalState::DeleteConfirm;
                        }
                    return;
                }
            }
        }
        if app.pressed("goals:new") { self.open_create_modal(); }
        if app.pressed("goals:edit")
            && let Some(idx) = self.selected_index
                && idx < self.goals.len() { self.populate_edit_form(idx); }
        if app.pressed("goals:delete")
            && let Some(idx) = self.selected_index
                && idx < self.goals.len() {
                    let goal = &self.goals[idx];
                    self.delete_target_id = Some(goal.id.clone());
                    self.delete_target_name = Some(goal.name.clone());
                    let pct = goal.progress_percent.unwrap_or(0.0);
                    self.delete_target_progress = Some(format!("{pct:.1}%"));
                    self.modal_state = GoalModalState::DeleteConfirm;
                }
    }

    fn populate_edit_form(&mut self, idx: usize) {
        let goal = &self.goals[idx];
        self.form_name = goal.name.clone();
        self.form_target_amount = goal.target_amount.clone().unwrap_or_default();
        self.form_target_date = goal.target_date.clone().unwrap_or_default();
        self.form_kind_index = goal.kind.as_deref()
            .and_then(|k| KIND_OPTIONS.iter().position(|&o| o == k)).unwrap_or(0);
        self.form_status_index = goal.status.as_deref()
            .and_then(|s| STATUS_OPTIONS.iter().position(|&o| o == s)).unwrap_or(0);
        self.form_field = GoalFormField::Name;
        self.form_error = None;
        self.modal_state = GoalModalState::Edit;
    }

    fn open_create_modal(&mut self) {
        self.form_name.clear();
        self.form_target_amount.clear();
        self.form_target_date.clear();
        self.form_kind_index = 0;
        self.form_status_index = 0;
        self.form_field = GoalFormField::Name;
        self.form_error = None;
        self.modal_state = GoalModalState::Create;
    }

    fn handle_modal_input(&mut self, app: &App, api: &ApiClient, events: &[InputEvent]) {
        match self.modal_state {
            GoalModalState::Create | GoalModalState::Edit => {
                let is_create = self.modal_state == GoalModalState::Create;
                for event in events {
                    if let InputEvent::Key(k) = event {
                        if k.name == "escape" { self.modal_state = GoalModalState::None; return; }
                        if k.name == "tab" && !k.shift { self.form_field = self.form_field.next(is_create); return; }
                        if k.name == "tab" && k.shift { self.form_field = self.form_field.prev(is_create); return; }
                        if k.name == "enter" { self.submit_form(api); return; }
                        if self.form_field == GoalFormField::Kind && is_create {
                            if k.name == "right" || k.name == "l" {
                                self.form_kind_index = (self.form_kind_index + 1) % KIND_OPTIONS.len(); return;
                            }
                            if k.name == "left" || k.name == "h" {
                                self.form_kind_index = if self.form_kind_index == 0 { KIND_OPTIONS.len() - 1 } else { self.form_kind_index - 1 }; return;
                            }
                        }
                        if self.form_field == GoalFormField::Status {
                            if k.name == "right" || k.name == "l" {
                                self.form_status_index = (self.form_status_index + 1) % STATUS_OPTIONS.len(); return;
                            }
                            if k.name == "left" || k.name == "h" {
                                self.form_status_index = if self.form_status_index == 0 { STATUS_OPTIONS.len() - 1 } else { self.form_status_index - 1 }; return;
                            }
                        }
                        if k.key == "backspace" {
                            match self.form_field {
                                GoalFormField::Name => { self.form_name.pop(); }
                                GoalFormField::TargetAmount => { self.form_target_amount.pop(); }
                                GoalFormField::TargetDate => { self.form_target_date.pop(); }
                                _ => {}
                            }
                            return;
                        }
                        if k.key == "ctrl+u" {
                            match self.form_field {
                                GoalFormField::Name => { self.form_name.clear(); }
                                GoalFormField::TargetAmount => { self.form_target_amount.clear(); }
                                GoalFormField::TargetDate => { self.form_target_date.clear(); }
                                _ => {}
                            }
                            return;
                        }
                        if let Some(ch) = &k.char
                            && !k.ctrl && !k.alt {
                                match self.form_field {
                                    GoalFormField::Name => { self.form_name.push_str(ch); }
                                    GoalFormField::TargetAmount => { self.form_target_amount.push_str(ch); }
                                    GoalFormField::TargetDate => { self.form_target_date.push_str(ch); }
                                    _ => {}
                                }
                            }
                    }
                }
                if app.pressed("goal_form_cancel") { self.modal_state = GoalModalState::None; }
                if app.pressed("goal_form_save") { self.submit_form(api); }
            }
            GoalModalState::DeleteConfirm => {
                for event in events {
                    if let InputEvent::Key(k) = event {
                        if k.name == "escape" || k.name == "n" {
                            self.modal_state = GoalModalState::None;
                            self.delete_target_id = None;
                            self.delete_target_name = None;
                            self.delete_target_progress = None;
                            return;
                        }
                        if k.name == "y" || k.name == "enter" {
                            if let Some(ref id) = self.delete_target_id {
                                match api.delete_goal(id) {
                                    Ok(()) => { self.fetch_goals(api); }
                                    Err(e) => { self.form_error = Some(format!("Erro ao excluir: {e}")); }
                                }
                            }
                            self.modal_state = GoalModalState::None;
                            self.delete_target_id = None;
                            self.delete_target_name = None;
                            self.delete_target_progress = None;
                            return;
                        }
                    }
                }
                if app.pressed("goal_delete_cancel") {
                    self.modal_state = GoalModalState::None;
                    self.delete_target_id = None;
                    self.delete_target_name = None;
                    self.delete_target_progress = None;
                }
                if app.pressed("goal_delete_confirm") {
                    if let Some(ref id) = self.delete_target_id {
                        match api.delete_goal(id) {
                            Ok(()) => { self.fetch_goals(api); }
                            Err(e) => { self.form_error = Some(format!("Erro ao excluir: {e}")); }
                        }
                    }
                    self.modal_state = GoalModalState::None;
                    self.delete_target_id = None;
                    self.delete_target_name = None;
                    self.delete_target_progress = None;
                }
            }
            GoalModalState::None => {}
        }
    }

    fn submit_form(&mut self, api: &ApiClient) {
        let name = self.form_name.trim();
        if name.is_empty() {
            self.form_error = Some("Nome \u{e9} obrigat\u{f3}rio.".to_string());
            return;
        }
        let target_amount = self.form_target_amount.trim();
        if target_amount.is_empty() {
            self.form_error = Some("Valor alvo \u{e9} obrigat\u{f3}rio.".to_string());
            return;
        }
        let result = match self.modal_state {
            GoalModalState::Create => {
                let kind = KIND_OPTIONS.get(self.form_kind_index).copied();
                let td = self.form_target_date.trim();
                let target_date = if td.is_empty() { None } else { Some(td) };
                api.create_goal(name, target_amount, kind, target_date, None)
            }
            GoalModalState::Edit => {
                if let Some(idx) = self.selected_index {
                    if idx < self.goals.len() {
                        let goal_id = self.goals[idx].id.clone();
                        let status = STATUS_OPTIONS.get(self.form_status_index).copied();
                        let td = self.form_target_date.trim();
                        let target_date = if td.is_empty() { None } else { Some(td) };
                        api.update_goal(&goal_id, Some(name), Some(target_amount), status, target_date)
                    } else {
                        self.form_error = Some("Meta n\u{e3}o encontrada.".to_string());
                        return;
                    }
                } else {
                    self.form_error = Some("Nenhuma meta selecionada.".to_string());
                    return;
                }
            }
            _ => return,
        };
        match result {
            Ok(_) => { self.modal_state = GoalModalState::None; self.fetch_goals(api); }
            Err(e) => { self.form_error = Some(format!("Erro: {e}")); }
        }
    }
}

impl Default for GoalsScreen {
    fn default() -> Self {
        Self::new()
    }
}

fn format_brl(amount_str: &str) -> String {
    let amount_str = amount_str.trim();
    let clean_str = amount_str
        .replace(['.', ',', ' '], "");
    if let Ok(amount) = clean_str.parse::<f64>() {
        let formatted = format!("{amount:.2}");
        let parts: Vec<&str> = formatted.split('.').collect();
        if parts.len() == 2 {
            let integer_part = parts[0];
            let decimal_part = parts[1];
            let mut result = String::new();
            let chars: Vec<char> = integer_part.chars().collect();
            for (i, c) in chars.iter().enumerate() {
                if i > 0 && (chars.len() - i).is_multiple_of(3) {
                    result.push('.');
                }
                result.push(*c);
            }
            format!("{result},{decimal_part}")
        } else {
            format!("{},00", parts[0])
        }
    } else {
        amount_str.to_string()
    }
}

fn status_badge(status: Option<&str>) -> String {
    match status {
        Some("active") => "[ATIVO]".to_string(),
        Some("archived") => "[ARQUIVADO]".to_string(),
        Some("completed") => "[CONCLU\u{cd}DO]".to_string(),
        Some(s) => format!("[{}]", s.to_uppercase()),
        None => "[SEM STATUS]".to_string(),
    }
}

pub fn render_with_state(
    ui: &mut Container<'_>,
    screen: &mut GoalsScreen,
    api: &ApiClient,
) {
    screen.load_initial(api);

    let goal_count = screen.goals.len();
    let panel_title = format!("Metas ({goal_count})");
    let error_msg = screen.error.clone();
    let loading = screen.loading;
    let selected = screen.selected_index;
    let modal_state = screen.modal_state;
    let form_name = screen.form_name.clone();
    let form_target_amount = screen.form_target_amount.clone();
    let form_kind_index = screen.form_kind_index;
    let form_status_index = screen.form_status_index;
    let form_target_date = screen.form_target_date.clone();
    let form_field = screen.form_field;
    let form_error = screen.form_error.clone();
    let delete_target_name = screen.delete_target_name.clone();
    let delete_target_progress = screen.delete_target_progress.clone();

    let goal_displays: Vec<_> = screen
        .goals
        .iter()
        .enumerate()
        .map(|(i, goal)| {
            let is_selected = selected == Some(i);
            let name = goal.name.clone();
            let kind = goal
                .kind
                .clone()
                .unwrap_or_else(|| "N\u{e3}o especificado".to_string());
            let status_text = status_badge(goal.status.as_deref());
            let target_display = goal
                .target_amount
                .as_ref().map_or_else(|| "N\u{e3}o definido".to_string(), |s| format!("R$ {}", format_brl(s)));
            let current_display = goal
                .current_amount
                .as_ref().map_or_else(|| "R$ 0,00".to_string(), |s| format!("R$ {}", format_brl(s)));
            let currency = goal
                .currency_code
                .clone()
                .unwrap_or_else(|| "BRL".to_string());
            let progress_pct = goal.progress_percent.unwrap_or(0.0);
            let progress_ratio = (progress_pct / 100.0).clamp(0.0, 1.0);
            let pct_text = format!("{progress_pct:.1}%");
            let target_date = goal
                .target_date
                .clone()
                .unwrap_or_else(|| "N\u{e3}o definido".to_string());
            let created_at = goal
                .created_at
                .clone()
                .unwrap_or_else(|| "Desconhecido".to_string());
            let item_title = format!("{name} - {status_text}");
            (
                is_selected,
                item_title,
                kind,
                target_display,
                current_display,
                currency,
                progress_ratio,
                pct_text,
                target_date,
                created_at,
            )
        })
        .collect();

    ui.column(Column::new().gap(1), move |col| {
        col.panel(Panel::new().title(&panel_title), move |p| {
            if loading {
                p.text("Carregando...");
                return;
            }
            if let Some(ref err) = error_msg {
                p.styled_text(err, TextStyle::new().fg(COLOR_NEGATIVE));
                return;
            }
            if goal_displays.is_empty() {
                p.text("Nenhuma meta encontrada.");
                p.text("Pressione 'n' para criar uma nova meta.");
                return;
            }

            p.text("n: Nova Meta | e: Editar | d: Excluir | \u{2191}/\u{2193}: Navegar");
            p.divider(DividerOptions::default());

            for (
                is_selected,
                item_title,
                kind,
                target_display,
                current_display,
                currency,
                progress_ratio,
                pct_text,
                target_date,
                created_at,
            ) in goal_displays.clone()
            {
                if is_selected {
                    p.styled_text(&format!("> {item_title}"), TextStyle::new().bold());
                } else {
                    p.text(&format!("  {item_title}"));
                }
                p.key_values(KeyValueOptions {
                    rows: vec![
                        KeyValueRow::new("Tipo", &kind),
                        KeyValueRow::new("Valor Alvo", &target_display),
                        KeyValueRow::new("Valor Atual", &current_display),
                        KeyValueRow::new("Moeda", &currency),
                        KeyValueRow::new("Data Alvo", &target_date),
                        KeyValueRow::new("Criado em", &created_at),
                    ],
                    ..Default::default()
                });
                p.divider(DividerOptions::default());
                let pr = progress_ratio;
                let pt = pct_text.clone();
                p.draw(move |s| {
                    hqtui::widgets::draw_meter(
                        s,
                        &hqtui::widgets::MeterOptions {
                            value: pr,
                            label: Some("Progresso".into()),
                            text: Some(pt.clone()),
                            ..Default::default()
                        },
                    );
                });
                p.divider(DividerOptions::default());
            }
        });

        col.button(
            ButtonOptions::new("Nova Meta").variant(ButtonVariant::Primary),
            "goals:new",
        );
    });

    // Render modals
    match modal_state {
        GoalModalState::Create => {
            let name_display = if form_field == GoalFormField::Name {
                format!("{form_name}_")
            } else {
                form_name.clone()
            };
            let amount_display = if form_field == GoalFormField::TargetAmount {
                format!("{form_target_amount}_")
            } else {
                form_target_amount.clone()
            };
            let kind_label = KIND_OPTIONS.get(form_kind_index).copied().unwrap_or("savings");
            let kind_display = if form_field == GoalFormField::Kind {
                format!("< {kind_label} >")
            } else {
                kind_label.to_string()
            };
            let date_display = if form_field == GoalFormField::TargetDate {
                format!("{form_target_date}_")
            } else {
                form_target_date.clone()
            };
            let err = form_error.clone();

            ui.modal(ModalOptions {
                title: Some("Nova Meta".to_string()),
                message: Some(format!(
                    "Nome: {name_display}\nValor alvo: {amount_display}\nTipo: {kind_display}\nData alvo (YYYY-MM-DD): {date_display}\n\nTab: pr\u{f3}ximo campo | Enter: salvar | Esc: cancelar{}",
                    err.map(|e| format!("\n\n{e}")).unwrap_or_default()
                )),
                buttons: vec![
                    ModalButton::new("Cancelar"),
                    ModalButton::new("Salvar"),
                ],
                ..Default::default()
            });
        }
        GoalModalState::Edit => {
            let name_display = if form_field == GoalFormField::Name {
                format!("{form_name}_")
            } else {
                form_name.clone()
            };
            let amount_display = if form_field == GoalFormField::TargetAmount {
                format!("{form_target_amount}_")
            } else {
                form_target_amount.clone()
            };
            let status_label = STATUS_OPTIONS.get(form_status_index).copied().unwrap_or("active");
            let status_display = if form_field == GoalFormField::Status {
                format!("< {status_label} >")
            } else {
                status_label.to_string()
            };
            let date_display = if form_field == GoalFormField::TargetDate {
                format!("{form_target_date}_")
            } else {
                form_target_date.clone()
            };
            let err = form_error.clone();

            ui.modal(ModalOptions {
                title: Some("Editar Meta".to_string()),
                message: Some(format!(
                    "Nome: {name_display}\nValor alvo: {amount_display}\nStatus: {status_display}\nData alvo (YYYY-MM-DD): {date_display}\n\nTab: pr\u{f3}ximo campo | Enter: salvar | Esc: cancelar{}",
                    err.map(|e| format!("\n\n{e}")).unwrap_or_default()
                )),
                buttons: vec![
                    ModalButton::new("Cancelar"),
                    ModalButton::new("Salvar"),
                ],
                ..Default::default()
            });
        }
        GoalModalState::DeleteConfirm => {
            let target = delete_target_name.unwrap_or_default();
            let progress = delete_target_progress.unwrap_or_default();
            ui.modal(ModalOptions {
                title: Some("Confirmar Exclus\u{e3}o".to_string()),
                message: Some(format!(
                    "Tem certeza que deseja excluir a meta \"{target}\"?\nProgresso atual: {progress}\n\nEsta a\u{e7}\u{e3}o n\u{e3}o pode ser desfeita."
                )),
                buttons: vec![
                    ModalButton::new("Cancelar"),
                    ModalButton::new("Excluir"),
                ],
                ..Default::default()
            });
        }
        GoalModalState::None => {}
    }
}

/// Legacy stateless render for backward compatibility.
pub fn render(ui: &mut Container<'_>, api: &ApiClient) {
    let mut screen = GoalsScreen::new();
    render_with_state(ui, &mut screen, api);
}
