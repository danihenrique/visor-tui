use hqtui::prelude::*;

use crate::api::ApiClient;
use crate::models::OverviewProjection;
use crate::presentation::{Metric, TableSelection, Tone, amount, metric_strip, money_total};

fn money(value: Option<&str>, currency: &str) -> String {
    crate::presentation::format_currency(value, Some(currency))
}

pub fn render(ui: &mut Container<'_>, api: &ApiClient) {
    render_scrolled(ui, api, &mut TableSelection::default());
}

pub fn render_scrolled(ui: &mut Container<'_>, api: &ApiClient, selection: &mut TableSelection) {
    match api.overview() {
        Ok(overview) => {
            if let Some(projection) = &overview.projection {
                render_projection_data(
                    ui,
                    projection,
                    overview.currency_code.as_deref().unwrap_or("BRL"),
                    selection,
                );
            } else {
                selection.set_row_count(0);
                ui.panel(Panel::new().title("Projeção"), |panel| {
                    panel.text("Sem dados de projeção");
                });
            }
        }
        Err(error) => {
            selection.set_row_count(0);
            let message = format!("Erro ao carregar dados: {error}");
            let danger = ui.theme().danger;
            ui.panel(Panel::new().title("Erro"), move |panel| {
                panel.styled_text(&message, TextStyle::new().fg(danger));
            });
        }
    }
}

fn render_projection_data(
    ui: &mut Container<'_>,
    projection: &OverviewProjection,
    currency: &str,
    selection: &mut TableSelection,
) {
    let projection = projection.clone();
    let currency = currency.to_string();
    let outflows = projected_outflows(&projection);
    let metrics = vec![
        Metric::new(
            "Saldo atual",
            money(projection.current_balance.as_deref(), &currency),
            balance_tone(projection.current_balance.as_deref()),
        ),
        Metric::new(
            "Saídas previstas",
            money_total(
                outflows
                    .iter()
                    .map(|(_, value)| (*value, Some(currency.as_str()))),
            ),
            Tone::Negative,
        ),
        Metric::new(
            "Saldo projetado",
            money(projection.projected_balance.as_deref(), &currency),
            balance_tone(projection.projected_balance.as_deref()),
        ),
    ];
    selection.set_row_count(2);
    let selected = selection.selected().unwrap_or_default();
    let full = ui.height() >= 20;
    let dense = ui.height() < 25;
    let tiles = ui.height() >= 14;
    ui.column(
        Column::new().gap(usize::from(tiles && !dense)),
        move |col| {
            if tiles {
                metric_strip(col, metrics);
            }
            if full {
                let comparison = projection.clone();
                let comparison_currency = currency.clone();
                col.panel(
                    Panel::new()
                        .title("Do saldo atual ao saldo previsto")
                        .focused(selected == 0)
                        .size(Size::Cells(if dense { 7 } else { 9 })),
                    move |panel| {
                        balance_comparison(panel, &comparison, &comparison_currency);
                        panel.label("Saldo atual + receitas esperadas − saídas previstas");
                    },
                );
                col.panel(
                    Panel::new()
                        .title("Compromissos e estimativas")
                        .focused(selected == 1),
                    move |panel| {
                        outflow_breakdown(panel, &projection, &currency);
                    },
                );
            } else {
                let title = [
                    "Do saldo atual ao saldo previsto",
                    "Compromissos e estimativas",
                ][selected];
                col.panel(Panel::new().title(title), move |panel| {
                    if selected == 0 {
                        balance_comparison(panel, &projection, &currency);
                    } else {
                        outflow_breakdown(panel, &projection, &currency);
                    }
                });
            }
            col.label(&format!("j/k Seções  ·  {}/2", selected + 1));
        },
    );
}

