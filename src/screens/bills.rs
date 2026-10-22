use hqtui::input::InputEvent;
use hqtui::prelude::*;
use hqtui::unicode::Align;

use crate::api::ApiClient;
use crate::models::{Bill, Card};
use crate::presentation::format_currency;

#[derive(Default)]
pub struct BillsScreen {
    cards: Vec<Card>,
    bills: Vec<Bill>,
    selected_card: usize,
    selected_bill: Option<usize>,
    error: Option<String>,
}

impl BillsScreen {
    pub fn load(&mut self, api: &ApiClient) {
        match api.cards() {
            Ok(cards) => {
                self.cards = cards;
                self.selected_card = self.selected_card.min(self.cards.len().saturating_sub(1));
                self.load_bills(api);
            }
            Err(error) => self.error = Some(format!("Falha ao carregar cartões: {error}")),
        }
    }

    fn load_bills(&mut self, api: &ApiClient) {
        self.error = None;
        self.bills.clear();
        self.selected_bill = None;
        if let Some(card) = self.cards.get(self.selected_card) {
            match api.bills(&card.id, None, None) {
                Ok(bills) => {
                    self.bills = bills;
                    self.selected_bill = (!self.bills.is_empty()).then_some(0);
                }
                Err(error) => self.error = Some(format!("Falha ao carregar faturas: {error}")),
            }
        }
    }

    pub fn handle_input(&mut self, app: &App, api: &ApiClient, events: &[InputEvent]) {
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
            self.load_bills(api);
        }
    }
}

pub fn render(ui: &mut Container<'_>, api: &ApiClient) {
    let mut screen = BillsScreen::default();
    screen.load(api);
    render_with_state(ui, &screen);
}

pub fn render_with_state(ui: &mut Container<'_>, screen: &BillsScreen) {
    let card = screen.cards.get(screen.selected_card).map_or_else(
        || "Nenhum cartão".to_string(),
        |card| {
            format!(
                "{}  ({}/{})",
                card.name,
                screen.selected_card + 1,
                screen.cards.len()
            )
        },
    );
    let rows = screen
        .bills
        .iter()
        .map(|bill| {
            let currency = bill.currency_code.as_deref();
            let estimate = if bill.amount_is_estimate { "~" } else { "" };
            TableRow::new([
                bill.status.clone().unwrap_or_else(|| "—".into()),
                bill.due_date.clone().unwrap_or_else(|| "—".into()),
                format!(
                    "{estimate}{}",
                    format_currency(bill.total_amount.as_deref(), currency)
                ),
                format_currency(bill.paid_amount.as_deref(), currency),
                format_currency(bill.minimum_payment.as_deref(), currency),
            ])
        })
        .collect::<Vec<_>>();
    let error = screen.error.clone();
    let selected = screen.selected_bill;
    let detail = selected
        .and_then(|index| screen.bills.get(index))
        .map(|bill| format!("Fechamento {}", bill.close_date.as_deref().unwrap_or("—")));
    ui.column(Column::new().gap(1), move |col| {
        col.row(Row::new().size(Size::Cells(1)).gap(1), move |row| {
            row.button(ButtonOptions::new("←"), "bills:previous");
            row.text(&card);
            row.button(ButtonOptions::new("→"), "bills:next");
        });
        col.panel(
            Panel::new()
                .title("Faturas")
                .footer("←/→ Cartão  j/k Faturas"),
            move |panel| {
                if let Some(error) = error {
                    panel.text(&error);
                } else if rows.is_empty() {
                    panel.text("Nenhuma fatura neste cartão.");
                } else {
                    panel.table(
                        TableOptions {
                            columns: vec![
                                TableColumn::new("Status"),
                                TableColumn::new("Vencimento"),
                                TableColumn::new("Total").align(Align::Right),
                                TableColumn::new("Pago").align(Align::Right),
                                TableColumn::new("Mínimo").align(Align::Right),
                            ],
                            rows,
                            selected,
                            zebra: true,
                            follow_selection: true,
                            ..Default::default()
                        },
                        "bills:table",
                    );
                    if let Some(detail) = detail {
                        panel.text(&detail);
                    }
                }
            },
        );
    });
}
