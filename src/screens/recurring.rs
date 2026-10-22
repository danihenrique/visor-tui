use hqtui::prelude::*;
use hqtui::color::Color;
use hqtui::widgets::{TableOptions, TableColumn, TableRow, ButtonOptions, ButtonVariant};
use hqtui::unicode::Align;

use crate::api::{ApiClient, ApiError};
use crate::models::RecurringExpense;

const COLOR_NEGATIVE: Color = Color::rgb(239, 68, 68);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Subscriptions,
    FixedBills,
}

fn format_money(amount: Option<&str>, currency: Option<&str>) -> String {
    let raw = match amount {
        Some(s) if !s.is_empty() => s,
        _ => return "—".to_string(),
    };
    let curr = currency.unwrap_or("BRL");

    match raw.parse::<f64>() {
        Ok(v) => {
            let negative = v < 0.0;
            let abs = v.abs();
            let integer_part = abs.trunc() as u64;
            let decimal_part = ((abs.fract()) * 100.0).round() as u64;
            let int_str = format_integer_with_dots(integer_part);
            let formatted = format!("{int_str},{decimal_part:02} {curr}");
            if negative {
                format!("-{formatted}")
            } else {
                formatted
            }
        }
        Err(_) => format!("{raw} {curr}"),
    }
}

fn format_integer_with_dots(n: u64) -> String {
    let s = n.to_string();
    let len = s.len();
    if len <= 3 {
        return s;
    }
    let mut result = String::with_capacity(len + len / 3);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            result.push('.');
        }
        result.push(ch);
    }
    result
}

fn format_frequency(freq: Option<&str>) -> String {
    match freq {
        Some(f) if !f.is_empty() => f.to_string(),
        _ => "—".to_string(),
    }
}

fn format_date(date: Option<&str>) -> String {
    match date {
        Some(d) if !d.is_empty() => d.to_string(),
        _ => "—".to_string(),
    }
}

struct RecurringRowData {
    name: String,
    amount_fmt: String,
    frequency: String,
    next_due: String,
    status: String,
}

fn build_row_data(items: &[RecurringExpense]) -> Vec<RecurringRowData> {
    items
        .iter()
        .map(|item| {
            let currency = item.currency_code.as_deref();
            let amount_fmt = format_money(item.amount.as_deref(), currency);
            let frequency = format_frequency(item.frequency.as_deref());
            let next_due = format_date(item.next_due_date.as_deref());
            let status = if item.active {
                "Ativo".to_string()
            } else {
                "Inativo".to_string()
            };
            RecurringRowData {
                name: item.name.clone(),
                amount_fmt,
                frequency,
                next_due,
                status,
            }
        })
        .collect()
}

fn table_columns() -> Vec<TableColumn> {
    vec![
        TableColumn::new("Nome").width(Size::Fill),
        TableColumn::new("Valor").width(Size::Cells(18)).align(Align::Right),
        TableColumn::new("Frequência").width(Size::Cells(14)).align(Align::Center),
        TableColumn::new("Próximo Vencimento").width(Size::Cells(18)).align(Align::Right),
        TableColumn::new("Status").width(Size::Cells(10)).align(Align::Center),
    ]
}

fn draw_error(ui: &mut Container<'_>, err: &ApiError) {
    let err_msg = format!("Falha ao carregar recorrentes: {err}");
    ui.panel(Panel::new().title("Erro"), move |p| {
        p.styled_text(&err_msg, TextStyle::new().fg(COLOR_NEGATIVE));
        p.label("Verifique sua conexão e credenciais.");
    });
}

pub fn render(ui: &mut Container<'_>, api: &ApiClient) {
    let subs_result = api.subscriptions();
    let bills_result = api.fixed_bills();

    // Check for errors first — if either fails, show error panel
    let subscriptions = match subs_result {
        Ok(data) => data,
        Err(ref e) => {
            draw_error(ui, e);
            return;
        }
    };
    let fixed_bills = match bills_result {
        Ok(data) => data,
        Err(ref e) => {
            draw_error(ui, e);
            return;
        }
    };

    // Default tab
    let active_tab = Tab::Subscriptions;

    // Pre-compute all row data before entering closures
    let subs_rows = build_row_data(&subscriptions);
    let bills_rows = build_row_data(&fixed_bills);

    let subs_count = subscriptions.len();
    let bills_count = fixed_bills.len();

    let columns = table_columns();

    // Build table rows for subscriptions
    let subs_table_rows: Vec<TableRow> = subs_rows
        .iter()
        .map(|r| {
            TableRow::new([
                r.name.clone(),
                r.amount_fmt.clone(),
                r.frequency.clone(),
                r.next_due.clone(),
                r.status.clone(),
            ])
        })
        .collect();

    // Build table rows for fixed bills
    let bills_table_rows: Vec<TableRow> = bills_rows
        .iter()
        .map(|r| {
            TableRow::new([
                r.name.clone(),
                r.amount_fmt.clone(),
                r.frequency.clone(),
                r.next_due.clone(),
                r.status.clone(),
            ])
        })
        .collect();

    // Pre-compute tab labels with counts
    let subs_label = format!("Assinaturas ({subs_count})");
    let bills_label = format!("Contas Fixas ({bills_count})");

    // Determine which button variant to use based on active tab
    let subs_variant = if active_tab == Tab::Subscriptions {
        ButtonVariant::Primary
    } else {
        ButtonVariant::Ghost
    };
    let bills_variant = if active_tab == Tab::FixedBills {
        ButtonVariant::Primary
    } else {
        ButtonVariant::Ghost
    };

    // Pre-compute title for the data panel
    let panel_title = match active_tab {
        Tab::Subscriptions => "Assinaturas",
        Tab::FixedBills => "Contas Fixas",
    };

    // Select the correct table rows and empty message based on active tab
    let (current_table_rows, empty_msg) = match active_tab {
        Tab::Subscriptions => (subs_table_rows, "Nenhuma assinatura encontrada."),
        Tab::FixedBills => (bills_table_rows, "Nenhuma conta fixa encontrada."),
    };
    let has_rows = !current_table_rows.is_empty();

    ui.column(Column::new().gap(1), move |col| {
        // Tab buttons row
        col.row(Row::new().gap(1), move |r| {
            r.button(
                ButtonOptions::new(subs_label.clone()).variant(subs_variant),
                "tab_subscriptions",
            );
            r.button(
                ButtonOptions::new(bills_label.clone()).variant(bills_variant),
                "tab_fixed_bills",
            );
        });

        // Data panel
        col.panel(Panel::new().title(panel_title), move |p| {
            if has_rows {
                p.table(
                    TableOptions {
                        columns: columns.clone(),
                        rows: current_table_rows.clone(),
                        header: true,
                        zebra: true,
                        ..Default::default()
                    },
                    "",
                );
            } else {
                p.label(empty_msg);
            }
        });
    });
}