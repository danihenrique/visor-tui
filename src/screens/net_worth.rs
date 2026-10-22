use hqtui::prelude::*;
use hqtui::widgets::{TableColumn, TableOptions, TableRow};

use crate::api::{ApiClient, ApiError};
use crate::models::{Investment, InvestmentSummary};

fn money(value: Option<&str>, currency: Option<&str>) -> String {
    crate::presentation::format_currency(value, currency)
}

pub fn render(ui: &mut Container<'_>, api: &ApiClient) {
    render_scrolled(ui, api, 0);
}

pub fn render_scrolled(ui: &mut Container<'_>, api: &ApiClient, selected: usize) {
    match api.investments() {
        Ok((investments, summary)) => {
            render_net_worth_data(ui, &investments, summary.as_ref(), selected);
        }
        Err(error) => render_error(ui, &error),
    }
}

fn render_error(ui: &mut Container<'_>, error: &ApiError) {
    let message = format!("Falha ao carregar patrimônio: {error}");
    let danger = ui.theme().danger;
    ui.panel(Panel::new().title("Erro"), move |panel| {
        panel.styled_text(&message, TextStyle::new().fg(danger));
        panel.label("Verifique sua conexão e credenciais.");
    });
}

fn render_net_worth_data(
    ui: &mut Container<'_>,
    investments: &[Investment],
    summary: Option<&InvestmentSummary>,
    selected: usize,
) {
    let theme = ui.theme().clone();
    let currency = summary
        .and_then(|summary| summary.currency_code.as_deref())
        .unwrap_or("BRL");
    let total = summary
        .and_then(|summary| summary.total_value.as_deref())
        .map_or_else(
            || "—".to_string(),
            |value| money(Some(value), Some(currency)),
        );
    let count = summary
        .and_then(|summary| summary.count)
        .unwrap_or_else(|| investments.len().try_into().unwrap_or(u32::MAX));
    let mut rows = vec![
        TableRow::new(["Valor total", &total]).color(amount_color(
            summary.and_then(|summary| summary.total_value.as_deref()),
            &theme,
        )),
        TableRow::new(["Investimentos", &count.to_string()]),
    ];

    if investments.is_empty() {
        rows.push(TableRow::new(["Investimentos", "Nenhum encontrado."]));
    }
    for (index, investment) in investments.iter().enumerate() {
        rows.push(
            TableRow::new([
                format!("INVESTIMENTO {}", index + 1),
                investment.name.clone(),
            ])
            .color(theme.accent),
        );
        rows.push(TableRow::new([
            "Tipo",
            investment.investment_type.as_deref().unwrap_or("—"),
        ]));
        rows.push(TableRow::new([
            "Categoria",
            investment.category.as_deref().unwrap_or("—"),
        ]));
        rows.push(
            TableRow::new([
                "Saldo",
                &money(
                    investment.balance.as_deref(),
                    investment.currency_code.as_deref(),
                ),
            ])
            .color(amount_color(investment.balance.as_deref(), &theme)),
        );
        rows.push(TableRow::new([
            "Provedor",
            investment.provider_name.as_deref().unwrap_or("—"),
        ]));
    }
    rows.push(
        TableRow::new(["Patrimônio líquido", &total]).color(amount_color(
            summary.and_then(|summary| summary.total_value.as_deref()),
            &theme,
        )),
    );

    ui.panel(Panel::new().title("Patrimônio"), move |panel| {
        panel.table(
            TableOptions {
                columns: vec![
                    TableColumn::new("Campo").width(Size::Cells(18)),
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
            "net_worth_facts",
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
    fn last_investment_details_remain_reachable() {
        let investments: Vec<Investment> = (0..4)
            .map(|index| {
                serde_json::from_value(serde_json::json!({
                    "id": index.to_string(), "name": format!("Fundo {index}"), "type": "Fundo",
                    "category": "Renda fixa", "balance": ((index + 1) * 1111).to_string(), "currency_code": "BRL",
                    "provider_name": format!("Corretora {index}")
                }))
                .unwrap()
            })
            .collect();
        let summary: InvestmentSummary = serde_json::from_value(serde_json::json!({
            "total_value": "11110", "currency_code": "BRL", "count": 4
        }))
        .unwrap();
        for (width, height) in [(60, 18), (80, 24)] {
            let first = render_to_text(width, height, "dark", |ui| {
                crate::presentation::shell(
                    ui,
                    crate::keymap::Screen::NetWorth,
                    "Pessoal".into(),
                    "Ok".into(),
                    false,
                    |content| render_net_worth_data(content, &investments, Some(&summary), 0),
                );
            });
            assert!(
                first.contains("Valor total") && first.contains("11.110,00 BRL"),
                "{width}x{height}\n{first}"
            );
            let last = render_to_text(width, height, "dark", |ui| {
                crate::presentation::shell(
                    ui,
                    crate::keymap::Screen::NetWorth,
                    "Pessoal".into(),
                    "Ok".into(),
                    false,
                    |content| render_net_worth_data(content, &investments, Some(&summary), 21),
                );
            });
            assert!(
                last.contains("Corretora 3") && last.contains("4.444,00 BRL"),
                "{width}x{height}\n{last}"
            );
        }
    }
}
