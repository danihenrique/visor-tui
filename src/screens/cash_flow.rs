use crate::interaction::{Interactions, Interactive};
use std::sync::Arc;

use hqtui::prelude::*;
use hqtui::unicode::Align;

use crate::models::{Overview, Transaction};
use crate::period::{FlowTotals, day_groups, money_cents, summarize};
use crate::presentation::{TableSelection, Tone, amount};

fn money(value: Option<&str>, currency: &str) -> String {
    crate::presentation::format_currency(value, Some(currency))
}

pub fn render_selected_period(
    ui: &mut Container<'_>,
    overview: &Result<Overview, String>,
    transactions: &Result<Arc<[Transaction]>, String>,
    selection: &mut TableSelection,
) {
    selection.hits.clear();
    let hits = selection.hits.clone();
    let Ok(transactions) = transactions else {
        selection.set_row_count(0);
        if let Err(error) = transactions {
            ui.text(&format!("Movimento do período indisponível: {error}"));
        }
        return;
    };
    let show_scope_dates = ui.height() >= 12;
    let count = transactions.len();
    let overview = overview.clone();
    let summary = summarize(transactions);
    let days = daily_flows(transactions);
    let day_count = days.len().max(1);
    selection.set_row_count(day_count + 2);
    let selected = selection.selected().unwrap_or_default();
    ui.column(Column::new().gap(0), {
        let hits = hits.clone();
        move |col| {
            crate::presentation::period_summary(col, &summary, count);
            if selected < day_count {
                col.panel(Panel::new().title("Fluxo diário do período"), {
                    let hits = hits.clone();
                    move |panel| {
                        daily_flow_table(panel, &days, selected, &hits);
                    }
                });
            } else {
                match overview {
                    Ok(data) => {
                        if show_scope_dates {
                            col.label(&format!(
                                "Previsão atual · {} a {}",
                                data.period_start.as_deref().unwrap_or("Sem início"),
                                data.period_end.as_deref().unwrap_or("Sem fim")
                            ));
                        }
                        let currency = data.currency_code.clone().unwrap_or_else(|| "BRL".into());
                        let outflows = selected == day_count + 1;
                        col.panel(
                            Panel::new().title(if outflows {
                                "Saídas previstas · atual"
                            } else {
                                "Próximo saldo · atual"
                            }),
                            move |panel| {
                                if outflows {
                                    outflow_bars(panel, data.projection.as_ref(), &currency);
                                } else {
                                    projection_bars(panel, data.projection.as_ref(), &currency);
                                }
                            },
                        );
                    }
                    Err(error) => {
                        col.text(&format!("Previsão atual indisponível: {error}"));
                    }
                }
            }
            col.label(&format!(
                "j/k Dias e previsão atual · {}/{}",
                selected + 1,
                day_count + 2
            ));
        }
    });
}

struct DailyFlow {
    date: String,
    currency: String,
    totals: FlowTotals,
}

fn daily_flows(transactions: &[Transaction]) -> Vec<DailyFlow> {
    day_groups(transactions)
        .into_iter()
        .flat_map(|group| {
            let date = group.date.map_or_else(
                || "Sem data válida".into(),
                |day| day.format("%d/%m/%Y").to_string(),
            );
            let mut currencies = group.summary.currencies;
            if currencies.is_empty() {
                currencies.insert("BRL".into(), FlowTotals::default());
            }
            currencies
                .into_iter()
                .map(move |(currency, totals)| DailyFlow {
                    date: date.clone(),
                    currency,
                    totals,
                })
        })
        .collect()
}

