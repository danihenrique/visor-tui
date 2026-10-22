use hqtui::color::Color;
use hqtui::prelude::*;
use hqtui::unicode::Align;
use hqtui::widgets::{KeyValueOptions, KeyValueRow, TableColumn, TableOptions, TableRow};

use crate::api::{ApiClient, ApiError};
use crate::models::{Investment, InvestmentSummary};

/// Finance palette: greens for positive balances, reds for negative, neutral for labels.
const COLOR_POSITIVE: Color = Color::rgb(34, 197, 94); // green-500
const COLOR_NEGATIVE: Color = Color::rgb(239, 68, 68); // red-500
const COLOR_ACCENT: Color = Color::rgb(14, 165, 233); // sky-500

fn format_money(amount: Option<&str>, currency: Option<&str>) -> String {
    crate::presentation::format_currency(amount, currency)
}

fn money_color(amount: Option<&str>) -> Option<Color> {
    match amount {
        Some(s) => match s.parse::<f64>() {
            Ok(v) if v > 0.0 => Some(COLOR_POSITIVE),
            Ok(v) if v < 0.0 => Some(COLOR_NEGATIVE),
            _ => None,
        },
        None => None,
    }
}

struct InvestmentRowData {
    name: String,
    investment_type: String,
    category: String,
    balance_fmt: String,
    provider: String,
}

fn build_row_data(items: &[Investment]) -> Vec<InvestmentRowData> {
    items
        .iter()
        .map(|inv| {
            let currency = inv.currency_code.as_deref();
            let balance_fmt = format_money(inv.balance.as_deref(), currency);
            InvestmentRowData {
                name: inv.name.clone(),
                investment_type: inv
                    .investment_type
                    .clone()
                    .unwrap_or_else(|| "—".to_string()),
                category: inv.category.clone().unwrap_or_else(|| "—".to_string()),
                balance_fmt,
                provider: inv.provider_name.clone().unwrap_or_else(|| "—".to_string()),
            }
        })
        .collect()
}

fn table_columns() -> Vec<TableColumn> {
    vec![
        TableColumn::new("Nome").width(Size::Fill),
        TableColumn::new("Tipo")
            .width(Size::Cells(16))
            .align(Align::Left),
        TableColumn::new("Categoria")
            .width(Size::Cells(16))
            .align(Align::Left),
        TableColumn::new("Saldo")
            .width(Size::Cells(20))
            .align(Align::Right),
        TableColumn::new("Provedor")
            .width(Size::Cells(18))
            .align(Align::Left),
    ]
}

fn render_error(ui: &mut Container<'_>, err: &ApiError) {
    let err_msg = format!("Falha ao carregar patrimônio: {err}");
    ui.panel(Panel::new().title("Erro"), move |p| {
        p.styled_text(&err_msg, TextStyle::new().fg(COLOR_NEGATIVE));
        p.label("Verifique sua conexão e credenciais.");
    });
}

#[allow(clippy::cast_possible_truncation)]
pub fn render(ui: &mut Container<'_>, api: &ApiClient) {
    match api.investments() {
        Ok((investments, summary)) => render_net_worth_data(ui, &investments, summary.as_ref()),
        Err(e) => render_error(ui, &e),
    }
}

fn render_net_worth_data(
    ui: &mut Container<'_>,
    investments: &[Investment],
    summary: Option<&InvestmentSummary>,
) {
    // Pre-compute summary values before closures
    let currency = summary
        .as_ref()
        .and_then(|s| s.currency_code.clone())
        .unwrap_or_else(|| "BRL".to_string());

    let total_value_fmt = summary
        .as_ref()
        .and_then(|s| s.total_value.as_deref())
        .map_or_else(
            || "—".to_string(),
            |v| format_money(Some(v), Some(&currency)),
        );

    let total_value_color = summary
        .as_ref()
        .and_then(|s| s.total_value.as_deref())
        .and_then(|v| money_color(Some(v)))
        .unwrap_or(COLOR_ACCENT);

    let investment_count = summary
        .as_ref()
        .and_then(|s| s.count)
        .unwrap_or_else(|| investments.len().try_into().unwrap_or(u32::MAX));

    let count_label = format!("{investment_count}");

    // Pre-compute summary key-value rows
    let summary_rows = vec![
        KeyValueRow::new("Valor Total", total_value_fmt.clone()).color(total_value_color),
        KeyValueRow::new("Investimentos", count_label),
    ];

    // Pre-compute table row data
    let row_data = build_row_data(investments);

    // Build table rows — we cannot color individual cells via TableRow API,
    // so we embed the formatted balance string directly.
    let table_rows: Vec<TableRow> = row_data
        .iter()
        .map(|r| {
            TableRow::new([
                r.name.clone(),
                r.investment_type.clone(),
                r.category.clone(),
                r.balance_fmt.clone(),
                r.provider.clone(),
            ])
        })
        .collect();

    let has_rows = !table_rows.is_empty();
    let columns = table_columns();

    // Pre-compute footer text
    let footer_total_label = format!("Patrimônio Líquido: {total_value_fmt}");
    let footer_color = total_value_color;

    ui.column(Column::new().gap(1), move |col| {
        // Summary panel at top
        col.panel(Panel::new().title("Resumo do Patrimônio"), move |p| {
            p.key_values(KeyValueOptions {
                rows: summary_rows.clone(),
                ..Default::default()
            });
        });

        // Investments table
        col.panel(Panel::new().title("Investimentos"), move |p| {
            if has_rows {
                p.table(
                    TableOptions {
                        columns: columns.clone(),
                        rows: table_rows.clone(),
                        header: true,
                        zebra: true,
                        ..Default::default()
                    },
                    "",
                );
            } else {
                p.label("Nenhum investimento encontrado.");
            }
        });

        // Footer with total
        col.panel(Panel::new().title("Total"), move |p| {
            p.styled_text(&footer_total_label, TextStyle::new().fg(footer_color));
        });
    });
}
