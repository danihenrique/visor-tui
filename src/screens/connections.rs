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

    pub fn handle_input(&mut self, app: &App, api: &ApiClient, events: &[InputEvent]) {
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

        if app.pressed("connections:refresh") {
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
    screen.load(api);

    let table_rows: Vec<TableRow> = screen
        .connections
        .iter()
        .map(|c| {
            let provider = c
                .provider_name
                .as_deref()
                .unwrap_or("(desconhecido)")
                .to_string();
            let (status_text, _status_color) =
                status_display(c.status.as_deref(), c.paused, c.requires_reauth);
            let last_sync = format_date(c.last_successful_sync_at.as_deref());
            let next_sync = format_date(c.next_auto_sync_at.as_deref());
            let accounts_count = c.account_ids.len().to_string();
            TableRow::new([provider, status_text, last_sync, next_sync, accounts_count])
        })
        .collect();

    let columns = vec![
        TableColumn::new("Provedor").width(Size::Fill),
        TableColumn::new("Status")
            .width(Size::Cells(16))
            .align(Align::Left),
        TableColumn::new("Última Sync")
            .width(Size::Cells(18))
            .align(Align::Center),
        TableColumn::new("Próxima Sync")
            .width(Size::Cells(18))
            .align(Align::Center),
        TableColumn::new("Contas")
            .width(Size::Cells(8))
            .align(Align::Center),
    ];

    let has_rows = !table_rows.is_empty();
    let conn_count = screen.connections.len();
    let active_count = screen
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
    let header_label = format!("Conexões ({conn_count} total, {active_count} ativas)");
    let error_msg = screen.error.clone();
    let loading = screen.loading;
    let selected = screen.selected_index;

    ui.column(Column::new().gap(1), move |col| {
        // Error display
        if let Some(err) = error_msg {
            col.panel(Panel::new().title("Erro"), move |p| {
                p.styled_text(&err, TextStyle::new().fg(COLOR_NEGATIVE));
            });
        }

        // Connections table
        col.panel(Panel::new().title(header_label), move |p| {
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
            } else {
                p.styled_text(
                    "Nenhuma conexão encontrada.",
                    TextStyle::new().fg(COLOR_NEUTRAL),
                );
            }
        });

        // Key hints
        col.row(Row::new(), move |r| {
            r.styled_text(
                "↑↓/jk: Navegar | r: Atualizar",
                TextStyle::new().fg(COLOR_NEUTRAL),
            );
        });
    });
}
