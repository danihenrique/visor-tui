use hqtui::prelude::*;
use hqtui::unicode::Align;

use crate::api::ApiClient;
use crate::models::Overview;
use crate::presentation::{TableSelection, format_currency};

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
    let currency = data.currency_code.as_deref();
    let mut rows = Vec::new();
    let mut money = |label: &str, value: Option<&str>, debt: bool| {
        let negative = value
            .and_then(|value| value.parse::<f64>().ok())
            .is_some_and(|value| value < 0.0);
        rows.push(
            TableRow::new([label.to_string(), format_currency(value, currency)]).color(
                if debt || negative {
                    ui.theme().danger
                } else {
                    ui.theme().success
                },
            ),
        );
    };
    money("Saldo em conta", data.bank_balance.as_deref(), false);
    money("Dívida cartão", data.credit_card_debt.as_deref(), true);
    money("Faturas abertas", data.open_bills_total.as_deref(), true);
    money("Patrimônio líquido", data.net_worth.as_deref(), false);
    if let Some(period) = &data.period {
        money("Receitas do período", period.income.as_deref(), false);
        money("Despesas do período", period.expenses.as_deref(), true);
        money("Resultado do período", period.result.as_deref(), false);
    }
    if let Some(projection) = &data.projection {
        money("Saldo atual", projection.current_balance.as_deref(), false);
        money(
            "Receita esperada",
            projection.expected_income.as_deref(),
            false,
        );
        if let Some(outflows) = &projection.committed_outflows {
            for (label, value) in [
                ("Comprometido · Faturas", outflows.bills.as_deref()),
                ("Comprometido · Parcelas", outflows.installments.as_deref()),
                (
                    "Comprometido · Assinaturas",
                    outflows.subscriptions.as_deref(),
                ),
                ("Comprometido · Agendados", outflows.scheduled.as_deref()),
            ] {
                money(label, value, true);
            }
        }
        money(
            "Dia a dia estimado",
            projection.estimated_day_to_day.as_deref(),
            true,
        );
        money(
            "Saldo projetado",
            projection.projected_balance.as_deref(),
            false,
        );
    }
    for bill in &data.upcoming_bills {
        let estimate = if bill.amount_is_estimate { "~" } else { "" };
        rows.push(
            TableRow::new([
                format!(
                    "{} · {}",
                    bill.due_date.as_deref().unwrap_or("Sem data"),
                    bill.description.as_deref().unwrap_or("Fatura")
                ),
                format!(
                    "{estimate}{}",
                    format_currency(bill.amount.as_deref(), currency)
                ),
            ])
            .color(ui.theme().warning),
        );
    }
    selection.set_row_count(rows.len());
    let selected = selection.selected();
    let period = format!(
        "{} a {}",
        data.period_start.as_deref().unwrap_or("—"),
        data.period_end.as_deref().unwrap_or("—")
    );
    ui.column(Column::new().gap(1), move |col| {
        col.text(&period);
        col.table(
            TableOptions {
                columns: vec![
                    TableColumn::new("Resumo financeiro").width(Size::Fill),
                    TableColumn::new("Valor")
                        .width(Size::Cells(24))
                        .align(Align::Right),
                ],
                rows,
                selected,
                follow_selection: true,
                zebra: true,
                ..Default::default()
            },
            "overview:table",
        );
        col.text("j/k ou ↑/↓ Rolar resumo e próximas faturas");
    });
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn draw_overview_mock(ui: &mut Container<'_>, data: &Overview) {
    draw_overview_data(ui, data, &mut TableSelection::default());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_overview_keeps_currency_and_projected_balance_reachable() {
        let data: Overview = serde_json::from_value(serde_json::json!({"bank_balance":"12345.67", "projection":{"projected_balance":"9876.54"}})).unwrap();
        let mut selection = TableSelection::default();
        let text = hqtui::testing::render_to_text(56, 10, "dark", |ui| {
            draw_overview_data(ui, &data, &mut selection)
        });
        assert!(text.contains("12.345,67 BRL"), "{text}");
        selection.move_by(isize::MAX);
        let text = hqtui::testing::render_to_text(56, 10, "dark", |ui| {
            draw_overview_data(ui, &data, &mut selection)
        });
        assert!(text.contains("Saldo projetado"));
        assert!(text.contains("9.876,54 BRL"), "{text}");
    }
}
