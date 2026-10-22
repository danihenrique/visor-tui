use crate::interaction::{Interactions, Interactive};
use hqtui::input::InputEvent;
use hqtui::prelude::*;
use hqtui::widgets::{
    ButtonOptions, ButtonVariant, ModalButton, ModalOptions, TableColumn, TableOptions, TableRow,
};

use crate::api::ApiClient;
use crate::models::Goal;
use crate::presentation::{Metric, Tone, amount, format_currency, metric_strip};

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
    interactions: Interactions,
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
            interactions: Interactions::default(),
        }
    }

    pub fn load_initial(&mut self, api: &ApiClient) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        self.fetch_goals(api);
    }

    pub fn fetch_goals(&mut self, api: &ApiClient) {
        self.loading = true;
        self.error = None;
        match api.goals(None) {
            Ok(goals) => {
                self.goals = goals;
                self.selected_index = (!self.goals.is_empty()).then_some(
                    self.selected_index
                        .unwrap_or(0)
                        .min(self.goals.len().saturating_sub(1)),
                );
                self.loading = false;
            }
            Err(e) => {
                self.error = Some(format!("Falha ao carregar metas: {e}"));
                self.loading = false;
            }
        }
    }

    pub fn paste_text(&mut self, text: &str) {
        if !matches!(
            self.modal_state,
            GoalModalState::Create | GoalModalState::Edit
        ) {
            return;
        }
        let text: String = text
            .chars()
            .filter(|character| !character.is_control())
            .collect();
        match self.form_field {
            GoalFormField::Name => self.form_name.push_str(&text),
            GoalFormField::TargetAmount => self.form_target_amount.push_str(&text),
            GoalFormField::TargetDate => self.form_target_date.push_str(&text),
            _ => {}
        }
    }

    pub fn handle_input(&mut self, api: &ApiClient, events: &[InputEvent]) {
        if self.modal_state != GoalModalState::None {
            if let Some(index) = self.interactions.field(events) {
                self.form_field = [
                    GoalFormField::Name,
                    GoalFormField::TargetAmount,
                    GoalFormField::Kind,
                    GoalFormField::TargetDate,
                    GoalFormField::Status,
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
                if k.name == "n" {
                    self.open_create_modal();
                    return;
                }
                if k.name == "e" {
                    if let Some(idx) = self.selected_index
                        && idx < self.goals.len()
                    {
                        self.populate_edit_form(idx);
                    }
                    return;
                }
                if k.name == "d" || k.name == "delete" {
                    if let Some(idx) = self.selected_index
                        && idx < self.goals.len()
                    {
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
        if self.interactions.pressed(events, "goals:new") {
            self.open_create_modal();
        }
        if self.interactions.pressed(events, "goals:edit")
            && let Some(idx) = self.selected_index
            && idx < self.goals.len()
        {
            self.populate_edit_form(idx);
        }
        if self.interactions.pressed(events, "goals:delete")
            && let Some(idx) = self.selected_index
            && idx < self.goals.len()
        {
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
        self.form_kind_index = goal
            .kind
            .as_deref()
            .and_then(|k| KIND_OPTIONS.iter().position(|&o| o == k))
            .unwrap_or(0);
        self.form_status_index = goal
            .status
            .as_deref()
            .and_then(|s| STATUS_OPTIONS.iter().position(|&o| o == s))
            .unwrap_or(0);
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

    fn handle_modal_input(&mut self, api: &ApiClient, events: &[InputEvent]) {
        match self.modal_state {
            GoalModalState::Create | GoalModalState::Edit => {
                let is_create = self.modal_state == GoalModalState::Create;
                for event in events {
                    if let InputEvent::Key(k) = event {
                        if k.name == "escape" {
                            self.modal_state = GoalModalState::None;
                            return;
                        }
                        if k.name == "tab" && !k.shift {
                            self.form_field = self.form_field.next(is_create);
                            return;
                        }
                        if k.name == "tab" && k.shift {
                            self.form_field = self.form_field.prev(is_create);
                            return;
                        }
                        if k.name == "enter" || k.key == "ctrl+s" {
                            self.submit_form(api);
                            return;
                        }
                        if self.form_field == GoalFormField::Kind && is_create {
                            if k.name == "right" || k.name == "l" {
                                self.form_kind_index =
                                    (self.form_kind_index + 1) % KIND_OPTIONS.len();
                                return;
                            }
                            if k.name == "left" || k.name == "h" {
                                self.form_kind_index = if self.form_kind_index == 0 {
                                    KIND_OPTIONS.len() - 1
                                } else {
                                    self.form_kind_index - 1
                                };
                                return;
                            }
                        }
                        if self.form_field == GoalFormField::Status {
                            if k.name == "right" || k.name == "l" {
                                self.form_status_index =
                                    (self.form_status_index + 1) % STATUS_OPTIONS.len();
                                return;
                            }
                            if k.name == "left" || k.name == "h" {
                                self.form_status_index = if self.form_status_index == 0 {
                                    STATUS_OPTIONS.len() - 1
                                } else {
                                    self.form_status_index - 1
                                };
                                return;
                            }
                        }
                        if k.key == "backspace" {
                            match self.form_field {
                                GoalFormField::Name => {
                                    self.form_name.pop();
                                }
                                GoalFormField::TargetAmount => {
                                    self.form_target_amount.pop();
                                }
                                GoalFormField::TargetDate => {
                                    self.form_target_date.pop();
                                }
                                _ => {}
                            }
                            return;
                        }
                        if k.key == "ctrl+u" {
                            match self.form_field {
                                GoalFormField::Name => {
                                    self.form_name.clear();
                                }
                                GoalFormField::TargetAmount => {
                                    self.form_target_amount.clear();
                                }
                                GoalFormField::TargetDate => {
                                    self.form_target_date.clear();
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
                                GoalFormField::Name => {
                                    self.form_name.push_str(ch);
                                }
                                GoalFormField::TargetAmount => {
                                    self.form_target_amount.push_str(ch);
                                }
                                GoalFormField::TargetDate => {
                                    self.form_target_date.push_str(ch);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                if self.interactions.pressed(events, "goal_form_cancel") {
                    self.modal_state = GoalModalState::None;
                }
                if self.interactions.pressed(events, "goal_form_save") {
                    self.submit_form(api);
                }
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
                                    Ok(()) => {
                                        self.fetch_goals(api);
                                    }
                                    Err(e) => {
                                        self.form_error = Some(format!("Erro ao excluir: {e}"));
                                    }
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
                if self.interactions.pressed(events, "goal_delete_cancel") {
                    self.modal_state = GoalModalState::None;
                    self.delete_target_id = None;
                    self.delete_target_name = None;
                    self.delete_target_progress = None;
                }
                if self.interactions.pressed(events, "goal_delete_confirm") {
                    if let Some(ref id) = self.delete_target_id {
                        match api.delete_goal(id) {
                            Ok(()) => {
                                self.fetch_goals(api);
                            }
                            Err(e) => {
                                self.form_error = Some(format!("Erro ao excluir: {e}"));
                            }
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
                        api.update_goal(
                            &goal_id,
                            Some(name),
                            Some(target_amount),
                            status,
                            target_date,
                        )
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
            Ok(_) => {
                self.modal_state = GoalModalState::None;
                self.fetch_goals(api);
            }
            Err(e) => {
                self.form_error = Some(format!("Erro: {e}"));
            }
        }
    }
}

impl Default for GoalsScreen {
    fn default() -> Self {
        Self::new()
    }
}

fn status_badge(status: Option<&str>) -> &str {
    match status {
        Some("active") => "Ativa",
        Some("archived") => "Arquivada",
        Some("completed") => "Concluída",
        Some(status) => status,
        None => "Sem status",
    }
}

fn goal_progress(goal: &Goal) -> Option<f64> {
    goal.progress_percent
        .filter(|value| value.is_finite())
        .or_else(|| {
            let target = amount(goal.target_amount.as_deref())?;
            (target > 0.0)
                .then(|| {
                    amount(goal.current_amount.as_deref()).map(|current| current / target * 100.0)
                })
                .flatten()
        })
}

fn goal_kind(kind: Option<&str>) -> &str {
    match kind {
        Some("savings") => "Reserva",
        Some("debt") => "Dívida",
        Some("investment") => "Investimento",
        Some("custom") => "Personalizada",
        Some(kind) => kind,
        None => "Meta",
    }
}

pub fn render_with_state(ui: &mut Container<'_>, screen: &mut GoalsScreen, api: &ApiClient) {
    screen.interactions.clear();
    let hits = screen.interactions.clone();
    screen.load_initial(api);

    let goal_count = screen.goals.len();
    let panel_title = format!(
        "Metas ({}/{goal_count})",
        screen.selected_index.unwrap_or(0) + usize::from(goal_count > 0)
    );
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

    let theme = ui.theme().clone();
    let selected_goal = selected.and_then(|index| screen.goals.get(index)).cloned();
    let active_count = screen
        .goals
        .iter()
        .filter(|goal| goal.status.as_deref() == Some("active"))
        .count();
    let metrics = vec![
        Metric::new(
            "Metas ativas",
            format!("{active_count} / {goal_count}"),
            Tone::Accent,
        ),
        Metric::new(
            "Atual · selecionada",
            selected_goal.as_ref().map_or_else(
                || "—".into(),
                |goal| {
                    format_currency(
                        goal.current_amount.as_deref(),
                        goal.currency_code.as_deref(),
                    )
                },
            ),
            Tone::Positive,
        ),
        Metric::new(
            "Alvo · selecionada",
            selected_goal.as_ref().map_or_else(
                || "—".into(),
                |goal| {
                    format_currency(goal.target_amount.as_deref(), goal.currency_code.as_deref())
                },
            ),
            Tone::Neutral,
        ),
    ];
    let rows = screen
        .goals
        .iter()
        .map(|goal| {
            let mut row = TableRow::new([
                goal.name.clone(),
                format_currency(
                    goal.current_amount.as_deref(),
                    goal.currency_code.as_deref(),
                ),
                format_currency(goal.target_amount.as_deref(), goal.currency_code.as_deref()),
                goal_progress(goal)
                    .map_or_else(|| "—".into(), |progress| format!("{progress:.0}%")),
            ]);
            row.cell_colors = vec![None, Some(theme.success), None, Some(theme.accent)];
            row
        })
        .collect::<Vec<_>>();
    let show_detail = ui.height() >= 16;
    let compact_detail = selected_goal.as_ref().map(|goal| {
        [
            format!(
                "{} · {} · Prazo {}",
                status_badge(goal.status.as_deref()),
                goal_kind(goal.kind.as_deref()),
                goal.target_date.as_deref().unwrap_or("não definido")
            ),
            format!("Criada {}", goal.created_at.as_deref().unwrap_or("—")),
        ]
    });
    ui.column(Column::new().gap(usize::from(show_detail)), {
        let hits = hits.clone();
        move |col| {
            if show_detail {
                metric_strip(col, metrics);
            }
            col.row(Row::new().size(Size::Cells(1)).gap(1), {
                let hits = hits.clone();
                move |row| {
                    row.mouse_button(
                        ButtonOptions::new("Nova meta (n)").variant(ButtonVariant::Primary),
                        "goals:new",
                        &hits,
                    );
                    row.mouse_button(
                        ButtonOptions::new("Editar (e)").variant(ButtonVariant::Ghost),
                        "goals:edit",
                        &hits,
                    );
                    row.mouse_button(
                        ButtonOptions::new("Excluir (d)").variant(ButtonVariant::Ghost),
                        "goals:delete",
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
                            panel.label("Carregando suas metas…");
                        } else if let Some(error) = error_msg {
                            panel.styled_text(&error, TextStyle::new().fg(theme.danger));
                        } else if rows.is_empty() {
                            panel.text("Dê um destino ao seu dinheiro.");
                            panel.label("Pressione n para criar sua primeira meta.");
                        } else {
                            panel.mouse_table(
                                TableOptions {
                                    columns: vec![
                                        TableColumn::new("Meta").width(Size::Fill),
                                        TableColumn::new("Atual")
                                            .width(Size::Cells(16))
                                            .align(Align::Right),
                                        TableColumn::new("Alvo")
                                            .width(Size::Cells(16))
                                            .align(Align::Right),
                                        TableColumn::new("Progresso")
                                            .width(Size::Cells(9))
                                            .align(Align::Right),
                                    ],
                                    rows,
                                    selected,
                                    follow_selection: true,
                                    scrollbar: true,
                                    header_color: Some(theme.muted),
                                    ..Default::default()
                                },
                                &hits,
                            );
                        }
                    }
                },
            );
            if show_detail && let Some(goal) = selected_goal {
                col.panel(
                    Panel::new()
                        .title(goal.name.clone())
                        .border(BorderStyle::Rounded)
                        .size(Size::Cells(5)),
                    move |panel| {
                        if let Some(progress) = goal_progress(&goal) {
                            panel.meter(MeterOptions {
                                value: progress / 100.0,
                                text: Some(format!("{progress:.0}%")),
                                color: Some(theme.success),
                                ..Default::default()
                            });
                        } else {
                            panel.label("Progresso indisponível");
                        }
                        panel.label(&format!(
                            "{} · {} · Prazo {}",
                            status_badge(goal.status.as_deref()),
                            goal_kind(goal.kind.as_deref()),
                            goal.target_date.as_deref().unwrap_or("não definido")
                        ));
                        panel.label(&format!(
                            "Criada {}",
                            goal.created_at.as_deref().unwrap_or("—")
                        ));
                    },
                );
            }
            if !show_detail && let Some(lines) = compact_detail {
                col.sized(Size::Cells(2), move |details| {
                    details.column(Column::new(), move |body| {
                        for line in lines {
                            body.label(&line);
                        }
                    });
                });
            }
        }
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
            let kind_label = KIND_OPTIONS
                .get(form_kind_index)
                .copied()
                .unwrap_or("savings");
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

            let options = ModalOptions {
                title: Some("Nova Meta".to_string()),
                message: Some(format!(
                    "Nome: {name_display}\nValor alvo: {amount_display}\nTipo: {kind_display}\nData alvo (YYYY-MM-DD): {date_display}\n\nTab: pr\u{f3}ximo campo | Enter: salvar | Esc: cancelar{}",
                    err.map(|e| format!("\n\n{e}")).unwrap_or_default()
                )),
                buttons: vec![ModalButton::new("Cancelar"), ModalButton::new("Salvar")],
                width: Some(68),
                align: Some(Align::Left),
                ..Default::default()
            };
            ui.mouse_modal(
                options,
                &[
                    "Nome:",
                    "Valor alvo:",
                    "Tipo:",
                    "Data alvo (YYYY-MM-DD):",
                    "Status:",
                ],
                &screen.interactions,
            );
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
            let status_label = STATUS_OPTIONS
                .get(form_status_index)
                .copied()
                .unwrap_or("active");
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

            let options = ModalOptions {
                title: Some("Editar Meta".to_string()),
                message: Some(format!(
                    "Nome: {name_display}\nValor alvo: {amount_display}\nStatus: {status_display}\nData alvo (YYYY-MM-DD): {date_display}\n\nTab: pr\u{f3}ximo campo | Enter: salvar | Esc: cancelar{}",
                    err.map(|e| format!("\n\n{e}")).unwrap_or_default()
                )),
                buttons: vec![ModalButton::new("Cancelar"), ModalButton::new("Salvar")],
                width: Some(68),
                align: Some(Align::Left),
                ..Default::default()
            };
            ui.mouse_modal(
                options,
                &[
                    "Nome:",
                    "Valor alvo:",
                    "Tipo:",
                    "Data alvo (YYYY-MM-DD):",
                    "Status:",
                ],
                &screen.interactions,
            );
        }
        GoalModalState::DeleteConfirm => {
            let target = delete_target_name.unwrap_or_default();
            let progress = delete_target_progress.unwrap_or_default();
            let options = ModalOptions {
                title: Some("Confirmar Exclus\u{e3}o".to_string()),
                message: Some(format!(
                    "Tem certeza que deseja excluir a meta \"{target}\"?\nProgresso atual: {progress}\n\nEsta a\u{e7}\u{e3}o n\u{e3}o pode ser desfeita."
                )),
                buttons: vec![ModalButton::new("Cancelar"), ModalButton::new("Excluir")],
                width: Some(68),
                align: Some(Align::Left),
                ..Default::default()
            };
            ui.mouse_modal(
                options,
                &[
                    "Nome:",
                    "Valor alvo:",
                    "Tipo:",
                    "Data alvo (YYYY-MM-DD):",
                    "Status:",
                ],
                &screen.interactions,
            );
        }
        GoalModalState::None => {}
    }
}

/// Legacy stateless render for backward compatibility.
pub fn render(ui: &mut Container<'_>, api: &ApiClient) {
    let mut screen = GoalsScreen::new();
    render_with_state(ui, &mut screen, api);
}

#[cfg(test)]
mod visual_tests {
    use super::*;

    #[test]
    fn compact_goals_keep_dates_and_selected_amounts_visible() {
        let api = ApiClient::for_test(
            "http://127.0.0.1:1",
            crate::auth::Credential::PersonalToken {
                token: "test-only".into(),
            },
            "space",
        )
        .unwrap();
        let mut screen = GoalsScreen::new();
        screen.loaded = true;
        screen.selected_index = Some(0);
        screen.goals = serde_json::from_value(serde_json::json!([{"id":"g", "name":"Reserva", "kind":"savings", "status":"active", "current_amount":"123.45", "target_amount":"246.90", "target_date":"2027-06-01", "created_at":"2026-10-22"}])).unwrap();
        for (width, height) in [(56, 10), (60, 18), (80, 24)] {
            let text = hqtui::testing::render_to_text(width, height, "dark", |ui| {
                render_with_state(ui, &mut screen, &api);
            });
            for value in [
                "Reserva",
                "123,45 BRL",
                "246,90 BRL",
                "2027-06-01",
                "2026-10-22",
            ] {
                assert!(
                    text.contains(value),
                    "Missing {value} at {width}x{height}\n{text}"
                );
            }
        }
    }

    #[test]
    fn missing_goal_progress_is_unknown_or_computed_from_known_amounts() {
        let mut goal: Goal =
            serde_json::from_value(serde_json::json!({"id":"g", "name":"Reserva"})).unwrap();
        assert_eq!(goal_progress(&goal), None);
        goal.target_amount = Some("200.00".into());
        goal.current_amount = Some("50.00".into());
        assert_eq!(goal_progress(&goal), Some(25.0));
        goal.target_amount = Some("0".into());
        assert_eq!(goal_progress(&goal), None);
        goal.progress_percent = Some(80.0);
        assert_eq!(goal_progress(&goal), Some(80.0));
    }
}
