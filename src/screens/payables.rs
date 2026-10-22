use hqtui::prelude::*;
use hqtui::color::Color;
use hqtui::widgets::{TableOptions, TableColumn, TableRow, ButtonOptions, ButtonVariant};
use hqtui::unicode::Align;

use crate::api::{ApiClient, ApiError};
use crate::models::InstallmentPlan;

const COLOR_NEGATIVE: Color = Color::rgb(239, 68, 68);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    All,
    Active,
    Completed,
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
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
        if i > 0 && (len - i).is_multiple_of(3) {
            result.push('.');
        }
        result.push(ch);
    }
    result
}

fn format_date(date: Option<&str>) -> String {
    match date {
        Some(d) if !d.is_empty() => d.to_string(),
        _ => "—".to_string(),
    }
}

fn format_progress(paid: Option<u32>, total: Option<u32>) -> String {
    match (paid, total) {
        (Some(p), Some(t)) => format!("{p}/{t}"),
        (Some(p), None) => format!("{p}/?"),
        (None, Some(t)) => format!("?/{t}"),
        (None, None) => "—".to_string(),
    }
}

fn status_label(status: Option<&str>) -> String {
    match status {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => "—".to_string(),
    }
}

#[derive(Clone)]
struct PayableRowData {
    description: String,
    progress: String,
    installment_amount: String,
    total: String,
    remaining: String,
    next_due: String,
    status: String,
}

fn build_row_data(items: &[InstallmentPlan]) -> Vec<PayableRowData> {
    items
        .iter()
        .map(|item| {
            let currency = item.currency_code.as_deref();
            let description = item.description.clone().unwrap_or_else(|| "Sem descrição".to_string());
            let progress = format_progress(item.paid_installments, item.total_installments);
            let installment_amount = format_money(item.installment_amount.as_deref(), currency);
            let total = format_money(item.total_amount.as_deref(), currency);
            let remaining = format_money(item.remaining_amount.as_deref(), currency);
            let next_due = format_date(item.next_due_date.as_deref());
            let status = status_label(item.status.as_deref());
            PayableRowData {
                description,
                progress,
                installment_amount,
                total,
                remaining,
                next_due,
                status,
            }
        })
        .collect()
}

fn table_columns() -> Vec<TableColumn> {
    vec![
        TableColumn::new("Descrição").width(Size::Fill),
        TableColumn::new("Parcelas").width(Size::Cells(10)).align(Align::Center),
        TableColumn::new("Valor Parcela").width(Size::Cells(18)).align(Align::Right),
        TableColumn::new("Total").width(Size::Cells(18)).align(Align::Right),
        TableColumn::new("Restante").width(Size::Cells(18)).align(Align::Right),
        TableColumn::new("Próximo Vencimento").width(Size::Cells(18)).align(Align::Right),
        TableColumn::new("Status").width(Size::Cells(12)).align(Align::Center),
    ]
}

fn draw_error(ui: &mut Container<'_>, err: &ApiError) {
    let err_msg = format!("Falha ao carregar contas a pagar: {err}");
    ui.panel(Panel::new().title("Erro"), move |p| {
        p.styled_text(&err_msg, TextStyle::new().fg(COLOR_NEGATIVE));
        p.label("Verifique sua conexão e credenciais.");
    });
}

pub fn render(ui: &mut Container<'_>, api: &ApiClient) {
    let plans_result = api.installment_plans(None);

    let plans = match plans_result {
        Ok(data) => data,
        Err(ref e) => {
            draw_error(ui, e);
            return;
        }
    };

    // Default tab
    let active_tab = Tab::All;

    // Pre-compute all row data before entering closures
    let all_rows = build_row_data(&plans);

    // Filter rows by tab
    let active_rows: Vec<PayableRowData> = all_rows
        .iter()
        .filter(|r| r.status == "active" || r.status == "Active" || r.status == "ativo" || r.status == "Ativo")
        .cloned()
        .collect();

    let completed_rows: Vec<PayableRowData> = all_rows
        .iter()
        .filter(|r| r.status == "completed" || r.status == "Completed" || r.status == "concluído" || r.status == "Concluído" || r.status == "concluido" || r.status == "Concluido")
        .cloned()
        .collect();

    let all_count = all_rows.len();
    let active_count = active_rows.len();
    let completed_count = completed_rows.len();

    let columns = table_columns();

    // Build table rows for each tab
    let all_table_rows: Vec<TableRow> = all_rows
        .iter()
        .map(|r| {
            TableRow::new([
                r.description.clone(),
                r.progress.clone(),
                r.installment_amount.clone(),
                r.total.clone(),
                r.remaining.clone(),
                r.next_due.clone(),
                r.status.clone(),
            ])
        })
        .collect();

    let active_table_rows: Vec<TableRow> = active_rows
        .iter()
        .map(|r| {
            TableRow::new([
                r.description.clone(),
                r.progress.clone(),
                r.installment_amount.clone(),
                r.total.clone(),
                r.remaining.clone(),
                r.next_due.clone(),
                r.status.clone(),
            ])
        })
        .collect();

    let completed_table_rows: Vec<TableRow> = completed_rows
        .iter()
        .map(|r| {
            TableRow::new([
                r.description.clone(),
                r.progress.clone(),
                r.installment_amount.clone(),
                r.total.clone(),
                r.remaining.clone(),
                r.next_due.clone(),
                r.status.clone(),
            ])
        })
        .collect();

    // Pre-compute tab labels with counts
    let all_label = format!("Todos ({all_count})");
    let active_label = format!("Ativos ({active_count})");
    let completed_label = format!("Concluídos ({completed_count})");

    // Determine button variants based on active tab
    let all_variant = if active_tab == Tab::All {
        ButtonVariant::Primary
    } else {
        ButtonVariant::Ghost
    };
    let active_variant = if active_tab == Tab::Active {
        ButtonVariant::Primary
    } else {
        ButtonVariant::Ghost
    };
    let completed_variant = if active_tab == Tab::Completed {
        ButtonVariant::Primary
    } else {
        ButtonVariant::Ghost
    };

    // Select the correct table rows and empty message based on active tab
    let (current_table_rows, empty_msg) = match active_tab {
        Tab::All => (all_table_rows, "Nenhum parcelamento encontrado."),
        Tab::Active => (active_table_rows, "Nenhum parcelamento ativo encontrado."),
        Tab::Completed => (completed_table_rows, "Nenhum parcelamento concluído encontrado."),
    };
    let has_rows = !current_table_rows.is_empty();

    ui.column(Column::new().gap(1), move |col| {
        // Tab buttons row
        col.row(Row::new().gap(1), move |r| {
            r.button(
                ButtonOptions::new(all_label.clone()).variant(all_variant),
                "tab_all",
            );
            r.button(
                ButtonOptions::new(active_label.clone()).variant(active_variant),
                "tab_active",
            );
            r.button(
                ButtonOptions::new(completed_label.clone()).variant(completed_variant),
                "tab_completed",
            );
        });

        // Data panel
        col.panel(Panel::new().title("Contas a Pagar"), move |p| {
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