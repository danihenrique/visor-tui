use hqtui::color::Color;
use hqtui::input::InputEvent;
use hqtui::prelude::*;
use hqtui::unicode::Align;
use hqtui::widgets::{TableColumn, TableOptions, TableRow, TextStyle};

use crate::api::ApiClient;
use crate::models::Notification;

const COLOR_NEGATIVE: Color = Color::rgb(239, 68, 68);
const COLOR_NEUTRAL: Color = Color::rgb(148, 163, 184);
const COLOR_ACCENT: Color = Color::rgb(59, 130, 246);

pub struct NotificationsScreen {
    pub notifications: Vec<Notification>,
    pub selected_index: Option<usize>,
    pub scroll_offset: usize,
    pub unread_filter: bool,
    pub loading: bool,
    pub error: Option<String>,
    pub loaded: bool,
}

impl NotificationsScreen {
    #[must_use]
    pub fn new() -> Self {
        Self {
            notifications: Vec::new(),
            selected_index: None,
            scroll_offset: 0,
            unread_filter: false,
            loading: false,
            error: None,
            loaded: false,
        }
    }

    pub fn load(&mut self, api: &ApiClient) {
        if self.loaded && !self.loading {
            return;
        }
        self.loaded = true;
        self.fetch(api);
    }

    pub fn fetch(&mut self, api: &ApiClient) {
        self.loading = true;
        self.error = None;
        let unread = if self.unread_filter { Some(true) } else { None };
        match api.notifications(unread, Some(50)) {
            Ok(notifications) => {
                self.notifications = notifications;
                self.selected_index = if self.notifications.is_empty() {
                    None
                } else {
                    Some(0)
                };
                self.scroll_offset = 0;
                self.loading = false;
            }
            Err(e) => {
                self.error = Some(format!("Falha ao carregar notificações: {e}"));
                self.loading = false;
            }
        }
    }

    pub fn handle_input(&mut self, app: &App, api: &ApiClient, events: &[InputEvent]) {
        for event in events {
            if let InputEvent::Key(k) = event {
                if k.name == "down" || k.name == "j" {
                    if !self.notifications.is_empty() {
                        let next = match self.selected_index {
                            Some(i) => (i + 1).min(self.notifications.len() - 1),
                            None => 0,
                        };
                        self.selected_index = Some(next);
                    }
                    return;
                }
                if k.name == "up" || k.name == "k" {
                    if !self.notifications.is_empty() {
                        let prev = match self.selected_index {
                            Some(i) => i.saturating_sub(1),
                            None => 0,
                        };
                        self.selected_index = Some(prev);
                    }
                    return;
                }
                if k.name == "u" {
                    self.unread_filter = !self.unread_filter;
                    self.fetch(api);
                    return;
                }
                if k.name == "m" || k.name == "enter" {
                    if let Some(idx) = self.selected_index
                        && idx < self.notifications.len()
                    {
                        let id = self.notifications[idx].id.clone();
                        match api.mark_notification_read(&id) {
                            Ok(()) => {
                                self.notifications[idx].read = true;
                            }
                            Err(e) => {
                                self.error = Some(format!("Erro ao marcar como lida: {e}"));
                            }
                        }
                    }
                    return;
                }
                if k.name == "r" {
                    self.fetch(api);
                    return;
                }
            }
        }

        // Button press for refresh
        if app.pressed("notifications:refresh") {
            self.fetch(api);
        }
    }
}

impl Default for NotificationsScreen {
    fn default() -> Self {
        Self::new()
    }
}

fn format_date(date_str: Option<&str>) -> String {
    match date_str {
        Some(s) => {
            // Take just the date portion (YYYY-MM-DD) from ISO timestamps
            if s.len() >= 10 {
                s[..10].to_string()
            } else {
                s.to_string()
            }
        }
        None => "—".to_string(),
    }
}

pub fn render_with_state(
    ui: &mut Container<'_>,
    screen: &mut NotificationsScreen,
    api: &ApiClient,
) {
    screen.load(api);

    let table_rows: Vec<TableRow> = screen
        .notifications
        .iter()
        .map(|n| {
            let title = n.title.as_deref().unwrap_or("(sem título)").to_string();
            let topic = n.topic.as_deref().unwrap_or("—").to_string();
            let date = format_date(n.created_at.as_deref());
            let read_status = if n.read { "✓" } else { "●" }.to_string();
            TableRow::new([title, topic, date, read_status])
        })
        .collect();

    let columns = vec![
        TableColumn::new("Título").width(Size::Fill),
        TableColumn::new("Tópico")
            .width(Size::Cells(20))
            .align(Align::Left),
        TableColumn::new("Data")
            .width(Size::Cells(12))
            .align(Align::Center),
        TableColumn::new("Lida")
            .width(Size::Cells(6))
            .align(Align::Center),
    ];

    let has_rows = !table_rows.is_empty();
    let notif_count = screen.notifications.len();
    let unread_count = screen.notifications.iter().filter(|n| !n.read).count();
    let filter_label = if screen.unread_filter {
        format!("Notificações ({unread_count} não lidas) [Filtro: somente não lidas]")
    } else {
        format!("Notificações ({notif_count} total, {unread_count} não lidas)")
    };
    let error_msg = screen.error.clone();
    let loading = screen.loading;
    let selected = screen.selected_index;
    let unread_filter = screen.unread_filter;

    ui.column(Column::new().gap(1), move |col| {
        // Filter indicator and action buttons
        col.row(Row::new().size(Size::Cells(1)).gap(1), move |r| {
            if unread_filter {
                r.styled_text(
                    " Filtro ativo: somente não lidas ",
                    TextStyle::new().fg(COLOR_ACCENT).bold(),
                );
            }
        });

        // Error display
        if let Some(err) = error_msg {
            col.panel(Panel::new().title("Erro"), move |p| {
                p.styled_text(&err, TextStyle::new().fg(COLOR_NEGATIVE));
            });
        }

        // Notifications table
        col.panel(Panel::new().title(filter_label), move |p| {
            if loading {
                p.text("Carregando...");
            } else if has_rows {
                p.table(
                    TableOptions {
                        follow_selection: true,
                        columns: columns.clone(),
                        rows: table_rows.clone(),
                        header: true,
                        zebra: true,
                        selected,
                        ..Default::default()
                    },
                    "",
                );
            } else if unread_filter {
                p.styled_text(
                    "Nenhuma notificação não lida.",
                    TextStyle::new().fg(COLOR_NEUTRAL),
                );
            } else {
                p.styled_text(
                    "Nenhuma notificação encontrada.",
                    TextStyle::new().fg(COLOR_NEUTRAL),
                );
            }
        });

        // Key hints
        col.row(Row::new(), move |r| {
            r.styled_text(
                "↑↓/jk: Navegar | m/Enter: Marcar lida | u: Filtrar não lidas | r: Atualizar",
                TextStyle::new().fg(COLOR_NEUTRAL),
            );
        });
    });
}
