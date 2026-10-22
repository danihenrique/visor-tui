use hqtui::prelude::*;

use crate::api::{ApiClient, ApiError};
use crate::models::Overview;
use crate::presentation::{Metric, TableSelection, Tone, amount, metric_strip};

fn money(value: Option<&str>, currency: &str) -> String {
    crate::presentation::format_currency(value, Some(currency))
}

pub fn render(ui: &mut Container<'_>, api: &ApiClient) {
    render_scrolled(ui, api, &mut TableSelection::default());
}

pub fn render_scrolled(ui: &mut Container<'_>, api: &ApiClient, selection: &mut TableSelection) {
    match api.overview() {
        Ok(data) => render_cash_flow_data(ui, &data, selection),
        Err(error) => {
            selection.set_row_count(0);
            render_error(ui, &error);
        }
    }
}

fn render_error(ui: &mut Container<'_>, error: &ApiError) {
    let message = format!("Falha ao carregar fluxo de caixa: {error}");
    let danger = ui.theme().danger;
    ui.panel(Panel::new().title("Erro"), move |panel| {
        panel.styled_text(&message, TextStyle::new().fg(danger));
        panel.label("Verifique sua conexão e credenciais.");
    });
}

fn render_cash_flow_data(ui: &mut Container<'_>, data: &Overview, selection: &mut TableSelection) {
    let data = data.clone();
    let currency = data.currency_code.as_deref().unwrap_or("BRL");
    let period = data.period.as_ref();
    let metrics = vec![
        Metric::new(
            "Receitas",
            money(period.and_then(|p| p.income.as_deref()), currency),
            Tone::Positive,
        ),
        Metric::new(
            "Despesas",
            money(period.and_then(|p| p.expenses.as_deref()), currency),
            Tone::Negative,
        ),
        Metric::new(
            "Resultado",
            money(period.and_then(|p| p.result.as_deref()), currency),
            result_tone(period.and_then(|p| p.result.as_deref())),
        ),
    ];
    selection.set_row_count(3);
    let selected = selection.selected().unwrap_or_default();
    let full = ui.height() >= 20;
    let dense = ui.height() < 25;
    let wide = ui.width() >= 90;
    let tiles = ui.height() >= 14;
    ui.column(
        Column::new().gap(usize::from(tiles && !dense)),
        move |col| {
            let currency = data.currency_code.clone().unwrap_or_else(|| "BRL".into());
            col.label(&format!(
                "{} a {}",
                data.period_start.as_deref().unwrap_or("Sem início"),
                data.period_end.as_deref().unwrap_or("Sem fim")
            ));
            if tiles {
                metric_strip(col, metrics);
            }
            if full {
                let period = data.period.clone();
                let period_currency = currency.clone();
                col.panel(
                    Panel::new()
                        .title("Movimento do período")
                        .focused(selected == 0)
                        .size(Size::Cells(if dense { 5 } else { 7 })),
                    move |panel| {
                        period_bars(panel, period.as_ref(), &period_currency);
                    },
                );
                let projection = data.projection.clone();
                if wide {
                    col.row(Row::new().gap(1), move |row| {
                        let second = projection.clone();
                        let second_currency = currency.clone();
                        row.panel(
                            Panel::new().title("Próximo saldo").focused(selected == 1),
                            move |panel| projection_bars(panel, projection.as_ref(), &currency),
                        );
                        row.panel(
                            Panel::new()
                                .title("Saídas previstas")
                                .focused(selected == 2),
                            move |panel| outflow_bars(panel, second.as_ref(), &second_currency),
                        );
                    });
                } else {
                    col.panel(
                        Panel::new()
                            .title(if selected == 2 {
                                "Saídas previstas"
                            } else {
                                "Próximo saldo"
                            })
                            .focused(selected != 0),
                        move |panel| {
                            if selected == 2 {
                                outflow_bars(panel, projection.as_ref(), &currency);
                            } else {
                                projection_bars(panel, projection.as_ref(), &currency);
                            }
                        },
                    );
                }
            } else {
                let title = ["Movimento do período", "Próximo saldo", "Saídas previstas"][selected];
                col.panel(Panel::new().title(title), move |panel| match selected {
                    0 => period_bars(panel, data.period.as_ref(), &currency),
                    1 => projection_bars(panel, data.projection.as_ref(), &currency),
                    _ => outflow_bars(panel, data.projection.as_ref(), &currency),
                });
            }
            col.label(&format!("j/k Seções  ·  {}/3", selected + 1));
        },
    );
}

fn result_tone(value: Option<&str>) -> Tone {
    match amount(value) {
        Some(value) if value > 0.0 => Tone::Positive,
        Some(value) if value < 0.0 => Tone::Negative,
        _ => Tone::Neutral,
    }
}

fn period_bars(
    ui: &mut Container<'_>,
    period: Option<&crate::models::OverviewPeriod>,
    currency: &str,
) {
    let Some(period) = period else {
        ui.label("Nenhum movimento disponível neste período.");
        return;
    };
    financial_bars(
        ui,
        &[
            ("Receitas", period.income.as_deref(), Tone::Positive),
            ("Despesas", period.expenses.as_deref(), Tone::Negative),
            (
                "Resultado",
                period.result.as_deref(),
                result_tone(period.result.as_deref()),
            ),
        ],
        currency,
    );
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
                    render_cash_flow_data(ui, &data, &mut selection);
                }));
                selection.move_by(1);
            }
            for text in [
                "Resultado",
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
