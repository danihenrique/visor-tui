use hqtui::input::InputEvent;
use hqtui::prelude::*;
use hqtui::unicode::Align;

use crate::api::ApiClient;
use crate::models::{Bill, Card};
use crate::period::{DateRange, Period};
use crate::presentation::{Metric, Tone, amount, format_currency, metric_strip, money_total};

#[derive(Default)]
pub struct BillsScreen {
    cards: Vec<Card>,
    bills: Vec<Bill>,
    selected_card: usize,
    selected_bill: Option<usize>,
    error: Option<String>,
    pub reload_requested: bool,
}

impl BillsScreen {
    pub fn load(&mut self, api: &ApiClient, range: DateRange) {
        self.bills.clear();
        self.error = None;
        self.reload_requested = false;
        let selected_id = self
            .cards
            .get(self.selected_card)
            .map(|card| card.id.clone());
        match api.cards() {
            Ok(cards) => {
                self.cards = cards;
                self.selected_card = selected_id
                    .and_then(|id| self.cards.iter().position(|card| card.id == id))
                    .unwrap_or(0);
                self.load_bills(api, range);
            }
            Err(error) => self.error = Some(format!("Falha ao carregar cartões: {error}")),
        }
    }

    fn load_bills(&mut self, api: &ApiClient, range: DateRange) {
        self.error = None;
        self.bills.clear();
        self.selected_bill = None;
        if let Some(card) = self.cards.get(self.selected_card) {
            match api.bills(
                &card.id,
                Some(&range.start().to_string()),
                Some(&range.end().to_string()),
            ) {
                Ok(bills) => {
                    self.bills = bills;
                    self.selected_bill = (!self.bills.is_empty()).then_some(0);
                }
                Err(error) => self.error = Some(format!("Falha ao carregar faturas: {error}")),
            }
        }
    }

    pub fn handle_input(&mut self, app: &App, events: &[InputEvent]) {
        let mut change = 0;
        for event in events {
            if let InputEvent::Key(key) = event {
                match key.name.as_str() {
                    "right" | "l" => change = 1,
                    "left" | "h" => change = -1,
                    "down" | "j" => {
                        self.selected_bill =
                            crate::presentation::next_row(self.selected_bill, self.bills.len(), 1);
                    }
                    "up" | "k" => {
                        self.selected_bill =
                            crate::presentation::next_row(self.selected_bill, self.bills.len(), -1);
                    }
                    _ => {}
                }
            }
        }
        if app.pressed("bills:previous") {
            change = -1;
        }
        if app.pressed("bills:next") {
            change = 1;
        }
        if change != 0 && !self.cards.is_empty() {
            self.selected_card = if change > 0 {
                (self.selected_card + 1) % self.cards.len()
            } else {
                (self.selected_card + self.cards.len() - 1) % self.cards.len()
            };
            self.reload_requested = true;
        }
    }
}

pub fn render(ui: &mut Container<'_>, api: &ApiClient) {
    let mut screen = BillsScreen::default();
    screen.load(api, Period::default().range());
    render_with_state(ui, &screen);
}

fn bill_status(status: Option<&str>) -> &str {
    match status {
        Some("open") => "Aberta",
        Some("closed") => "Fechada",
        Some("paid") => "Paga",
        Some("overdue") => "Vencida",
        Some("future") => "Futura",
        Some(status) => status,
        None => "—",
    }
}

fn paid_ratio(bill: &Bill) -> Option<f64> {
    let total = amount(bill.total_amount.as_deref())?;
    let paid = amount(bill.paid_amount.as_deref())?;
    (total > 0.0).then_some((paid / total).clamp(0.0, 1.0))
}