fn projected_outflows(projection: &OverviewProjection) -> [(&'static str, Option<&str>); 5] {
    let committed = projection.committed_outflows.as_ref();
    [
        ("Faturas", committed.and_then(|o| o.bills.as_deref())),
        (
            "Parcelas",
            committed.and_then(|o| o.installments.as_deref()),
        ),
        (
            "Assinaturas",
            committed.and_then(|o| o.subscriptions.as_deref()),
        ),
        ("Agendados", committed.and_then(|o| o.scheduled.as_deref())),
        (
            "Dia a dia estimado",
            projection.estimated_day_to_day.as_deref(),
        ),
    ]
}

fn balance_tone(value: Option<&str>) -> Tone {
    match amount(value) {
        Some(value) if value > 0.0 => Tone::Positive,
        Some(value) if value < 0.0 => Tone::Negative,
        _ => Tone::Neutral,
    }
}

fn balance_comparison(ui: &mut Container<'_>, projection: &OverviewProjection, currency: &str) {
    let values = [
        ("Saldo atual", projection.current_balance.as_deref()),
        ("Receita esperada", projection.expected_income.as_deref()),
        ("Saldo projetado", projection.projected_balance.as_deref()),
    ];
    let max = values
        .iter()
        .filter_map(|(_, value)| amount(*value))
        .map(f64::abs)
        .fold(1.0, f64::max);
    for (label, value) in values {
        projection_meter(ui, label, value, currency, max, balance_tone(value));
    }
}

fn outflow_breakdown(ui: &mut Container<'_>, projection: &OverviewProjection, currency: &str) {
    let outflows = projected_outflows(projection);
    let max = outflows
        .iter()
        .filter_map(|(_, value)| amount(*value))
        .map(f64::abs)
        .fold(1.0, f64::max);
    for (label, value) in outflows {
        let tone = if label == "Dia a dia estimado" {
            Tone::Warning
        } else {
            Tone::Negative
        };
        projection_meter(ui, label, value, currency, max, tone);
    }
}

fn projection_meter(
    ui: &mut Container<'_>,
    label: &str,
    raw: Option<&str>,
    currency: &str,
    max: f64,
    tone: Tone,
) {
    if let Some(value) = amount(raw) {
        ui.meter(MeterOptions {
            value: value.abs(),
            max: Some(max),
            label: Some(label.into()),
            label_width: Some(19),
            text: Some(money(raw, currency)),
            value_width: Some(18),
            color: Some(tone.color(ui.theme())),
            heat: Some(false),
            ..Default::default()
        });
    } else {
        ui.label(&format!("{label}  ·  Indisponível"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hqtui::testing::render_to_text;

    #[test]
    fn all_projection_components_are_reachable_in_short_terminals() {
        let projection: OverviewProjection = serde_json::from_value(serde_json::json!({
            "current_balance":"1200", "expected_income":"300",
            "committed_outflows":{"bills":"50", "installments":"60", "subscriptions":"70", "scheduled":"80"},
            "estimated_day_to_day":"90", "projected_balance":"1150"
        })).unwrap();
        for (width, height) in [(56, 10), (60, 18), (80, 24)] {
            let mut selection = TableSelection::default();
            let first = render_to_text(width, height, "dark", |ui| {
                render_projection_data(ui, &projection, "BRL", &mut selection);
            });
            selection.move_by(1);
            let last = render_to_text(width, height, "dark", |ui| {
                render_projection_data(ui, &projection, "BRL", &mut selection);
            });
            assert!(
                first.contains("Saldo projetado") && first.contains("1.150,00 BRL"),
                "{first}"
            );
            for text in [
                "Faturas",
                "50,00 BRL",
                "Agendados",
                "Dia a dia estimado",
                "90,00 BRL",
            ] {
                assert!(
                    last.contains(text),
                    "{width}x{height}: missing {text}\n{last}"
                );
            }
            assert_eq!(selection.selected(), Some(1));
        }
    }

    #[test]
    fn absent_components_are_unknown_instead_of_zero() {
        let projection: OverviewProjection =
            serde_json::from_value(serde_json::json!({"projected_balance":"-10"})).unwrap();
        let mut selection = TableSelection::default();
        let text = render_to_text(80, 24, "dark", |ui| {
            render_projection_data(ui, &projection, "BRL", &mut selection);
        });
        assert!(text.contains("Indisponível"), "{text}");
        assert!(text.contains("-10,00 BRL"), "{text}");
    }
}
