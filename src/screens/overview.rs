use std::sync::Arc;

use hqtui::prelude::*;
use hqtui::unicode::Align;

use crate::models::{Overview, Transaction};
use crate::period::summarize;
use crate::presentation::{Metric, TableSelection, Tone, amount, format_currency, metric_strip};

pub fn draw_selected_period(
    ui: &mut Container<'_>,
    overview: &Result<Overview, String>,
    transactions: &Result<Arc<[Transaction]>, String>,
    selection: &mut TableSelection,
) {
    selection.set_row_count(
        overview
            .as_ref()
            .map_or(0, |data| 3 + data.upcoming_bills.len().max(1)),
    );
    let selected = selection.selected().unwrap_or_default();
    let overview = overview.clone();
    let transactions = transactions.clone();
    ui.column(Column::new().gap(0), move |col| {
        match transactions {
            Ok(rows) => crate::presentation::period_summary(col, &summarize(&rows), rows.len()),
            Err(error) => {
                col.text(&format!("Movimento do período indisponível: {error}"));
            }
        }
        match overview {
            Ok(data) => draw_current_snapshot(col, &data, selected),
            Err(error) => {
                col.text(&format!("Posição atual indisponível: {error}"));
            }
        }
    });
}

fn draw_current_snapshot(ui: &mut Container<'_>, data: &Overview, selected: usize) {
    let sections = 3 + data.upcoming_bills.len().max(1);
    let data = data.clone();
    let full = ui.height() >= 24;
    ui.column(Column::new().gap(0), move |col| {
        let currency = data.currency_code.clone().unwrap_or_else(|| "BRL".into());
        col.label("Posição atual das contas");
        col.label(&format!(
            "Previsão atual · {} a {}",
            data.period_start.as_deref().unwrap_or("Sem início"),
            data.period_end.as_deref().unwrap_or("Sem fim")
        ));
        if full {
            metric_strip(col, snapshot_metrics(&data, &currency));
            let projection = data.projection.clone();
            let projection_currency = currency.clone();
            col.row(Row::new().size(Size::Cells(8)).gap(1), move |row| {
                let outflows = projection.clone();
                let outflow_currency = projection_currency.clone();
                row.panel(
                    Panel::new().title("Próximo saldo").focused(selected == 1),
                    move |panel| {
                        overview_projection(panel, projection.as_ref(), &projection_currency);
                    },
                );
                row.panel(
                    Panel::new()
                        .title("Saídas previstas")
                        .focused(selected == 2),
                    move |panel| overview_outflows(panel, outflows.as_ref(), &outflow_currency),
                );
            });
            col.panel(
                Panel::new()
                    .title("Próximas faturas")
                    .focused(selected >= 3),
                move |panel| {
                    upcoming_bills(
                        panel,
                        &data.upcoming_bills,
                        &currency,
                        selected.saturating_sub(3),
                    );
                },
            );
        } else {
            match selected {
                0 => {
                    let mut metrics = snapshot_metrics(&data, &currency).into_iter();
                    metric_strip(col, metrics.by_ref().take(2).collect());
                    metric_strip(col, metrics.collect());
                }
                1 => {
                    col.panel(Panel::new().title("Próximo saldo"), move |panel| {
                        overview_projection(panel, data.projection.as_ref(), &currency);
                    });
                }
                2 => {
                    col.panel(Panel::new().title("Saídas previstas"), move |panel| {
                        overview_outflows(panel, data.projection.as_ref(), &currency);
                    });
                }
                _ => {
                    col.panel(Panel::new().title("Próximas faturas"), move |panel| {
                        upcoming_bills(panel, &data.upcoming_bills, &currency, selected - 3);
                    });
                }
            }
        }
        col.label(&format!(
            "j/k Posição atual e faturas · {}/{sections}",
            selected + 1
        ));
    });
}

fn snapshot_metrics(data: &Overview, currency: &str) -> Vec<Metric> {
    [
        (
            "Saldo em conta",
            data.bank_balance.as_deref(),
            balance_tone(data.bank_balance.as_deref()),
        ),
        (
            "Patrim. líquido",
            data.net_worth.as_deref(),
            balance_tone(data.net_worth.as_deref()),
        ),
        (
            "Dívida cartão",
            data.credit_card_debt.as_deref(),
            Tone::Negative,
        ),
        (
            "Faturas abertas",
            data.open_bills_total.as_deref(),
            Tone::Warning,
        ),
    ]
    .into_iter()
    .map(|(label, value, tone)| Metric::new(label, format_currency(value, Some(currency)), tone))
    .collect()
}

