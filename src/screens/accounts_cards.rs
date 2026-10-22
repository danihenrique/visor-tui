use hqtui::prelude::*;
use hqtui::unicode::Align;
use hqtui::widgets::{
    CheckboxOptions, CheckboxVariant, KeyValueOptions, KeyValueRow, TableColumn, TableOptions,
    TableRow,
};

use crate::api::ApiClient;
use crate::models::{Account, Card};

fn format_money(value: Option<&str>, currency: Option<&str>) -> String {
    crate::presentation::format_currency(value, currency)
}

/// Determine balance color: green for positive, red for negative/debt.
fn balance_color(balance: Option<&str>, theme: &Theme) -> Color {
    match balance.and_then(|b| b.parse::<f64>().ok()) {
        Some(v) if v >= 0.0 => theme.success,
        Some(_) => theme.danger,
        None => theme.foreground,
    }
}

/// Render the Accounts & Cards screen content into a container.
#[allow(clippy::cast_possible_truncation)]
pub fn render(ui: &mut Container<'_>, api: &ApiClient, show_hidden: bool) {
    render_scrolled(ui, api, show_hidden, 0);
}

pub fn render_scrolled(ui: &mut Container<'_>, api: &ApiClient, show_hidden: bool, offset: usize) {
    let accounts_result = api.accounts(show_hidden);
    let cards_result = api.cards();

    match (accounts_result, cards_result) {
        (Ok(accounts), Ok(cards)) => {
            render_content(ui, &accounts, &cards, show_hidden, offset);
        }
        (Err(e), _) | (_, Err(e)) => {
            let err_msg = format!("Falha ao carregar dados: {e}");
            ui.panel(Panel::new().title("Erro"), move |p| {
                p.styled_text(&err_msg, TextStyle::new().fg(Color::rgb(239, 68, 68)));
                p.text("Verifique sua conexão e credenciais.");
            });
        }
    }
}

/// Pre-computed card display data to avoid lifetime issues in closures.
struct CardData {
    name: String,
    provider: String,
    limit_fmt: String,
    used_fmt: String,
    available_fmt: String,
    close_date: String,
    due_date: String,
    meter_ratio: Option<f64>,
    meter_pct: String,
}

