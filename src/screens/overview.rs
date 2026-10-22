use hqtui::prelude::*;
use hqtui::unicode::Align;

use crate::api::ApiClient;
use crate::models::Overview;
use crate::presentation::{Metric, TableSelection, Tone, amount, format_currency, metric_strip};

pub fn draw_overview(ui: &mut Container<'_>, api: &ApiClient) {
    draw_overview_scrolled(ui, api, &mut TableSelection::default());
}

pub fn draw_overview_scrolled(
    ui: &mut Container<'_>,
    api: &ApiClient,
    selection: &mut TableSelection,
) {
    match api.overview() {
        Ok(data) => draw_overview_data(ui, &data, selection),
        Err(error) => {
            selection.set_row_count(0);
            ui.text(&format!(
                "Falha ao carregar visão geral: {error}. Pressione r para tentar novamente."
            ));
        }
    }
}

fn draw_overview_data(ui: &mut Container<'_>, data: &Overview, selection: &mut TableSelection) {
    let data = data.clone();
    let sections = 4 + data.upcoming_bills.len().max(1);
    selection.set_row_count(sections);
    let selected = selection.selected().unwrap_or_default();
    let full = ui.height() >= 28;
    let period_preview = ui.height() >= 19;
    let tiles = ui.height() >= 14;
    let currency = data.currency_code.clone().unwrap_or_else(|| "BRL".into());
    let snapshot = snapshot_metrics(&data, &currency);
    ui.column(Column::new().gap(usize::from(tiles)), move |col| {
        col.label(&format!(
            "{} a {}",
            data.period_start.as_deref().unwrap_or("Sem início"),
            data.period_end.as_deref().unwrap_or("Sem fim")
        ));
        if full {
            metric_strip(col, snapshot);
            let period = data.period.clone();
            let period_currency = currency.clone();
            let projection = data.projection.clone();
            let projection_currency = currency.clone();
            col.row(Row::new().size(Size::Cells(8)).gap(1), move |row| {
                row.panel(
                    Panel::new()
                        .title("Movimento do período")
                        .focused(selected == 1),
                    move |panel| overview_period(panel, period.as_ref(), &period_currency),
                );
                row.panel(
                    Panel::new().title("Próximo saldo").focused(selected == 2),
                    move |panel| {
                        overview_projection(panel, projection.as_ref(), &projection_currency);
                    },
                );
            });
            let projection = data.projection.clone();
            let outflow_currency = currency.clone();
            col.row(Row::new().gap(1), move |row| {
                row.panel(
                    Panel::new()
                        .title("Saídas previstas")
                        .focused(selected == 3),
                    move |panel| overview_outflows(panel, projection.as_ref(), &outflow_currency),
                );
                row.panel(
                    Panel::new()
                        .title("Próximas faturas")
                        .focused(selected >= 4),
                    move |panel| {
                        upcoming_bills(
                            panel,
                            &data.upcoming_bills,
                            &currency,
                            selected.saturating_sub(4),
                        );
                    },
                );
            });
        } else {
            match selected {
                0 => {
                    let mut snapshot = snapshot.into_iter();
                    metric_strip(col, snapshot.by_ref().take(2).collect());
                    metric_strip(col, snapshot.collect());
                    if period_preview {
                        col.panel(Panel::new().title("Movimento do período"), move |panel| {
                            overview_period(panel, data.period.as_ref(), &currency);
                        });
                    }
                }
                1 => {
                    col.panel(Panel::new().title("Movimento do período"), move |panel| {
                        overview_period(panel, data.period.as_ref(), &currency);
                    });
                }
                2 => {
                    col.panel(Panel::new().title("Próximo saldo"), move |panel| {
                        overview_projection(panel, data.projection.as_ref(), &currency);
                    });
                }
                3 => {
                    col.panel(Panel::new().title("Saídas previstas"), move |panel| {
                        overview_outflows(panel, data.projection.as_ref(), &currency);
                    });
                }
                _ => {
                    col.panel(Panel::new().title("Próximas faturas"), move |panel| {
                        upcoming_bills(panel, &data.upcoming_bills, &currency, selected - 4);
                    });
                }
            }
        }
        col.label(&format!(
            "j/k Resumo e faturas  ·  {}/{sections}",
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

fn overview_period(
    ui: &mut Container<'_>,
    period: Option<&crate::models::OverviewPeriod>,
    currency: &str,
) {
    let Some(period) = period else {
        ui.label("Nenhum movimento disponível neste período.");
        return;
    };
    overview_bars(
        ui,
        &[
            ("Receitas", period.income.as_deref(), Tone::Positive),
            ("Despesas", period.expenses.as_deref(), Tone::Negative),
            (
                "Resultado",
                period.result.as_deref(),
                balance_tone(period.result.as_deref()),
            ),
        ],
        currency,
    );
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
#[allow(dead_code)]
pub(crate) fn draw_overview_mock(ui: &mut Container<'_>, data: &Overview) {
    draw_overview_data(ui, data, &mut TableSelection::default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use hqtui::testing::render_to_text;

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
                    draw_overview_data(ui, &data, &mut selection);
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
            assert_eq!(selection.selected(), Some(5));
            assert!(!seen.contains("Campo"));
        }
    }
}
