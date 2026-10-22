use hqtui::prelude::*;
use hqtui::color::Color;
use hqtui::widgets::{KeyValueOptions, KeyValueRow};

use crate::api::{ApiClient, ApiError};
use crate::models::Overview;

/// Finance palette: greens for positive/income, reds for negative/expenses, neutral for labels.
const COLOR_POSITIVE: Color = Color::rgb(34, 197, 94);   // green-500
const COLOR_NEGATIVE: Color = Color::rgb(239, 68, 68);   // red-500
const COLOR_NEUTRAL: Color = Color::rgb(148, 163, 184);  // slate-400
const COLOR_ACCENT: Color = Color::rgb(14, 165, 233);    // sky-500

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

pub fn render(ui: &mut Container<'_>, api: &ApiClient) {
    match api.overview() {
        Ok(data) => render_cash_flow_data(ui, &data),
        Err(e) => render_error(ui, &e),
    }
}

fn render_error(ui: &mut Container<'_>, err: &ApiError) {
    let err_msg = format!("Falha ao carregar fluxo de caixa: {err}");
    ui.panel(Panel::new().title("Erro"), move |p| {
        p.styled_text(
            &err_msg,
            TextStyle::new().fg(COLOR_NEGATIVE),
        );
        p.label("Verifique sua conexão e credenciais.");
    });
}

fn render_cash_flow_data(ui: &mut Container<'_>, data: &Overview) {
    let currency = data.currency_code.clone().unwrap_or_else(|| "BRL".to_string());

    // Period heading
    let period_heading = match (&data.period_start, &data.period_end) {
        (Some(s), Some(e)) => format!("Período: {s} a {e}"),
        (Some(s), None) => format!("Período: a partir de {s}"),
        (None, Some(e)) => format!("Período: até {e}"),
        (None, None) => String::from("Fluxo de Caixa"),
    };

    // Pre-compute period key-value rows
    let period_rows: Vec<KeyValueRow> = data.period.as_ref().map(|period| {
        let income_fmt = format_money(period.income.as_deref(), Some(&currency));
        let expenses_fmt = format_money(period.expenses.as_deref(), Some(&currency));
        let result_fmt = format_money(period.result.as_deref(), Some(&currency));
        let result_color = money_color(period.result.as_deref()).unwrap_or(COLOR_NEUTRAL);
        vec![
            KeyValueRow::new("Receitas", income_fmt).color(COLOR_POSITIVE),
            KeyValueRow::new("Despesas", expenses_fmt).color(COLOR_NEGATIVE),
            KeyValueRow::new("Resultado", result_fmt).color(result_color),
        ]
    }).unwrap_or_default();

    let has_period = data.period.is_some();

    // Pre-compute projection key-value rows
    let projection_rows: Vec<KeyValueRow> = data.projection.as_ref().map(|proj| {
        let current_bal_fmt = format_money(proj.current_balance.as_deref(), Some(&currency));
        let expected_income_fmt = format_money(proj.expected_income.as_deref(), Some(&currency));
        let projected_bal_fmt = format_money(proj.projected_balance.as_deref(), Some(&currency));
        let projected_bal_color = money_color(proj.projected_balance.as_deref()).unwrap_or(COLOR_ACCENT);

        let mut rows = vec![
            KeyValueRow::new("Saldo atual", current_bal_fmt),
            KeyValueRow::new("Receita esperada", expected_income_fmt).color(COLOR_POSITIVE),
        ];

        if let Some(outflows) = &proj.committed_outflows {
            if let Some(ref bills) = outflows.bills {
                rows.push(
                    KeyValueRow::new("  Faturas", format_money(Some(bills.as_str()), Some(&currency)))
                        .color(COLOR_NEGATIVE),
                );
            }
            if let Some(ref inst) = outflows.installments {
                rows.push(
                    KeyValueRow::new("  Parcelas", format_money(Some(inst.as_str()), Some(&currency)))
                        .color(COLOR_NEGATIVE),
                );
            }
            if let Some(ref subs) = outflows.subscriptions {
                rows.push(
                    KeyValueRow::new("  Assinaturas", format_money(Some(subs.as_str()), Some(&currency)))
                        .color(COLOR_NEGATIVE),
                );
            }
            if let Some(ref sched) = outflows.scheduled {
                rows.push(
                    KeyValueRow::new("  Agendados", format_money(Some(sched.as_str()), Some(&currency)))
                        .color(COLOR_NEGATIVE),
                );
            }
        }

        rows.push(
            KeyValueRow::new("Saldo projetado", projected_bal_fmt).color(projected_bal_color),
        );
        rows
    }).unwrap_or_default();

    let has_projection = data.projection.is_some();

    ui.column(Column::new().gap(1), move |col| {
        // Heading
        col.panel(Panel::new().title("Fluxo de Caixa"), move |p| {
            p.styled_text(
                &period_heading,
                TextStyle::new().fg(COLOR_NEUTRAL),
            );
        });

        // Period summary panel
        col.panel(Panel::new().title("Resumo do Período"), move |p| {
            if has_period {
                p.key_values(KeyValueOptions {
                    rows: period_rows.clone(),
                    ..Default::default()
                });
            } else {
                p.label("Sem dados do período.");
            }
        });

        // Projection panel
        col.panel(Panel::new().title("Projeção"), move |p| {
            if has_projection {
                p.key_values(KeyValueOptions {
                    rows: projection_rows.clone(),
                    ..Default::default()
                });
            } else {
                p.label("Sem dados de projeção.");
            }
        });
    });
}