use hqtui::color::Color;
use hqtui::input::InputEvent;
use hqtui::prelude::*;
use hqtui::widgets::{TableColumn, TableOptions, TableRow, TextStyle};

use crate::api::ApiClient;
use crate::models::Notification;

const COLOR_NEGATIVE: Color = Color::rgb(239, 68, 68);

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
            api.invalidate_cache();
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

fn topic_label(topic: Option<&str>) -> &str {
    match topic {
        Some("bill") => "Fatura",
        Some("connection") => "Conexão",
        Some("goal") => "Meta",
        Some("transaction") => "Transação",
        Some(topic) => topic,
        None => "Geral",
    }
}

pub fn render_with_state(
    ui: &mut Container<'_>,
    screen: &mut NotificationsScreen,
    api: &ApiClient,
) {
    use crate::presentation::{Metric, Tone, metric_strip};
    screen.load(api);
    let width = ui.width();
    let show_metrics = ui.height() >= 15;
    let wide = width >= 90;
    let muted = ui.theme().muted;
    let accent = ui.theme().accent;
    let rows = screen
        .notifications
        .iter()
        .map(|notification| {
            let mut cells = vec![
                if notification.read {
                    "✓".into()
                } else {
                    "●".into()
                },
                notification
                    .title
                    .clone()
                    .unwrap_or_else(|| "Sem título".into()),
            ];
            if wide {
                cells.push(topic_label(notification.topic.as_deref()).into());
            }
            cells.push(format_date(notification.created_at.as_deref()));
            let mut colors = vec![None; cells.len()];
            colors[0] = Some(if notification.read { muted } else { accent });
            if notification.read {
                colors[1] = Some(muted);
            }
            TableRow {
                cells,
                cell_colors: colors,
                ..Default::default()
            }
        })
        .collect::<Vec<_>>();
    let mut columns = vec![
        TableColumn::new("").width(Size::Cells(2)),
        TableColumn::new("Mensagem").width(Size::Fill),
    ];
    if wide {
        columns.push(TableColumn::new("Assunto").width(Size::Cells(14)));
    }
    columns.push(TableColumn::new("Recebida").width(Size::Cells(10)));
    let count = screen.notifications.len();
    let unread = screen.notifications.iter().filter(|n| !n.read).count();
    let detail = screen
        .selected_index
        .and_then(|i| screen.notifications.get(i))
        .map(|n| {
            (
                n.title.clone().unwrap_or_else(|| "Mensagem".into()),
                n.body
                    .clone()
                    .unwrap_or_else(|| "Sem detalhes adicionais.".into()),
                topic_label(n.topic.as_deref()).to_owned(),
            )
        });
    let selected = screen.selected_index;
    let error = screen.error.clone();
    let loading = screen.loading;
    let unread_filter = screen.unread_filter;
    ui.column(Column::new().gap(1), move |col| {
        if show_metrics {
            metric_strip(
                col,
                vec![
                    Metric::new("Não lidas", unread.to_string(), Tone::Accent),
                    Metric::new("Nesta lista", count.to_string(), Tone::Neutral),
                    Metric::new(
                        "Filtro",
                        if unread_filter { "Não lidas" } else { "Todas" },
                        Tone::Neutral,
                    ),
                ],
            );
        }
        col.panel(
            Panel::new().title(if unread_filter {
                "Caixa de entrada · não lidas"
            } else {
                "Caixa de entrada"
            }),
            move |panel| {
                if loading {
                    panel.label("Carregando notificações…");
                } else if let Some(error) = error {
                    panel.styled_text(&error, TextStyle::new().fg(COLOR_NEGATIVE));
                } else if rows.is_empty() {
                    panel.text(if unread_filter {
                        "Tudo em dia. Nenhuma mensagem não lida."
                    } else {
                        "Sua caixa de entrada está vazia."
                    });
                    panel.label("u Alternar filtro  ·  r Atualizar");
                } else {
                    panel.table(
                        TableOptions {
                            columns,
                            rows,
                            selected,
                            follow_selection: true,
                            scrollbar: true,
                            zebra: true,
                            ..Default::default()
                        },
                        "notifications:table",
                    );
                }
            },
        );
        if let Some((title, body, topic)) = detail {
            col.panel(
                Panel::new().title(title).footer(topic).size(Size::Cells(5)),
                move |panel| {
                    for line in hqtui::unicode::wrap(&body, panel.width())
                        .into_iter()
                        .take(3)
                    {
                        panel.styled_text(&line, TextStyle::new().fg(muted));
                    }
                },
            );
        }
        col.row(Row::new().size(Size::Cells(1)), move |row| {
            row.sized(Size::Fill, |hint| {
                hint.label("j/k Selecionar  m/Enter Lida  u Filtrar");
            });
            row.sized(Size::Cells(13), |button| {
                button.button(
                    ButtonOptions::new("r Atualizar").variant(ButtonVariant::Ghost),
                    "notifications:refresh",
                );
            });
        });
    });
}
