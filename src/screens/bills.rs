use hqtui::prelude::*;
use hqtui::color::Color;
use hqtui::widgets::{TableOptions, TableColumn, TableRow, SelectOptions};
use hqtui::unicode::Align;

use crate::api::ApiClient;
use crate::models::{Card, Bill};

/// Format a decimal string as BRL-style currency (dots for thousands, comma for decimal).
fn format_money(value: Option<&str>, currency: Option<&str>, is_estimate: bool) -> String {
    let raw = match value {
        Some(v) if !v.is_empty() => v,
        _ => return "-".to_string(),
    };
    let code = currency.unwrap_or("BRL");
    let amount: f64 = match raw.parse() {
        Ok(n) => n,
        Err(_) => {
            let prefix = if is_estimate { "~" } else { "" };
            return format!("{prefix}{raw} {code}");
        }
    };
    let negative = amount < 0.0;
    let abs = amount.abs();
    let whole = abs.trunc() as u64;
    let frac = ((abs.fract()) * 100.0).round() as u64;
    let whole_str = format!("{whole}");
    let grouped: String = whole_str
        .chars()
        .rev()
        .enumerate()
        .flat_map(|(i, c)| {
            if i > 0 && i % 3 == 0 {
                vec!['.', c]
            } else {
                vec![c]
            }
        })
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let sign = if negative { "-" } else { "" };
    let prefix = if is_estimate { "~" } else { "" };
    format!("{prefix}{sign}{grouped},{frac:02} {code}")
}

/// Render the Faturas (Bills) screen content into a container.
pub fn render(ui: &mut Container<'_>, api: &ApiClient) {
    let cards_result = api.cards();

    match cards_result {
        Ok(cards) => {
            if cards.is_empty() {
                ui.panel(Panel::new().title("Faturas"), move |p| {
                    p.label("Nenhum cartão encontrado.");
                });
                return;
            }
            render_content(ui, api, &cards);
        }
        Err(e) => {
            let err_msg = format!("Falha ao carregar cartões: {e}");
            ui.panel(Panel::new().title("Erro"), move |p| {
                p.styled_text(
                    &err_msg,
                    TextStyle::new().fg(Color::rgb(239, 68, 68)),
                );
                p.text("Verifique sua conexão e credenciais.");
            });
        }
    }
}

fn render_content(ui: &mut Container<'_>, api: &ApiClient, cards: &[Card]) {
    // Pre-compute card selector options and default selection.
    let card_options: Vec<String> = cards
        .iter()
        .map(|c| {
            let provider = c.provider_name.as_deref().unwrap_or("");
            if provider.is_empty() {
                c.name.clone()
            } else {
                format!("{} ({})", c.name, provider)
            }
        })
        .collect();
    let selected_card_index: usize = 0;
    let selected_card_value = card_options.first().cloned().unwrap_or_default();
    let selected_card_id = cards.first().map(|c| c.id.clone()).unwrap_or_default();

    // Load bills for the initially selected card.
    let bills_result = api.bills(&selected_card_id, None, None);

    // Pre-compute table columns.
    let columns = vec![
        TableColumn::new("Status").width(Size::Fr(1.0)),
        TableColumn::new("Fechamento").width(Size::Fr(1.5)),
        TableColumn::new("Vencimento").width(Size::Fr(1.5)),
        TableColumn::new("Total").width(Size::Fr(2.0)).align(Align::Right),
        TableColumn::new("Pago").width(Size::Fr(2.0)).align(Align::Right),
        TableColumn::new("Mínimo").width(Size::Fr(2.0)).align(Align::Right),
    ];

    // Pre-compute bill table rows.
    let (bill_rows, has_bills, bills_error): (Vec<TableRow>, bool, Option<String>) =
        match bills_result {
            Ok(bills) => {
                let rows: Vec<TableRow> = bills
                    .iter()
                    .map(|b| {
                        let currency = b.currency_code.as_deref();
                        let est = b.amount_is_estimate;
                        let status = b.status.as_deref().unwrap_or("-").to_string();
                        let close_date = b.close_date.as_deref().unwrap_or("-").to_string();
                        let due_date = b.due_date.as_deref().unwrap_or("-").to_string();
                        let total = format_money(b.total_amount.as_deref(), currency, est);
                        let paid = format_money(b.paid_amount.as_deref(), currency, false);
                        let minimum = format_money(b.minimum_payment.as_deref(), currency, false);
                        TableRow::new([status, close_date, due_date, total, paid, minimum])
                    })
                    .collect();
                let has = !rows.is_empty();
                (rows, has, None)
            }
            Err(e) => (Vec::new(), false, Some(format!("Falha ao carregar faturas: {e}"))),
        };

    // Clone everything needed by closures.
    let card_options_c = card_options.clone();
    let selected_value_c = selected_card_value.clone();
    let columns_c = columns.clone();
    let bill_rows_c = bill_rows.clone();
    let error_msg = bills_error.clone();

    ui.column(Column::new().gap(1), move |col| {
        // Card selector row.
        col.row(Row::new().size(Size::Cells(3)).gap(1), move |r| {
            r.column(Column::new().size(Size::Cells(10)).gap(0), move |c| {
                c.label("Cartão:");
            });
            r.column(Column::new().size(Size::Fill).gap(0), move |c| {
                c.select(
                    SelectOptions {
                        options: card_options_c.clone(),
                        selected_index: Some(selected_card_index),
                        width: Some(40),
                        ..SelectOptions::new(selected_value_c.clone())
                    },
                    "bills:card_select",
                );
            });
        });

        // Bills panel.
        col.panel(Panel::new().title("Faturas"), move |p| {
            if let Some(ref err) = error_msg {
                p.styled_text(
                    err,
                    TextStyle::new().fg(Color::rgb(239, 68, 68)),
                );
                p.text("Tente novamente ou verifique sua conexão.");
            } else if has_bills {
                p.table(
                    TableOptions {
                        columns: columns_c.clone(),
                        rows: bill_rows_c.clone(),
                        header: true,
                        zebra: true,
                        scrollbar: true,
                        ..Default::default()
                    },
                    "bills_table",
                );
            } else {
                p.label("Nenhuma fatura encontrada para este cartão.");
            }
        });
    });
}