fn render_content(
    ui: &mut Container<'_>,
    accounts: &[Account],
    cards: &[Card],
    show_hidden: bool,
    offset: usize,
) {
    // Pre-compute all data before entering closures to avoid lifetime issues.
    let theme = ui.theme().clone();

    // Pre-compute account table rows
    let account_columns = vec![
        TableColumn::new("Nome").width(Size::Fr(2.0)),
        TableColumn::new("Tipo").width(Size::Fr(1.0)),
        TableColumn::new("Fonte").width(Size::Fr(1.0)),
        TableColumn::new("Saldo")
            .width(Size::Fr(2.0))
            .align(Align::Right),
        TableColumn::new("Provedor").width(Size::Fr(1.5)),
    ];
    let account_rows: Vec<TableRow> = accounts
        .iter()
        .map(|a| {
            let bal_color = balance_color(a.balance.as_deref(), &theme);
            let balance_str = format_money(a.balance.as_deref(), a.currency_code.as_deref());
            TableRow::new([
                a.name.clone(),
                a.account_type.as_deref().unwrap_or("-").to_string(),
                a.source.as_deref().unwrap_or("-").to_string(),
                balance_str,
                a.provider_name.as_deref().unwrap_or("-").to_string(),
            ])
            .color(bal_color)
        })
        .collect();

    let has_accounts = !accounts.is_empty();

    // Pre-compute card data
    let card_data_list: Vec<CardData> = cards
        .iter()
        .skip(offset.min(cards.len().saturating_sub(1)))
        .take(1)
        .map(|card| {
            let currency = card.currency_code.as_deref();
            let limit_val: f64 = card
                .credit_limit
                .as_deref()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0);
            let used_val: f64 = card
                .used_limit
                .as_deref()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0);

            let meter_info = if limit_val > 0.0 {
                let ratio = (used_val / limit_val).clamp(0.0, 1.0);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let pct = (ratio * 100.0).round() as i64;
                Some((ratio, format!("{pct}%")))
            } else {
                None
            };

            CardData {
                name: card.name.clone(),
                provider: card.provider_name.as_deref().unwrap_or("-").to_string(),
                limit_fmt: format_money(card.credit_limit.as_deref(), currency),
                used_fmt: format_money(card.used_limit.as_deref(), currency),
                available_fmt: format_money(card.available_limit.as_deref(), currency),
                close_date: card
                    .current_bill_close_date
                    .as_deref()
                    .unwrap_or("-")
                    .to_string(),
                due_date: card
                    .current_bill_due_date
                    .as_deref()
                    .unwrap_or("-")
                    .to_string(),
                meter_ratio: meter_info.as_ref().map(|(r, _)| *r),
                meter_pct: meter_info.map(|(_, t)| t).unwrap_or_default(),
            }
        })
        .collect();
    let has_cards = !cards.is_empty();
    let warning_color = theme.warning;
    let success_color = theme.success;

    ui.column(Column::new().gap(1), move |col| {
        // Toggle for hidden accounts
        col.row(Row::new().size(Size::Cells(1)), move |row| {
            row.checkbox(
                CheckboxOptions {
                    variant: CheckboxVariant::Toggle,
                    ..CheckboxOptions::new("Mostrar contas ocultas (h)", show_hidden)
                },
                "toggle_hidden",
            );
        });

        // Accounts section
        col.panel(Panel::new().title("Contas"), move |p| {
            if has_accounts {
                p.table(
                    TableOptions {
                        columns: account_columns.clone(),
                        rows: account_rows.clone(),
                        header: true,
                        zebra: true,
                        scrollbar: true,
                        selected: (!account_rows.is_empty())
                            .then_some(offset.min(account_rows.len().saturating_sub(1))),
                        follow_selection: true,
                        ..Default::default()
                    },
                    "accounts_table",
                );
            } else {
                p.label("Nenhuma conta encontrada.");
            }
        });

        // Cards section
        col.panel(Panel::new().title("Cartões"), move |p| {
            if has_cards {
                for (i, cd) in card_data_list.iter().enumerate() {
                    if i > 0 {
                        p.divider(DividerOptions::default());
                    }

                    p.key_values(KeyValueOptions {
                        rows: vec![
                            KeyValueRow::new("Cartão", &cd.name),
                            KeyValueRow::new("Provedor", &cd.provider),
                            KeyValueRow::new("Limite", &cd.limit_fmt),
                            KeyValueRow::new("Usado", &cd.used_fmt).color(warning_color),
                            KeyValueRow::new("Disponível", &cd.available_fmt).color(success_color),
                            KeyValueRow::new("Fechamento", &cd.close_date),
                            KeyValueRow::new("Vencimento", &cd.due_date),
                        ],
                        label_width: Some(14),
                        ..Default::default()
                    });

                    if let Some(ratio) = cd.meter_ratio {
                        let pct_text = cd.meter_pct.clone();
                        p.draw(move |s| {
                            hqtui::widgets::draw_meter(
                                s,
                                &hqtui::widgets::MeterOptions {
                                    value: ratio,
                                    label: Some("Utilização".into()),
                                    text: Some(pct_text.clone()),
                                    ..Default::default()
                                },
                            );
                        });
                    }
                }
            } else {
                p.label("Nenhum cartão encontrado.");
            }
        });
    });
}

/// Render with pre-loaded data (for testing without API calls).
pub fn render_with_data(
    ui: &mut Container<'_>,
    accounts: &[Account],
    cards: &[Card],
    show_hidden: bool,
) {
    render_content(ui, accounts, cards, show_hidden, 0);
}
