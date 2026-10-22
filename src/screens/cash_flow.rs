use hqtui::prelude::*;
use hqtui::widgets::{TableColumn, TableOptions, TableRow};

use crate::api::{ApiClient, ApiError};
use crate::models::Overview;

fn money(value: Option<&str>, currency: &str) -> String {
    crate::presentation::format_currency(value, Some(currency))
}

pub fn render(ui: &mut Container<'_>, api: &ApiClient) {
    render_scrolled(ui, api, 0);
}

pub fn render_scrolled(ui: &mut Container<'_>, api: &ApiClient, selected: usize) {
    match api.overview() {
        Ok(data) => render_cash_flow_data(ui, &data, selected),
        Err(error) => render_error(ui, &error),
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

fn render_cash_flow_data(ui: &mut Container<'_>, data: &Overview, selected: usize) {
    let currency = data.currency_code.as_deref().unwrap_or("BRL");
    let theme = ui.theme().clone();
    let mut rows = Vec::new();

    if let Some(start) = &data.period_start {
        rows.push(TableRow::new(["Início", start]));
    }
    if let Some(end) = &data.period_end {
        rows.push(TableRow::new(["Fim", end]));
    }

    rows.push(TableRow::new(["RESUMO DO PERÍODO", ""]).color(theme.accent));
    if let Some(period) = &data.period {
        rows.push(
            TableRow::new(["Receitas", &money(period.income.as_deref(), currency)])
                .color(theme.success),
        );
        rows.push(
            TableRow::new(["Despesas", &money(period.expenses.as_deref(), currency)])
                .color(theme.danger),
        );
        rows.push(
            TableRow::new(["Resultado", &money(period.result.as_deref(), currency)])
                .color(amount_color(period.result.as_deref(), &theme)),
        );
    } else {
        rows.push(TableRow::new(["Período", "Sem dados."]));
    }

    rows.push(TableRow::new(["PROJEÇÃO", ""]).color(theme.accent));
    if let Some(projection) = &data.projection {
        rows.push(TableRow::new([
            "Saldo atual",
            &money(projection.current_balance.as_deref(), currency),
        ]));
        rows.push(
            TableRow::new([
                "Receita esperada",
                &money(projection.expected_income.as_deref(), currency),
            ])
            .color(theme.success),
        );
        if let Some(outflows) = &projection.committed_outflows {
            for (label, value) in [
                ("Faturas", &outflows.bills),
                ("Parcelas", &outflows.installments),
                ("Assinaturas", &outflows.subscriptions),
                ("Agendados", &outflows.scheduled),
            ] {
                if let Some(value) = value {
                    rows.push(
                        TableRow::new([label, &money(Some(value), currency)]).color(theme.danger),
                    );
                }
            }
        }
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
    } else {
        rows.push(TableRow::new(["Projeção", "Sem dados."]));
    }

    ui.panel(Panel::new().title("Fluxo de Caixa"), move |panel| {
        panel.table(
            TableOptions {
                columns: vec![
                    TableColumn::new("Campo").width(Size::Cells(20)),
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
            "cash_flow_facts",
        );
    });
}

fn amount_color(value: Option<&str>, theme: &Theme) -> Color {
    match value.and_then(|value| value.parse::<f64>().ok()) {
        Some(amount) if amount > 0.0 => theme.success,
        Some(amount) if amount < 0.0 => theme.danger,
        _ => theme.foreground,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hqtui::testing::render_to_text;

    #[test]
    fn result_and_projected_balance_remain_reachable() {
        let data: Overview = serde_json::from_value(serde_json::json!({
            "currency_code": "BRL",
            "period_start": "2026-10-01",
            "period_end": "2026-10-31",
            "period": {"income": "5000", "expenses": "1200", "result": "3800"},
            "projection": {
                "current_balance": "3800", "expected_income": "500",
                "committed_outflows": {"bills": "100", "installments": "200", "subscriptions": "30", "scheduled": "40"},
                "projected_balance": "3930"
            }
        })).unwrap();
        for (width, height) in [(60, 18), (80, 24)] {
            let first = render_to_text(width, height, "dark", |ui| {
                crate::presentation::shell(
                    ui,
                    crate::keymap::Screen::CashFlow,
                    "Pessoal".into(),
                    "Ok".into(),
                    false,
                    |content| render_cash_flow_data(content, &data, 0),
                );
            });
            assert!(
                first.contains("Resultado") && first.contains("3.800,00 BRL"),
                "{width}x{height}\n{first}"
            );
            let last = render_to_text(width, height, "dark", |ui| {
                crate::presentation::shell(
                    ui,
                    crate::keymap::Screen::CashFlow,
                    "Pessoal".into(),
                    "Ok".into(),
                    false,
                    |content| render_cash_flow_data(content, &data, usize::MAX),
                );
            });
            assert!(
                last.contains("Saldo projetado") && last.contains("3.930,00 BRL"),
                "{width}x{height}\n{last}"
            );
        }
    }
}
