use crate::interaction::{Interactions, Interactive};
use crate::presentation::{Metric, Tone, metric_strip};
use hqtui::color::Color;
use hqtui::input::InputEvent;
use hqtui::prelude::*;
use hqtui::unicode::Align;
use hqtui::widgets::{TableColumn, TableOptions, TableRow, TextStyle};

use crate::api::ApiClient;
use crate::models::Connection;

const COLOR_NEGATIVE: Color = Color::rgb(239, 68, 68);
const COLOR_NEUTRAL: Color = Color::rgb(148, 163, 184);
const COLOR_SUCCESS: Color = Color::rgb(34, 197, 94);
const COLOR_WARNING: Color = Color::rgb(234, 179, 8);

pub struct ConnectionsScreen {
    interactions: Interactions,
    pub connections: Vec<Connection>,
    pub selected_index: Option<usize>,
    pub scroll_offset: usize,
    pub loading: bool,
    pub error: Option<String>,
    pub loaded: bool,
}

impl ConnectionsScreen {
    #[must_use]
    pub fn new() -> Self {
        Self {
            interactions: Interactions::default(),
            connections: Vec::new(),
            selected_index: None,
            scroll_offset: 0,
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
        match api.connections() {
            Ok(connections) => {
                self.connections = connections;
                self.selected_index = (!self.connections.is_empty()).then_some(
                    self.selected_index
                        .unwrap_or(0)
                        .min(self.connections.len().saturating_sub(1)),
                );
                self.selected_index = if self.connections.is_empty() {
                    None
                } else {
                    Some(0)
                };
                self.scroll_offset = 0;
                self.loading = false;
            }
            Err(e) => {
                self.error = Some(format!("Falha ao carregar conexões: {e}"));
                self.loading = false;
            }
        }
    }

    pub fn handle_input(&mut self, api: &ApiClient, events: &[InputEvent]) {
        let mapped_events = self
            .interactions
            .list_events(events, &mut self.selected_index);
        let events = mapped_events.as_slice();
        for event in events {
            if let InputEvent::Key(k) = event {
                if k.name == "down" || k.name == "j" {
                    if !self.connections.is_empty() {
                        let next = match self.selected_index {
                            Some(i) => (i + 1).min(self.connections.len() - 1),
                            None => 0,
                        };
                        self.selected_index = Some(next);
                    }
                    return;
                }
                if k.name == "up" || k.name == "k" {
                    if !self.connections.is_empty() {
                        let prev = match self.selected_index {
                            Some(i) => i.saturating_sub(1),
                            None => 0,
                        };
                        self.selected_index = Some(prev);
                    }
                    return;
                }
                if k.name == "r" {
                    self.fetch(api);
                    return;
                }
            }
        }

        if self.interactions.pressed(events, "connections:refresh") {
            api.invalidate_cache();
            self.fetch(api);
        }
    }
}

impl Default for ConnectionsScreen {
    fn default() -> Self {
        Self::new()
    }
}

fn format_date(date_str: Option<&str>) -> String {
    match date_str {
        Some(s) => {
            if s.len() >= 16 {
                // Show YYYY-MM-DD HH:MM
                s[..16].replace('T', " ")
            } else if s.len() >= 10 {
                s[..10].to_string()
            } else {
                s.to_string()
            }
        }
        None => "—".to_string(),
    }
}

fn status_display(status: Option<&str>, paused: bool, requires_reauth: bool) -> (String, Color) {
    if requires_reauth {
        return ("● Reautenticar".to_string(), COLOR_NEGATIVE);
    }
    if paused {
        return ("● Pausado".to_string(), COLOR_WARNING);
    }
    match status {
        Some("connected" | "active" | "syncing") => ("● Ativo".to_string(), COLOR_SUCCESS),
        Some("error" | "failed") => ("● Erro".to_string(), COLOR_NEGATIVE),
        Some(other) => (format!("● {other}"), COLOR_NEUTRAL),
        None => ("● —".to_string(), COLOR_NEUTRAL),
    }
}

pub fn render_with_state(ui: &mut Container<'_>, screen: &mut ConnectionsScreen, api: &ApiClient) {
    screen.interactions.clear();
    let hits = screen.interactions.clone();
    screen.load(api);
    let wide = ui.width() >= 95;
    let show_metrics = ui.height() >= 15;
    let total = screen.connections.len();
    let active = screen
        .connections
        .iter()
        .filter(|c| {
            !c.paused
                && !c.requires_reauth
                && matches!(
                    c.status.as_deref(),
                    Some("connected" | "active" | "syncing")
                )
        })
        .count();
    let attention = screen
        .connections
        .iter()
        .filter(|c| c.requires_reauth || matches!(c.status.as_deref(), Some("error" | "failed")))
        .count();
    let accounts: usize = screen.connections.iter().map(|c| c.account_ids.len()).sum();
    let rows = screen
        .connections
        .iter()
        .map(|connection| {
            let (status, color) = status_display(
                connection.status.as_deref(),
                connection.paused,
                connection.requires_reauth,
            );
            let mut cells = vec![
                connection
                    .provider_name
                    .clone()
                    .unwrap_or_else(|| "Instituição não informada".into()),
                status,
                format_date(connection.last_successful_sync_at.as_deref()),
            ];
            if wide {
                cells.push(format_date(connection.next_auto_sync_at.as_deref()));
            }
            cells.push(connection.account_ids.len().to_string());
            let mut colors = vec![None; cells.len()];
            colors[1] = Some(color);
            TableRow {
                cells,
                cell_colors: colors,
                ..Default::default()
            }
        })
        .collect::<Vec<_>>();
    let mut columns = vec![
        TableColumn::new("Instituição").width(Size::Fill),
        TableColumn::new("Situação").width(Size::Cells(15)),
        TableColumn::new("Atualizada em").width(Size::Cells(17)),
    ];
    if wide {
        columns.push(TableColumn::new("Próxima").width(Size::Cells(17)));
    }
    columns.push(
        TableColumn::new("Contas")
            .width(Size::Cells(6))
            .align(Align::Right),
    );
    let selected = screen.selected_index;
    let detail = selected.and_then(|i| screen.connections.get(i)).map(|c| {
        let action = if c.requires_reauth {
            "Renove a autorização no Visor para voltar a sincronizar."
        } else if c.paused {
            "Conexão pausada. Os dados exibidos são da última atualização."
        } else {
            "Atualização automática da instituição conectada."
        };
        (
            c.provider_name.clone().unwrap_or_default(),
            action.to_owned(),
            format!(
                "Próxima sincronização {}",
                format_date(c.next_auto_sync_at.as_deref())
            ),
        )
    });
    let error = screen.error.clone();
    let loading = screen.loading;
    let muted = ui.theme().muted;
    ui.column(Column::new().gap(1), {
        let hits = hits.clone();
        move |col| {
            if show_metrics {
                metric_strip(
                    col,
                    vec![
                        Metric::new("Ativas", format!("{active} de {total}"), Tone::Positive),
                        Metric::new(
                            "Precisam de atenção",
                            attention.to_string(),
                            if attention > 0 {
                                Tone::Warning
                            } else {
                                Tone::Neutral
                            },
                        ),
                        Metric::new("Contas vinculadas", accounts.to_string(), Tone::Accent),
                    ],
                );
            }
            col.panel(Panel::new().title("Instituições conectadas"), {
                let hits = hits.clone();
                move |panel| {
                    if loading {
                        panel.label("Atualizando conexões…");
                    } else if let Some(error) = error {
                        panel.styled_text(&error, TextStyle::new().fg(COLOR_NEGATIVE));
                    } else if rows.is_empty() {
                        panel.text("Nenhuma instituição conectada.");
                        panel.label("Conecte uma conta pelo Visor e pressione r para atualizar.");
                    } else {
                        panel.mouse_table(
                            TableOptions {
                                columns,
                                rows,
                                selected,
                                follow_selection: true,
                                scrollbar: true,
                                zebra: true,
                                ..Default::default()
                            },
                            &hits,
                        );
                    }
                }
            });
            if let Some((provider, action, next)) = detail {
                col.panel(
                    Panel::new().title(provider).size(Size::Cells(4)),
                    move |panel| {
                        panel.styled_text(&action, TextStyle::new().fg(muted));
                        panel.styled_text(&next, TextStyle::new().fg(muted));
                    },
                );
            }
            col.row(Row::new().size(Size::Cells(1)), {
                let hits = hits.clone();
                move |row| {
                    row.label("j/k Selecionar");
                    row.mouse_button(
                        ButtonOptions::new("r Atualizar").variant(ButtonVariant::Ghost),
                        "connections:refresh",
                        &hits,
                    );
                }
            });
        }
    });
}
