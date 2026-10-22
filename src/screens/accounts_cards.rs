use hqtui::prelude::*;
use hqtui::unicode::Align;

use crate::api::ApiClient;
use crate::models::{Account, Card};
use crate::presentation::{
    Metric, TableSelection, Tone, amount, format_currency, metric_strip, money_total,
};

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

fn used_limit(card: &Card) -> Option<String> {
    use crate::presentation::minor_units;
    if let Some(value) = &card.used_limit {
        minor_units(value)?;
        return Some(value.clone());
    }
    let used = minor_units(card.credit_limit.as_deref()?)?
        .checked_sub(minor_units(card.available_limit.as_deref()?)?)?;
    let sign = if used < 0 { "-" } else { "" };
    let absolute = used.unsigned_abs();
    Some(format!("{sign}{}.{:02}", absolute / 100, absolute % 100))
}

fn utilization(card: &Card) -> Option<f64> {
    let limit = amount(card.credit_limit.as_deref()).filter(|value| *value > 0.0)?;
    let ratio = amount(used_limit(card).as_deref())? / limit;
    ratio.is_finite().then_some(ratio)
}

fn utilization_tone(ratio: Option<f64>) -> Tone {
    match ratio {
        Some(value) if value >= 0.9 => Tone::Negative,
        Some(value) if value >= 0.7 => Tone::Warning,
        Some(_) => Tone::Positive,
        None => Tone::Neutral,
    }
}

fn card_panel(ui: &mut Container<'_>, card: Card, selected: bool) {
    let ratio = utilization(&card);
    let used = used_limit(&card);
    let title = format!(
        "{}{} · {}",
        if selected { "▎ " } else { "" },
        card.name,
        card.provider_name
            .as_deref()
            .unwrap_or("Instituição não informada")
    );
    let border = if selected {
        ui.theme().border_focused
    } else {
        ui.theme().border
    };
    ui.panel(
        Panel {
            border_color: Some(border),
            ..Panel::new()
                .title(title)
                .border(BorderStyle::Rounded)
                .size(Size::Fill)
        },
        move |panel| {
            let color = utilization_tone(ratio).color(panel.theme());
            let percentage =
                ratio.map_or_else(|| "—".into(), |value| format!("{:.0}%", value * 100.0));
            if panel.height() >= 5 {
                if let Some(ratio) = ratio {
                    panel.meter(MeterOptions {
                        value: ratio.clamp(0.0, 1.0),
                        text: Some(percentage.clone()),
                        color: Some(color),
                        ..Default::default()
                    });
                } else {
                    panel.label("Utilização indisponível");
                }
            }
            let values = [
                (
                    format!("Usado  {percentage}"),
                    format_currency(used.as_deref(), card.currency_code.as_deref()),
                    color,
                ),
                (
                    "Disponível".into(),
                    format_currency(
                        card.available_limit.as_deref(),
                        card.currency_code.as_deref(),
                    ),
                    panel.theme().foreground,
                ),
                (
                    "Limite".into(),
                    format_currency(card.credit_limit.as_deref(), card.currency_code.as_deref()),
                    panel.theme().foreground,
                ),
            ];
            for (label, value, color) in values {
                panel.row(Row::new().size(Size::Cells(1)), move |row| {
                    row.label(&label);
                    row.styled_text(&value, TextStyle::new().fg(color).align(Align::Right));
                });
            }
            let close = card.current_bill_close_date.as_deref().unwrap_or("—");
            let due = card.current_bill_due_date.as_deref().unwrap_or("—");
            if panel.height() >= 6 {
                panel.label(&format!("Fechamento  {close}"));
                panel.text(&format!("Vencimento  {due}"));
            } else {
                panel.text(&format!("Fecha {close}  Vence {due}"));
            }
        },
    );
}