fn daily_flow_table(
    ui: &mut Container<'_>,
    days: &[DailyFlow],
    selected: usize,
    hits: &Interactions,
) {
    if days.is_empty() {
        ui.label("Nenhum lançamento no período selecionado.");
        return;
    }
    let rows = days
        .iter()
        .map(|day| {
            let mut row = TableRow::new([
                day.date.clone(),
                money_cents(day.totals.income, &day.currency),
                money_cents(day.totals.expenses, &day.currency),
                money_cents(day.totals.result(), &day.currency),
            ]);
            row.cell_colors = vec![
                Some(ui.theme().muted),
                Some(ui.theme().success),
                Some(ui.theme().danger),
                Some(if day.totals.result() < 0 {
                    ui.theme().danger
                } else {
                    ui.theme().success
                }),
            ];
            row
        })
        .collect();
    ui.mouse_table(
        TableOptions {
            columns: vec![
                TableColumn::new("Dia").width(Size::Cells(10)),
                TableColumn::new("Receitas")
                    .width(Size::Fill)
                    .align(Align::Right),
                TableColumn::new("Despesas")
                    .width(Size::Fill)
                    .align(Align::Right),
                TableColumn::new("Saldo")
                    .width(Size::Fill)
                    .align(Align::Right),
            ],
            rows,
            selected: Some(selected),
            follow_selection: true,
            zebra: true,
            scrollbar: true,
            ..Default::default()
        },
        hits,
    );
    if let Some(day) = days.get(selected) {
        ui.label(&format!(
            "{} · {} lançamentos em {}",
            day.date, day.totals.count, day.currency
        ));
    }
}

fn result_tone(value: Option<&str>) -> Tone {
    match amount(value) {
        Some(value) if value > 0.0 => Tone::Positive,
        Some(value) if value < 0.0 => Tone::Negative,
        _ => Tone::Neutral,
    }
}

fn projection_bars(
    ui: &mut Container<'_>,
    projection: Option<&crate::models::OverviewProjection>,
    currency: &str,
) {
    let Some(projection) = projection else {
        ui.label("Nenhuma projeção disponível.");
        return;
    };
    financial_bars(
        ui,
        &[
            (
                "Saldo atual",
                projection.current_balance.as_deref(),
                result_tone(projection.current_balance.as_deref()),
            ),
            (
                "A receber",
                projection.expected_income.as_deref(),
                Tone::Positive,
            ),
            (
                "Saldo projetado",
                projection.projected_balance.as_deref(),
                result_tone(projection.projected_balance.as_deref()),
            ),
        ],
        currency,
    );
}

fn outflow_bars(
    ui: &mut Container<'_>,
    projection: Option<&crate::models::OverviewProjection>,
    currency: &str,
) {
    let Some(projection) = projection else {
        ui.label("Nenhuma previsão disponível.");
        return;
    };
    let outflows = projection.committed_outflows.as_ref();
    financial_bars(
        ui,
        &[
            (
                "Faturas",
                outflows.and_then(|o| o.bills.as_deref()),
                Tone::Negative,
            ),
            (
                "Parcelas",
                outflows.and_then(|o| o.installments.as_deref()),
                Tone::Negative,
            ),
            (
                "Assinaturas",
                outflows.and_then(|o| o.subscriptions.as_deref()),
                Tone::Negative,
            ),
            (
                "Agendados",
                outflows.and_then(|o| o.scheduled.as_deref()),
                Tone::Negative,
            ),
            (
                "Dia a dia",
                projection.estimated_day_to_day.as_deref(),
                Tone::Warning,
            ),
        ],
        currency,
    );
}