fn balance_tone(raw: Option<&str>) -> Tone {
    match amount(raw) {
        Some(value) if value > 0.0 => Tone::Positive,
        Some(value) if value < 0.0 => Tone::Negative,
        _ => Tone::Neutral,
    }
}

fn overview_projection(
    ui: &mut Container<'_>,
    projection: Option<&crate::models::OverviewProjection>,
    currency: &str,
) {
    let Some(projection) = projection else {
        ui.label("Nenhuma projeção disponível.");
        return;
    };
    overview_bars(
        ui,
        &[
            (
                "Saldo atual",
                projection.current_balance.as_deref(),
                balance_tone(projection.current_balance.as_deref()),
            ),
            (
                "Receita esperada",
                projection.expected_income.as_deref(),
                Tone::Positive,
            ),
            (
                "Saldo projetado",
                projection.projected_balance.as_deref(),
                balance_tone(projection.projected_balance.as_deref()),
            ),
        ],
        currency,
    );
}

fn overview_outflows(
    ui: &mut Container<'_>,
    projection: Option<&crate::models::OverviewProjection>,
    currency: &str,
) {
    let Some(projection) = projection else {
        ui.label("Nenhuma previsão disponível.");
        return;
    };
    let committed = projection.committed_outflows.as_ref();
    overview_bars(
        ui,
        &[
            (
                "Faturas",
                committed.and_then(|o| o.bills.as_deref()),
                Tone::Negative,
            ),
            (
                "Parcelas",
                committed.and_then(|o| o.installments.as_deref()),
                Tone::Negative,
            ),
            (
                "Assinaturas",
                committed.and_then(|o| o.subscriptions.as_deref()),
                Tone::Negative,
            ),
            (
                "Agendados",
                committed.and_then(|o| o.scheduled.as_deref()),
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

fn overview_bars(ui: &mut Container<'_>, values: &[(&str, Option<&str>, Tone)], currency: &str) {
    let max = values
        .iter()
        .filter_map(|(_, value, _)| amount(*value))
        .map(f64::abs)
        .fold(1.0, f64::max);
    for &(label, raw, tone) in values {
        if let Some(value) = amount(raw) {
            ui.meter(MeterOptions {
                value: value.abs(),
                max: Some(max),
                label: Some(label.into()),
                label_width: Some(16),
                text: Some(format_currency(raw, Some(currency))),
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

fn upcoming_bills(
    ui: &mut Container<'_>,
    bills: &[crate::models::OverviewUpcomingBill],
    currency: &str,
    selected: usize,
) {
    if bills.is_empty() {
        ui.label("Nenhuma fatura prevista para este período.");
        return;
    }
    let rows = bills
        .iter()
        .map(|bill| {
            let estimate = if bill.amount_is_estimate { "≈ " } else { "" };
            let mut row = TableRow::new([
                bill.due_date.clone().unwrap_or_else(|| "Sem data".into()),
                bill.description.clone().unwrap_or_else(|| "Fatura".into()),
                format!(
                    "{estimate}{}",
                    format_currency(bill.amount.as_deref(), Some(currency))
                ),
            ]);
            row.cell_colors = vec![Some(ui.theme().muted), None, Some(ui.theme().warning)];
            row
        })
        .collect();
    ui.table(
        TableOptions {
            columns: vec![
                TableColumn::new("Vencimento").width(Size::Cells(10)),
                TableColumn::new("Descrição").width(Size::Fill),
                TableColumn::new("Valor")
                    .width(Size::Cells(18))
                    .align(Align::Right),
            ],
            rows,
            selected: Some(selected),
            follow_selection: true,
            zebra: true,
            scrollbar: true,
            ..Default::default()
        },
        "overview:upcoming",
    );
    if let Some(bill) = bills.get(selected) {
        ui.label(bill.description.as_deref().unwrap_or("Fatura"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hqtui::testing::render_to_text;

    #[test]
    fn selected_movement_stays_visible_with_current_position_and_forecast() {
        let overview = Ok(serde_json::from_value(serde_json::json!({
            "currency_code":"BRL", "period_start":"2026-10-01", "period_end":"2026-10-31",
            "bank_balance":"12345.67", "net_worth":"22000", "credit_card_debt":"2000", "open_bills_total":"500",
            "period":{"income":"9999.99", "expenses":"7777.77", "result":"2222.22"},
            "projection":{"current_balance":"1200", "expected_income":"300", "projected_balance":"9876.54", "estimated_day_to_day":"90", "committed_outflows":{"scheduled":"80"}},
            "upcoming_bills":[{"description":"Última fatura", "amount":"222", "due_date":"2026-11-15"}]
        })).unwrap());
        let rows: Arc<[Transaction]> = serde_json::from_value::<Vec<Transaction>>(serde_json::json!([
            {"id":"one", "account_id":"a", "type":"credit", "amount":"0.10", "currency_code":"BRL", "transacted_at":"2026-09-01"},
            {"id":"two", "account_id":"a", "type":"credit", "amount":"0.20", "currency_code":"BRL", "transacted_at":"2026-09-01"},
            {"id":"three", "account_id":"a", "type":"debit", "amount":"9.50", "currency_code":"USD", "transacted_at":"2026-09-01"}
        ])).unwrap().into();
        let rows = Ok(rows);
        for (width, height) in [(76, 17), (92, 32)] {
            let mut selection = TableSelection::default();
            let mut seen = String::new();
            for _ in 0..4 {
                let rendered = render_to_text(width, height, "dark", |ui| {
                    draw_selected_period(ui, &overview, &rows, &mut selection);
                });
                assert!(
                    rendered.contains("0,30 BRL"),
                    "{width}x{height}\n{rendered}"
                );
                assert!(rendered.contains("USD original"), "{rendered}");
                assert!(!rendered.contains("9.999,99"), "{rendered}");
                seen.push_str(&rendered);
                selection.move_by(1);
            }
            for expected in [
                "Posição atual",
                "2026-10-01",
                "2026-10-31",
                "12.345,67 BRL",
                "Saldo projetado",
                "9.876,54 BRL",
                "Agendados",
                "Última fatura",
            ] {
                assert!(
                    seen.contains(expected),
                    "{width}x{height}: missing {expected}\n{seen}"
                );
            }
            assert_eq!(selection.selected(), Some(3));
        }
    }

    #[test]
    fn selected_period_failure_does_not_display_current_api_movement() {
        let overview = Ok(serde_json::from_value(serde_json::json!({
            "period":{"income":"9999.99", "expenses":"7777.77", "result":"2222.22"}
        }))
        .unwrap());
        let rows = Err("Falha de leitura".into());
        let rendered = render_to_text(76, 17, "dark", |ui| {
            draw_selected_period(ui, &overview, &rows, &mut TableSelection::default());
        });
        assert!(
            rendered.contains("Movimento do período indisponível"),
            "{rendered}"
        );
        assert!(!rendered.contains("9.999,99"), "{rendered}");
    }

    #[test]
    fn overview_sections_and_last_bill_remain_reachable() {
        let data: Overview = serde_json::from_value(serde_json::json!({
            "bank_balance":"12345.67", "net_worth":"22000", "credit_card_debt":"2000", "open_bills_total":"500",
            "period":{"income":"5000", "expenses":"1200", "result":"3800"},
            "projection":{"current_balance":"1200", "expected_income":"300", "projected_balance":"9876.54", "estimated_day_to_day":"90", "committed_outflows":{"bills":"50", "installments":"60", "subscriptions":"70", "scheduled":"80"}},
            "upcoming_bills":[{"description":"Primeira fatura", "amount":"111", "due_date":"2026-11-01"},{"description":"Última fatura", "amount":"222", "due_date":"2026-11-15", "amount_is_estimate":true}]
        })).unwrap();
        for (width, height) in [(56, 10), (60, 18), (80, 24)] {
            let mut selection = TableSelection::default();
            let mut seen = String::new();
            for _ in 0..6 {
                seen.push_str(&render_to_text(width, height, "dark", |ui| {
                    let sections = 3 + data.upcoming_bills.len().max(1);
                    selection.set_row_count(sections);
                    draw_current_snapshot(ui, &data, selection.selected().unwrap_or_default());
                }));
                selection.move_by(1);
            }
            for expected in [
                "12.345,67 BRL",
                "Saldo projetado",
                "9.876,54 BRL",
                "Agendados",
                "Dia a dia",
                "Última fatura",
                "≈ 222,00 BRL",
            ] {
                assert!(
                    seen.contains(expected),
                    "{width}x{height}: missing {expected}\n{seen}"
                );
            }
            assert_eq!(selection.selected(), Some(4));
            assert!(!seen.contains("Campo"));
        }
    }
}