pub fn render_with_state(ui: &mut Container<'_>, screen: &BillsScreen) {
    let theme = ui.theme().clone();
    let compact = ui.width() < 85;
    let card = screen.cards.get(screen.selected_card).map_or_else(
        || "Nenhum cartão".to_string(),
        |card| {
            format!(
                "{}  ·  {} de {}",
                card.name,
                screen.selected_card + 1,
                screen.cards.len()
            )
        },
    );
    let metrics = vec![
        Metric::new(
            "Total das faturas",
            money_total(
                screen
                    .bills
                    .iter()
                    .map(|bill| (bill.total_amount.as_deref(), bill.currency_code.as_deref())),
            ),
            Tone::Neutral,
        ),
        Metric::new(
            "Total pago",
            money_total(
                screen
                    .bills
                    .iter()
                    .map(|bill| (bill.paid_amount.as_deref(), bill.currency_code.as_deref())),
            ),
            Tone::Positive,
        ),
        Metric::new(
            "Em aberto",
            screen
                .bills
                .iter()
                .filter(|bill| {
                    matches!(bill.status.as_deref(), Some("open" | "closed" | "overdue"))
                })
                .count()
                .to_string(),
            Tone::Warning,
        ),
    ];
    let rows = screen
        .bills
        .iter()
        .map(|bill| {
            let currency = bill.currency_code.as_deref();
            let estimate = if bill.amount_is_estimate { "~" } else { "" };
            let mut cells = vec![
                bill_status(bill.status.as_deref()).into(),
                bill.due_date.clone().unwrap_or_else(|| "—".into()),
                format!(
                    "{estimate}{}",
                    format_currency(bill.total_amount.as_deref(), currency)
                ),
                format_currency(bill.paid_amount.as_deref(), currency),
            ];
            if !compact {
                cells.push(format_currency(bill.minimum_payment.as_deref(), currency));
            }
            let status_color = match bill.status.as_deref() {
                Some("paid") => theme.success,
                Some("overdue") => theme.danger,
                _ => theme.warning,
            };
            let mut row = TableRow::new(cells);
            row.cell_colors = vec![
                Some(status_color),
                None,
                None,
                Some(theme.success),
                Some(theme.muted),
            ];
            row
        })
        .collect::<Vec<_>>();
    let error = screen.error.clone();
    let selected = screen.selected_bill;
    let selected_bill = selected.and_then(|index| screen.bills.get(index)).cloned();
    let show_detail = ui.height() >= 16;
    let compact_detail = selected_bill.as_ref().map(|bill| {
        format!(
            "Fechamento {} · Mínimo {}",
            bill.close_date.as_deref().unwrap_or("—"),
            format_currency(
                bill.minimum_payment.as_deref(),
                bill.currency_code.as_deref()
            )
        )
    });
    ui.column(Column::new().gap(usize::from(show_detail)), move |col| {
        if show_detail {
            metric_strip(col, metrics);
        }
        col.row(Row::new().size(Size::Cells(1)).gap(1), move |row| {
            row.sized(Size::Cells(3), |button| {
                button.button(ButtonOptions::new("←"), "bills:previous");
            });
            row.sized(Size::Fill, move |name| {
                name.styled_text(&card, TextStyle::new().fg(theme.accent).bold());
            });
            row.sized(Size::Cells(3), |button| {
                button.button(ButtonOptions::new("→"), "bills:next");
            });
        });
        col.panel(
            Panel::new()
                .title("Faturas com vencimento no período")
                .border(BorderStyle::Rounded)
                .footer("←/→ Cartão  j/k Faturas  ~ Valor estimado"),
            move |panel| {
                if let Some(error) = error {
                    panel.styled_text(&error, TextStyle::new().fg(theme.danger));
                } else if rows.is_empty() {
                    panel.text("Nenhuma fatura neste cartão.");
                    panel.label("Use ←/→ para consultar outro cartão.");
                } else {
                    let mut columns = vec![
                        TableColumn::new("Status").width(Size::Fill),
                        TableColumn::new("Vencimento").width(Size::Cells(10)),
                        TableColumn::new("Total")
                            .width(Size::Cells(17))
                            .align(Align::Right),
                        TableColumn::new("Pago")
                            .width(Size::Cells(17))
                            .align(Align::Right),
                    ];
                    if !compact {
                        columns.push(
                            TableColumn::new("Mínimo")
                                .width(Size::Cells(16))
                                .align(Align::Right),
                        );
                    }
                    panel.table(
                        TableOptions {
                            columns,
                            rows,
                            selected,
                            follow_selection: true,
                            scrollbar: true,
                            header_color: Some(theme.muted),
                            ..Default::default()
                        },
                        "bills:table",
                    );
                }
            },
        );
        if show_detail && let Some(bill) = selected_bill {
            col.panel(
                Panel::new()
                    .title("Pagamento da fatura selecionada")
                    .border(BorderStyle::Rounded)
                    .size(Size::Cells(4)),
                move |panel| {
                    if let Some(ratio) = paid_ratio(&bill) {
                        panel.meter(MeterOptions::new(ratio).label("Pago").color(theme.success));
                    } else {
                        panel.label("Pagamento indisponível");
                    }
                    panel.label(&format!(
                        "Fechamento {} · Mínimo {}",
                        bill.close_date.as_deref().unwrap_or("—"),
                        format_currency(
                            bill.minimum_payment.as_deref(),
                            bill.currency_code.as_deref()
                        )
                    ));
                },
            );
        }
        if !show_detail && let Some(detail) = compact_detail {
            col.label(&detail);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bills_use_selected_due_date_range_and_keep_card_identity() {
        let mut server = mockito::Server::new();
        let cards = server
            .mock("GET", "/v1/spaces/s/cards")
            .with_body(r#"{"data":[{"id":"one","name":"One"},{"id":"two","name":"Two"}]}"#)
            .create();
        let bills = server
            .mock(
                "GET",
                "/v1/spaces/s/cards/two/bills?from=2024-02-01&to=2024-02-29",
            )
            .with_body(r#"{"data":[{"id":"bill","due_date":"2024-02-29"}]}"#)
            .create();
        let api = ApiClient::for_test(
            &server.url(),
            crate::auth::Credential::PersonalToken {
                token: "test-only".into(),
            },
            "s",
        )
        .unwrap();
        let mut screen = BillsScreen {
            cards: serde_json::from_value(serde_json::json!([{"id":"two","name":"Two"}])).unwrap(),
            ..Default::default()
        };
        screen.load(&api, DateRange::parse("2024-02-01", "2024-02-29").unwrap());
        assert_eq!(screen.selected_card, 1);
        assert_eq!(screen.bills[0].id.as_deref(), Some("bill"));
        cards.assert();
        bills.assert();
    }

    #[test]
    fn compact_bills_keep_minimum_and_closing_date_visible() {
        let screen = BillsScreen {
            cards: serde_json::from_value(serde_json::json!([{"id":"c", "name":"Nubank Ultravioleta"}])).unwrap(),
            bills: serde_json::from_value(serde_json::json!([{"status":"open", "due_date":"2026-11-02", "close_date":"2026-10-25", "total_amount":"2450", "paid_amount":"0", "minimum_payment":"367.50"}])).unwrap(),
            selected_bill: Some(0),
            ..Default::default()
        };
        for (width, height) in [(56, 10), (60, 18), (80, 24)] {
            let text = hqtui::testing::render_to_text(width, height, "dark", |ui| {
                render_with_state(ui, &screen);
            });
            for value in [
                "Nubank Ultravioleta",
                "2026-10-25",
                "367,50 BRL",
                "2.450,00 BRL",
            ] {
                assert!(
                    text.contains(value),
                    "Missing {value} at {width}x{height}\n{text}"
                );
            }
        }
    }

    #[test]
    fn paid_progress_requires_known_positive_total() {
        let mut bill: Bill =
            serde_json::from_value(serde_json::json!({"total_amount":"100", "paid_amount":"25"}))
                .unwrap();
        assert_eq!(paid_ratio(&bill), Some(0.25));
        bill.paid_amount = None;
        assert_eq!(paid_ratio(&bill), None);
        bill.paid_amount = Some("25".into());
        bill.total_amount = Some("0".into());
        assert_eq!(paid_ratio(&bill), None);
    }
}