fn render_content(
    ui: &mut Container<'_>,
    accounts: &[Account],
    cards: &[Card],
    show_hidden: bool,
    selection: &mut TableSelection,
) {
    selection.set_row_count(accounts.len() + cards.len());
    let selected = selection.selected();
    let selected_card = selected.and_then(|index| index.checked_sub(accounts.len()));
    let height = ui.height();
    let compact = height < 17;
    let both_sections = !compact && !accounts.is_empty() && !cards.is_empty();
    let show_accounts = !accounts.is_empty() && (both_sections || selected_card.is_none());
    let show_cards = !cards.is_empty() && (both_sections || !show_accounts);
    let card_columns = if ui.width() >= 88 { 2 } else { 1 };
    let account_height = if both_sections {
        (accounts.len() + 3).min((height - 6) / 2)
    } else {
        0
    };
    let card_height = height.saturating_sub(
        account_height + if compact { 0 } else { 4 } + usize::from(height >= 9) + 1,
    );
    let card_rows = (card_height / 8)
        .max(1)
        .min(cards.len().div_ceil(card_columns).max(1));
    let card_capacity = card_columns * card_rows;
    let card_start = selected_card.unwrap_or(0) / card_capacity * card_capacity;
    let visible_cards: Vec<_> = cards
        .iter()
        .enumerate()
        .skip(card_start)
        .take(card_capacity)
        .map(|(index, card)| (index, card.clone()))
        .collect();
    let narrow = ui.width() < 65;
    let balance_width = accounts
        .iter()
        .map(|account| {
            hqtui::unicode::string_width(&format_currency(
                account.balance.as_deref(),
                account.currency_code.as_deref(),
            ))
        })
        .max()
        .unwrap_or(16)
        .max(16)
        .min(ui.width() / 2);
    let mut columns = vec![TableColumn::new("Nome").width(Size::Fill)];
    if !narrow {
        columns.push(TableColumn::new("Instituição").width(Size::Fill));
    }
    columns.push(TableColumn::new("Tipo").width(Size::Cells(10)));
    columns.push(
        TableColumn::new("Saldo")
            .width(Size::Cells(
                i64::try_from(balance_width).unwrap_or(i64::MAX),
            ))
            .align(Align::Right),
    );
    let rows = accounts
        .iter()
        .map(|account| {
            let source = match account.source.as_deref() {
                Some("manual") => "○ ".to_owned(),
                Some("connected" | "pluggy" | "open_finance") => "● ".to_owned(),
                Some(_) => "· ".to_owned(),
                _ => String::new(),
            };
            let mut cells = vec![format!("{source}{}", account.name)];
            if !narrow {
                cells.push(account.provider_name.clone().unwrap_or_else(|| "—".into()));
            }
            cells.push(match account.account_type.as_deref() {
                Some("CHECKING" | "checking") => "Corrente".into(),
                Some("SAVINGS" | "savings") => "Poupança".into(),
                Some("CASH" | "cash") => "Dinheiro".into(),
                Some(value) => value.into(),
                None => "—".into(),
            });
            cells.push(format_currency(
                account.balance.as_deref(),
                account.currency_code.as_deref(),
            ));
            let tone = match amount(account.balance.as_deref()) {
                Some(value) if value < 0.0 => Tone::Negative,
                Some(_) => Tone::Positive,
                None => Tone::Neutral,
            };
            let mut cell_colors = vec![None; cells.len()];
            *cell_colors.last_mut().unwrap() = Some(tone.color(ui.theme()));
            TableRow {
                cells,
                cell_colors,
                ..Default::default()
            }
        })
        .collect();
    let used_values: Vec<_> = cards.iter().map(used_limit).collect();
    let metrics = vec![
        Metric::new(
            "Saldo total",
            money_total(
                accounts
                    .iter()
                    .map(|account| (account.balance.as_deref(), account.currency_code.as_deref())),
            ),
            Tone::Accent,
        ),
        Metric::new(
            "Limite total",
            money_total(
                cards
                    .iter()
                    .map(|card| (card.credit_limit.as_deref(), card.currency_code.as_deref())),
            ),
            Tone::Neutral,
        ),
        Metric::new(
            "Usado",
            money_total(
                cards
                    .iter()
                    .zip(&used_values)
                    .map(|(card, used)| (used.as_deref(), card.currency_code.as_deref())),
            ),
            Tone::Warning,
        ),
    ];
    let count = accounts.len() + cards.len();
    let account_count = accounts.len();
    let card_count = cards.len();
    let institution = selected
        .and_then(|index| accounts.get(index))
        .and_then(|account| account.provider_name.clone());
    let account_detail = selected
        .and_then(|index| accounts.get(index))
        .map(|account| {
            format!(
                "{} · {}",
                account.name,
                account
                    .provider_name
                    .as_deref()
                    .unwrap_or("Instituição não informada")
            )
        });
    ui.column(Column::new().gap(0), move |col| {
        if !compact {
            metric_strip(col, metrics);
        }
        if height >= 9 {
            col.row(Row::new().size(Size::Cells(1)), move |row| {
                row.checkbox(
                    CheckboxOptions::new("Mostrar contas ocultas (h)", show_hidden),
                    "toggle_hidden",
                );
            });
        }
        if show_accounts {
            let size = if both_sections {
                Size::Cells(
                    i64::try_from((account_count + 3).min((height - 6) / 2)).unwrap_or(i64::MAX),
                )
            } else {
                Size::Fill
            };
            col.panel(
                Panel {
                    footer: account_detail,
                    ..Panel::new()
                        .title(format!("Contas · {account_count}"))
                        .border(BorderStyle::Rounded)
                        .size(size)
                },
                move |panel| {
                    panel.table(
                        TableOptions {
                            columns,
                            rows,
                            selected: selected.filter(|index| *index < account_count),
                            follow_selection: true,
                            zebra: true,
                            scrollbar: true,
                            ..Default::default()
                        },
                        "accounts_table",
                    );
                    if narrow && let Some(institution) = institution {
                        panel.label(&format!("Instituição · {institution}"));
                    }
                },
            );
        }
        if show_cards {
            col.column(Column::new().size(Size::Fill).gap(0), move |grid| {
                for chunk in visible_cards.chunks(card_columns) {
                    let cards = chunk.to_vec();
                    grid.row(Row::new().size(Size::Fill).gap(1), move |row| {
                        for (index, card) in cards {
                            card_panel(row, card, selected_card == Some(index));
                        }
                    });
                }
            });
        }
        if count == 0 {
            col.text("Nenhuma conta ou cartão encontrado.");
        }
        col.row(Row::new().size(Size::Cells(1)), move |footer| {
            footer.label(&format!(
                "j/k Selecionar · {} de {count} · {account_count} contas · {card_count} cartões",
                selected.map_or(0, |index| index + 1)
            ));
        });
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
    fn credit_usage_requires_known_finite_inputs_and_keeps_over_limit_ratio() {
        let mut card: Card = serde_json::from_value(
            serde_json::json!({"id":"c", "name":"Cartão", "credit_limit":"1000"}),
        )
        .unwrap();
        assert_eq!(used_limit(&card), None);
        assert_eq!(utilization(&card), None);
        card.available_limit = Some("400".into());
        assert_eq!(used_limit(&card).as_deref(), Some("600.00"));
        card.credit_limit = Some("9999999999999999.99".into());
        card.available_limit = Some("9999999999999999.98".into());
        assert_eq!(used_limit(&card).as_deref(), Some("0.01"));
        card.credit_limit = Some("1000".into());
        card.used_limit = Some("1250".into());
        assert_eq!(utilization(&card), Some(1.25));
        card.used_limit = Some("NaN".into());
        assert_eq!(utilization(&card), None);
        card.credit_limit = Some("0".into());
        card.used_limit = Some("0".into());
        assert_eq!(utilization(&card), None);
    }

    #[test]
    fn one_cursor_step_reaches_a_complete_card_and_last_card() {
        let accounts: Vec<Account> = serde_json::from_value(serde_json::json!([
            {"id":"a", "name":"Conta", "balance":"12345.67", "provider_name":"Banco", "type":"CHECKING"}
        ])).unwrap();
        let cards: Vec<Card> = serde_json::from_value(serde_json::json!([
            {"id":"c1", "name":"Primeiro", "credit_limit":"1000", "used_limit":"1250", "available_limit":"-250", "current_bill_close_date":"2026-10-25", "current_bill_due_date":"2026-11-01"},
            {"id":"c2", "name":"Último", "credit_limit":"2000", "current_bill_close_date":"2026-10-28", "current_bill_due_date":"2026-11-08"}
        ])).unwrap();
        let mut selection = TableSelection::default();
        hqtui::testing::render_to_text(80, 24, "dark", |ui| {
            render_content(ui, &accounts, &cards, false, &mut selection);
        });
        selection.move_by(1);
        let text = hqtui::testing::render_to_text(80, 24, "dark", |ui| {
            render_content(ui, &accounts, &cards, false, &mut selection);
        });
        for value in [
            "125%",
            "1.250,00 BRL",
            "-250,00 BRL",
            "1.000,00 BRL",
            "2026-10-25",
            "2026-11-01",
        ] {
            assert!(text.contains(value), "{value}\n{text}");
        }
        selection.move_by(isize::MAX);
        assert_eq!(selection.selected(), Some(2));
        let text = hqtui::testing::render_to_text(56, 10, "dark", |ui| {
            render_content(ui, &accounts, &cards, false, &mut selection);
        });
        assert!(text.contains("Último"), "{text}");
        assert!(text.contains("2026-11-08"), "{text}");
        let text = hqtui::testing::render_to_text(56, 10, "dark", |ui| {
            crate::presentation::shell(
                ui,
                crate::keymap::Screen::AccountsCards,
                "Pessoal".into(),
                "Sincronizado".into(),
                false,
                |body| render_content(body, &accounts, &cards, false, &mut selection),
            );
        });
        for value in ["2.000,00 BRL", "2026-11-08", "2026-10-28"] {
            assert!(text.contains(value), "{value}\n{text}");
        }
    }

    #[test]
    fn small_terminal_keeps_full_balances_and_card_due_dates_reachable() {
        let accounts: Vec<Account> = serde_json::from_value(
            serde_json::json!([{"id":"a","name":"Conta","balance":"12345.67","provider_name":"Banco Horizonte"}]),
        )
        .unwrap();
        let cards: Vec<Card> = serde_json::from_value(serde_json::json!([{"id":"c","name":"Cartão","credit_limit":"20000.00","current_bill_due_date":"2026-11-01"}])).unwrap();
        let mut selection = TableSelection::default();
        let text = hqtui::testing::render_to_text(56, 10, "dark", |ui| {
            render_content(ui, &accounts, &cards, false, &mut selection);
        });
        assert!(text.contains("12.345,67 BRL"), "{text}");
        assert!(text.contains("Banco Horizonte"), "{text}");
        selection.move_by(isize::MAX);
        let text = hqtui::testing::render_to_text(56, 10, "dark", |ui| {
            render_content(ui, &accounts, &cards, false, &mut selection);
        });
        assert!(text.contains("2026-11-01"), "{text}");
    }
}
