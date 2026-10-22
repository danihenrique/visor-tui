use hqtui::color::Color;
use hqtui::prelude::*;
use hqtui::unicode::Align;
use hqtui::widgets::{KeyValueOptions, KeyValueRow, TableColumn, TableOptions, TableRow};

use crate::api::{ApiClient, ApiError};
use crate::models::Overview;

/// Finance palette: greens for positive, reds for debt/negative, neutral for labels.
const COLOR_POSITIVE: Color = Color::rgb(34, 197, 94); // green-500
const COLOR_NEGATIVE: Color = Color::rgb(239, 68, 68); // red-500
const COLOR_NEUTRAL: Color = Color::rgb(148, 163, 184); // slate-400
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

pub fn draw_overview(ui: &mut Container<'_>, api: &ApiClient) {
    match api.overview() {
        Ok(data) => draw_overview_data(ui, &data),
        Err(e) => draw_error(ui, &e),
    }
}

fn draw_error(ui: &mut Container<'_>, err: &ApiError) {
    let err_msg = format!("Falha ao carregar visão geral: {err}");
    ui.panel(Panel::new().title("Erro"), move |p| {
        p.styled_text(&err_msg, TextStyle::new().fg(COLOR_NEGATIVE));
        p.label("Verifique sua conexão e credenciais.");
    });
}

