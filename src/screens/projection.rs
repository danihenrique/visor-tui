use hqtui::prelude::*;
use hqtui::widgets::{TableColumn, TableOptions, TableRow};

use crate::api::ApiClient;
use crate::models::OverviewProjection;

fn money(value: Option<&str>, currency: &str) -> String {
    crate::presentation::format_currency(value, Some(currency))
}

pub fn render(ui: &mut Container<'_>, api: &ApiClient) {
    render_scrolled(ui, api, 0);
}

pub fn render_scrolled(ui: &mut Container<'_>, api: &ApiClient, selected: usize) {
    match api.overview() {
        Ok(overview) => {
            if let Some(projection) = &overview.projection {
                render_projection_data(
                    ui,
                    projection,
                    overview.currency_code.as_deref().unwrap_or("BRL"),
                    selected,
                );
            } else {
                ui.panel(Panel::new().title("Projeção"), |panel| {
                    panel.text("Sem dados de projeção");
                });
            }
        }
        Err(error) => {
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
    selected: usize,
) {
    let theme = ui.theme().clone();
    let mut rows = vec![
        TableRow::new([
            "Saldo atual",
            &money(projection.current_balance.as_deref(), currency),
        ])
        .color(amount_color(projection.current_balance.as_deref(), &theme)),
        TableRow::new([
            "Receita esperada",
            &money(projection.expected_income.as_deref(), currency),
        ])
        .color(theme.success),
        TableRow::new(["SAÍDAS COMPROMETIDAS", ""]).color(theme.accent),
    ];
    let outflows = projection.committed_outflows.as_ref();
    for (label, value) in [
        (
            "Contas",
            outflows.and_then(|outflows| outflows.bills.as_deref()),
        ),
        (
            "Parcelas",
            outflows.and_then(|outflows| outflows.installments.as_deref()),
        ),
        (
            "Assinaturas",
            outflows.and_then(|outflows| outflows.subscriptions.as_deref()),
        ),
        (
            "Agendadas",
            outflows.and_then(|outflows| outflows.scheduled.as_deref()),
        ),
    ] {
        rows.push(TableRow::new([label, &money(value, currency)]).color(theme.danger));
    }
    rows.push(
        TableRow::new([
            "Dia a dia estimado",
            &money(projection.estimated_day_to_day.as_deref(), currency),
        ])
        .color(amount_color(
            projection.estimated_day_to_day.as_deref(),
            &theme,
        )),
    );
    rows.push(
        TableRow::new([
            "Saldo projetado",
            &money(projection.projected_balance.as_deref(), currency),
        ])
        .color(amount_color(
            projection.projected_balance.as_deref(),
            &theme,
        )),
    );

    ui.panel(Panel::new().title("Projeção"), move |panel| {
        panel.table(
            TableOptions {
                columns: vec![
                    TableColumn::new("Campo").width(Size::Cells(22)),
                    TableColumn::new("Valor").width(Size::Fill),
                ],
                selected: Some(selected.min(rows.len().saturating_sub(1))),
                rows,
                header: false,
                follow_selection: true,
                scrollbar: true,
                zebra: true,
                ..Default::default()
            },
            "projection_facts",
        );
    });
}

fn amount_color(value: Option<&str>, theme: &Theme) -> Color {
    match value.and_then(|value| value.parse::<f64>().ok()) {
        Some(amount) if amount >= 0.0 => theme.success,
        Some(_) => theme.danger,
        None => theme.foreground,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hqtui::testing::render_to_text;

    #[test]
    fn outflows_and_final_balance_remain_reachable() {
        let projection: OverviewProjection = serde_json::from_value(serde_json::json!({
            "current_balance": "1200", "expected_income": "300",
            "committed_outflows": {"bills": "50", "installments": "60", "subscriptions": "70", "scheduled": "80"},
            "estimated_day_to_day": "-90", "projected_balance": "1150"
        })).unwrap();
        for (width, height) in [(60, 18), (80, 24)] {
            let first = render_to_text(width, height, "dark", |ui| {
                crate::presentation::shell(
                    ui,
                    crate::keymap::Screen::Projection,
                    "Pessoal".into(),
                    "Ok".into(),
                    false,
                    |content| render_projection_data(content, &projection, "BRL", 0),
                );
            });
            assert!(
                first.contains("Contas") && first.contains("50,00 BRL"),
                "{width}x{height}\n{first}"
            );
            let last = render_to_text(width, height, "dark", |ui| {
                crate::presentation::shell(
                    ui,
                    crate::keymap::Screen::Projection,
                    "Pessoal".into(),
                    "Ok".into(),
                    false,
                    |content| render_projection_data(content, &projection, "BRL", usize::MAX),
                );
            });
            assert!(
                last.contains("Saldo projetado") && last.contains("1.150,00 BRL"),
                "{width}x{height}\n{last}"
            );
        }
    }
}