fn financial_bars(ui: &mut Container<'_>, values: &[(&str, Option<&str>, Tone)], currency: &str) {
    let max = values
        .iter()
        .filter_map(|(_, value, _)| amount(*value))
        .map(f64::abs)
        .fold(1.0, f64::max);
    for &(label, raw, tone) in values {
        let text = money(raw, currency);
        if let Some(value) = amount(raw) {
            ui.meter(MeterOptions {
                value: value.abs(),
                max: Some(max),
                label: Some(label.to_string()),
                label_width: Some(16),
                text: Some(text),
                value_width: Some(18),
                color: Some(tone.color(ui.theme())),
                heat: Some(false),
                ..Default::default()
            });
        } else {
            ui.label(&format!("{label}  ·  Indisponível"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hqtui::testing::render_to_text;

    #[test]
    fn daily_flow_uses_selected_rows_and_keeps_current_forecast_accessible() {
        let overview = Ok(serde_json::from_value(serde_json::json!({
            "currency_code":"BRL", "period_start":"2026-10-01", "period_end":"2026-10-31",
            "period":{"income":"9999.99", "expenses":"7777.77", "result":"2222.22"},
            "projection":{"current_balance":"3800", "expected_income":"500", "projected_balance":"3930",
                "estimated_day_to_day":"90", "committed_outflows":{"scheduled":"40"}}
        })).unwrap());
        let rows: Arc<[Transaction]> = serde_json::from_value::<Vec<Transaction>>(serde_json::json!([
            {"id":"one", "account_id":"a", "type":"credit", "amount":"0.10", "currency_code":"BRL", "transacted_at":"2026-09-01"},
            {"id":"two", "account_id":"a", "type":"credit", "amount":"0.20", "currency_code":"BRL", "transacted_at":"2026-09-01"},
            {"id":"three", "account_id":"a", "type":"debit", "amount":"9.50", "currency_code":"USD", "transacted_at":"2026-09-01"},
            {"id":"four", "account_id":"a", "type":"debit", "amount":"0.15", "currency_code":"BRL", "transacted_at":"2026-09-02"}
        ])).unwrap().into();
        let rows = Ok(rows);
        for (width, height) in [(76, 17), (92, 32)] {
            let mut selection = TableSelection::default();
            let mut seen = String::new();
            for _ in 0..5 {
                let rendered = render_to_text(width, height, "dark", |ui| {
                    render_selected_period(ui, &overview, &rows, &mut selection);
                });
                assert!(
                    rendered.contains("0,30 BRL"),
                    "{width}x{height}\n{rendered}"
                );
                assert!(rendered.contains("0,15 BRL"), "{rendered}");
                assert!(!rendered.contains("9.999,99"), "{rendered}");
                seen.push_str(&rendered);
                selection.move_by(1);
            }
            for expected in [
                "01/09/2026",
                "02/09/2026",
                "-9,50 USD",
                "Previsão atual",
                "2026-10-01",
                "2026-10-31",
                "Saldo projetado",
                "3.930,00 BRL",
                "Agendados",
                "Dia a dia",
            ] {
                assert!(
                    seen.contains(expected),
                    "{width}x{height}: missing {expected}\n{seen}"
                );
            }
            assert_eq!(selection.selected(), Some(4));
        }
    }

    #[test]
    fn empty_and_failed_periods_never_fall_back_to_current_movement() {
        let overview = Ok(serde_json::from_value(serde_json::json!({
            "period":{"income":"9999.99", "expenses":"7777.77", "result":"2222.22"}
        }))
        .unwrap());
        for (rows, expected) in [
            (Ok(Arc::<[Transaction]>::from([])), "Nenhum lançamento"),
            (
                Err("Falha de leitura".into()),
                "Movimento do período indisponível",
            ),
        ] {
            let rendered = render_to_text(76, 17, "dark", |ui| {
                render_selected_period(ui, &overview, &rows, &mut TableSelection::default());
            });
            assert!(rendered.contains(expected), "{rendered}");
            assert!(!rendered.contains("9.999,99"), "{rendered}");
        }
    }

    #[test]
    fn every_cash_flow_section_is_reachable_without_flattening_records() {
        let data: Overview = serde_json::from_value(serde_json::json!({
            "currency_code":"BRL", "period_start":"2026-10-01", "period_end":"2026-10-31",
            "period":{"income":"5000", "expenses":"1200", "result":"3800"},
            "projection":{"current_balance":"3800", "expected_income":"500", "projected_balance":"3930",
                "estimated_day_to_day":"90", "committed_outflows":{"bills":"100", "installments":"200", "subscriptions":"30", "scheduled":"40"}}
        })).unwrap();
        for (width, height) in [(56, 10), (60, 18), (80, 24)] {
            let mut selection = TableSelection::default();
            let mut seen = String::new();
            for _ in 0..3 {
                seen.push_str(&render_to_text(width, height, "dark", |ui| {
                    let transactions = serde_json::from_value::<Vec<Transaction>>(serde_json::json!([
                        {"id":"income", "account_id":"a", "type":"credit", "amount":"5000", "currency_code":"BRL", "transacted_at":"2026-10-01"},
                        {"id":"expense", "account_id":"a", "type":"debit", "amount":"1200", "currency_code":"BRL", "transacted_at":"2026-10-01"}
                    ])).unwrap().into();
                    render_selected_period(ui, &Ok(data.clone()), &Ok(transactions), &mut selection);
                }));
                selection.move_by(1);
            }
            for text in [
                "Saldo do período",
                "3.800,00 BRL",
                "Saldo projetado",
                "3.930,00 BRL",
                "Agendados",
                "Dia a dia",
            ] {
                assert!(
                    seen.contains(text),
                    "{width}x{height}: missing {text}\n{seen}"
                );
            }
            assert_eq!(selection.selected(), Some(2));
            assert!(!seen.contains("Campo"));
        }
    }
}