fn draw_overview_data(ui: &mut Container<'_>, data: &Overview) {
    let currency = data
        .currency_code
        .clone()
        .unwrap_or_else(|| "BRL".to_string());

    // Pre-compute all formatted strings before entering closures
    let bank_balance_fmt = format_money(data.bank_balance.as_deref(), Some(&currency));
    let bank_balance_color = money_color(data.bank_balance.as_deref()).unwrap_or(COLOR_POSITIVE);

    let credit_debt_fmt = format_money(data.credit_card_debt.as_deref(), Some(&currency));
    let credit_debt_positive = data
        .credit_card_debt
        .as_deref()
        .and_then(|s| s.parse::<f64>().ok())
        .is_some_and(|v| v > 0.0);
    let credit_debt_color = if credit_debt_positive {
        COLOR_NEGATIVE
    } else {
        COLOR_POSITIVE
    };

    let open_bills_fmt = format_money(data.open_bills_total.as_deref(), Some(&currency));
    let net_worth_fmt = format_money(data.net_worth.as_deref(), Some(&currency));
    let net_worth_color = money_color(data.net_worth.as_deref()).unwrap_or(COLOR_POSITIVE);

    // Period data
    let period_rows = data.period.as_ref().map(|period| {
        let income_fmt = format_money(period.income.as_deref(), Some(&currency));
        let expenses_fmt = format_money(period.expenses.as_deref(), Some(&currency));
        let result_fmt = format_money(period.result.as_deref(), Some(&currency));
        let result_color = money_color(period.result.as_deref()).unwrap_or(COLOR_NEUTRAL);
        vec![
            KeyValueRow::new("Receitas", income_fmt).color(COLOR_POSITIVE),
            KeyValueRow::new("Despesas", expenses_fmt).color(COLOR_NEGATIVE),
            KeyValueRow::new("Resultado", result_fmt).color(result_color),
        ]
    });

    let period_label = match (&data.period_start, &data.period_end) {
        (Some(s), Some(e)) => format!("{s} a {e}"),
        _ => String::new(),
    };

    // Projection data
    let projection_rows = data.projection.as_ref().map(|proj| {
        let current_bal_fmt = format_money(proj.current_balance.as_deref(), Some(&currency));
        let expected_income_fmt = format_money(proj.expected_income.as_deref(), Some(&currency));
        let projected_bal_fmt = format_money(proj.projected_balance.as_deref(), Some(&currency));
        let projected_bal_color =
            money_color(proj.projected_balance.as_deref()).unwrap_or(COLOR_ACCENT);

        let mut rows = vec![
            KeyValueRow::new("Saldo atual", current_bal_fmt),
            KeyValueRow::new("Receita esperada", expected_income_fmt).color(COLOR_POSITIVE),
        ];

        if let Some(outflows) = &proj.committed_outflows {
            if let Some(ref bills) = outflows.bills {
                rows.push(
                    KeyValueRow::new(
                        "  Faturas",
                        format_money(Some(bills.as_str()), Some(&currency)),
                    )
                    .color(COLOR_NEGATIVE),
                );
            }
            if let Some(ref inst) = outflows.installments {
                rows.push(
                    KeyValueRow::new(
                        "  Parcelas",
                        format_money(Some(inst.as_str()), Some(&currency)),
                    )
                    .color(COLOR_NEGATIVE),
                );
            }
            if let Some(ref subs) = outflows.subscriptions {
                rows.push(
                    KeyValueRow::new(
                        "  Assinaturas",
                        format_money(Some(subs.as_str()), Some(&currency)),
                    )
                    .color(COLOR_NEGATIVE),
                );
            }
            if let Some(ref sched) = outflows.scheduled {
                rows.push(
                    KeyValueRow::new(
                        "  Agendados",
                        format_money(Some(sched.as_str()), Some(&currency)),
                    )
                    .color(COLOR_NEGATIVE),
                );
            }
        }

        rows.push(
            KeyValueRow::new("Saldo projetado", projected_bal_fmt).color(projected_bal_color),
        );
        rows
    });

    // Upcoming bills table
    let bill_columns = vec![
        TableColumn::new("Descrição").width(Size::Fill),
        TableColumn::new("Valor")
            .width(Size::Cells(18))
            .align(Align::Right),
        TableColumn::new("Vencimento")
            .width(Size::Cells(12))
            .align(Align::Right),
    ];
    let bill_rows: Vec<TableRow> = data
        .upcoming_bills
        .iter()
        .map(|bill| {
            let desc = bill.description.as_deref().unwrap_or("—").to_string();
            let amount = format_money(bill.amount.as_deref(), Some(&currency));
            let due = bill.due_date.as_deref().unwrap_or("—").to_string();
            let est = if bill.amount_is_estimate { "~" } else { "" };
            TableRow::new([desc, format!("{est}{amount}"), due])
        })
        .collect();
    let has_bills = !data.upcoming_bills.is_empty();

    ui.column(Column::new().gap(1), move |col| {
        // Top row: summary KPIs
        col.row(Row::new().gap(1), move |row| {
            // Balances panel
            row.column(Column::new().size(Size::Fr(1.0)).gap(0), move |c| {
                c.panel(Panel::new().title("Saldos"), move |p| {
                    let rows = vec![
                        KeyValueRow::new("Saldo em conta", bank_balance_fmt.clone())
                            .color(bank_balance_color),
                        KeyValueRow::new("Dívida cartão", credit_debt_fmt.clone())
                            .color(credit_debt_color),
                        KeyValueRow::new("Faturas abertas", open_bills_fmt.clone())
                            .color(COLOR_NEUTRAL),
                        KeyValueRow::new("Patrimônio líquido", net_worth_fmt.clone())
                            .color(net_worth_color),
                    ];
                    p.key_values(KeyValueOptions {
                        rows,
                        ..Default::default()
                    });
                });
            });

            // Period panel
            row.column(Column::new().size(Size::Fr(1.0)).gap(0), move |c| {
                c.panel(Panel::new().title("Período"), move |p| {
                    if let Some(ref rows) = period_rows {
                        if !period_label.is_empty() {
                            p.label(&period_label);
                        }
                        p.key_values(KeyValueOptions {
                            rows: rows.clone(),
                            ..Default::default()
                        });
                    } else {
                        p.label("Sem dados do período.");
                    }
                });
            });
        });

        // Middle row: projection and upcoming bills
        col.row(Row::new().gap(1), move |row| {
            // Projection panel
            row.column(Column::new().size(Size::Fr(1.0)).gap(0), move |c| {
                c.panel(Panel::new().title("Projeção"), move |p| {
                    if let Some(ref rows) = projection_rows {
                        p.key_values(KeyValueOptions {
                            rows: rows.clone(),
                            ..Default::default()
                        });
                    } else {
                        p.label("Sem dados de projeção.");
                    }
                });
            });

            // Upcoming bills panel
            row.column(Column::new().size(Size::Fr(1.0)).gap(0), move |c| {
                c.panel(Panel::new().title("Próximas Faturas"), move |p| {
                    if has_bills {
                        p.table(
                            TableOptions {
                                columns: bill_columns.clone(),
                                rows: bill_rows.clone(),
                                header: true,
                                zebra: true,
                                ..Default::default()
                            },
                            "",
                        );
                    } else {
                        p.label("Nenhuma fatura próxima.");
                    }
                });
            });
        });
    });
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn draw_overview_mock(ui: &mut Container<'_>, data: &Overview) {
    draw_overview_data(ui, data);
}
