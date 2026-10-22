use hqtui::prelude::*;
use hqtui::unicode::Align;

use crate::api::ApiClient;
use crate::models::{Account, Card};
use crate::presentation::{TableSelection, format_currency};

pub fn render(ui: &mut Container<'_>, api: &ApiClient, show_hidden: bool) {
    render_scrolled(ui, api, show_hidden, &mut TableSelection::default());
}

pub fn render_scrolled(
    ui: &mut Container<'_>,
    api: &ApiClient,
    show_hidden: bool,
    selection: &mut TableSelection,
) {
    match (api.accounts(show_hidden), api.cards()) {
        (Ok(accounts), Ok(cards)) => render_content(ui, &accounts, &cards, show_hidden, selection),
        (Err(error), _) | (_, Err(error)) => {
            selection.set_row_count(0);
            ui.text(&format!(
                "Falha ao carregar contas: {error}. Pressione r para tentar novamente."
            ));
        }
    }
}

fn render_content(
    ui: &mut Container<'_>,
    accounts: &[Account],
    cards: &[Card],
    show_hidden: bool,
    selection: &mut TableSelection,
) {
    let mut rows = Vec::new();
    for account in accounts {
        let color = if account
            .balance
            .as_deref()
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(0.0)
            < 0.0
        {
            ui.theme().danger
        } else {
            ui.theme().success
        };
        rows.push(
            TableRow::new([
                account.name.clone(),
                "Saldo".into(),
                format_currency(account.balance.as_deref(), account.currency_code.as_deref()),
            ])
            .color(color),
        );
        for (label, value) in [
            ("Tipo", account.account_type.as_deref()),
            ("Fonte", account.source.as_deref()),
            ("Provedor", account.provider_name.as_deref()),
        ] {
            rows.push(TableRow::new([
                account.name.clone(),
                label.into(),
                value.unwrap_or("—").into(),
            ]));
        }
    }
    for card in cards {
        for (label, amount) in [
            ("Limite", card.credit_limit.as_deref()),
            ("Usado", card.used_limit.as_deref()),
            ("Disponível", card.available_limit.as_deref()),
        ] {
            rows.push(
                TableRow::new([
                    card.name.clone(),
                    label.into(),
                    format_currency(amount, card.currency_code.as_deref()),
                ])
                .color(if label == "Usado" {
                    ui.theme().warning
                } else {
                    ui.theme().success
                }),
            );
        }
        for (label, value) in [
            ("Provedor", card.provider_name.as_deref()),
            ("Fechamento", card.current_bill_close_date.as_deref()),
            ("Vencimento", card.current_bill_due_date.as_deref()),
        ] {
            rows.push(TableRow::new([
                card.name.clone(),
                label.into(),
                value.unwrap_or("—").into(),
            ]));
        }
        let utilization = card
            .credit_limit
            .as_deref()
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|limit| *limit > 0.0)
            .map_or_else(
                || "—".into(),
                |limit| {
                    let used = card
                        .used_limit
                        .as_deref()
                        .and_then(|value| value.parse::<f64>().ok())
                        .unwrap_or(0.0);
                    format!("{:.0}%", used / limit * 100.0)
                },
            );
        rows.push(TableRow::new([
            card.name.clone(),
            "Utilização".into(),
            utilization,
        ]));
    }
    selection.set_row_count(rows.len());
    let selected = selection.selected();
    ui.column(Column::new().gap(1), move |col| {
        col.row(Row::new().size(Size::Cells(1)), move |row| {
            row.checkbox(
                CheckboxOptions::new("Mostrar contas ocultas (h)", show_hidden),
                "toggle_hidden",
            );
        });
        if rows.is_empty() {
            col.text("Nenhuma conta ou cartão encontrado.");
        } else {
            col.table(
                TableOptions {
                    columns: vec![
                        TableColumn::new("Conta / cartão").width(Size::Fill),
                        TableColumn::new("Campo").width(Size::Cells(12)),
                        TableColumn::new("Valor")
                            .width(Size::Cells(21))
                            .align(Align::Right),
                    ],
                    rows,
                    selected,
                    follow_selection: true,
                    zebra: true,
                    ..Default::default()
                },
                "accounts_table",
            );
        }
        col.text("j/k ou ↑/↓ Rolar todas as informações");
    });
}

pub fn render_with_data(
    ui: &mut Container<'_>,
    accounts: &[Account],
    cards: &[Card],
    show_hidden: bool,
) {
    render_content(
        ui,
        accounts,
        cards,
        show_hidden,
        &mut TableSelection::default(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_terminal_keeps_full_balances_and_card_due_dates_reachable() {
        let accounts: Vec<Account> = serde_json::from_value(
            serde_json::json!([{"id":"a","name":"Conta","balance":"12345.67"}]),
        )
        .unwrap();
        let cards: Vec<Card> = serde_json::from_value(serde_json::json!([{"id":"c","name":"Cartão","credit_limit":"20000.00","current_bill_due_date":"2026-11-01"}])).unwrap();
        let mut selection = TableSelection::default();
        let text = hqtui::testing::render_to_text(56, 10, "dark", |ui| {
            render_content(ui, &accounts, &cards, false, &mut selection)
        });
        assert!(text.contains("12.345,67 BRL"), "{text}");
        selection.move_by(isize::MAX);
        let text = hqtui::testing::render_to_text(56, 10, "dark", |ui| {
            render_content(ui, &accounts, &cards, false, &mut selection)
        });
        assert!(text.contains("2026-11-01"), "{text}");
    }
}
