use hqtui::{
    buffer::{Attrs, Style},
    color::Color,
    prelude::*,
    surface::{BoxOptions, TextOptions},
    unicode::{Align, fit, string_width, wrap},
    widgets::{ModalOptions, draw_modal},
};

use crate::{
    models::Transaction,
    period::{day_groups, money_cents, summarize},
};

use super::{
    TransactionMode, TransactionsScreen,
    model::{Choice, FilterField, FormField, PickerKind, TransactionForm, format_amount},
};

const RED: Color = Color::rgb(255, 119, 134);

#[derive(Clone)]
pub(super) enum Action {
    Scroll,
    Key(String),
    Row(usize),
    Field(FormField),
    Filter(FilterField),
    Choice(Choice),
}

fn coord(value: usize) -> isize {
    isize::try_from(value).unwrap_or(isize::MAX)
}

fn text(surface: &Surface, y: usize, value: &str, color: Color) {
    surface.text(
        1,
        coord(y),
        value,
        &TextOptions {
            fg: Some(color),
            max_width: Some(surface.width().saturating_sub(2)),
            ..Default::default()
        },
    );
}

fn hit(
    surface: &Surface,
    x: usize,
    y: usize,
    width: usize,
    action: Action,
    hits: &crate::interaction::HitMap<Action>,
) {
    hits.add(&surface.sub(coord(x), coord(y), width, 1), action);
}

fn buttons(
    surface: &Surface,
    y: usize,
    items: &[(&str, &str)],
    hits: &crate::interaction::HitMap<Action>,
) {
    let mut x = 1;
    for (label, key) in items {
        let width = string_width(label) + 2;
        if x + width > surface.width() {
            break;
        }
        surface.text(
            coord(x),
            coord(y),
            &format!(" {label} "),
            &TextOptions::new().fg(surface.theme.accent),
        );
        hit(surface, x, y, width, Action::Key((*key).into()), hits);
        x += width + 1;
    }
}

pub(super) fn draw(surface: &Surface, state: &TransactionsScreen) {
    let hits = state.hitboxes.clone();
    hits.clear();
    draw_list(surface, state, &hits);
    if state.is_modal_open() {
        hits.clear();
        draw_overlay(surface, state, &hits);
    }
}

fn draw_list(
    surface: &Surface,
    state: &TransactionsScreen,
    hits: &crate::interaction::HitMap<Action>,
) {
    let width = surface.width();
    let height = surface.height();
    if width < 30 || height < 9 {
        text(surface, 0, "Amplie o terminal para ver transações.", RED);
        return;
    }
    let summary_height = if height >= 25 { 4 } else { 2 };
    draw_summary(&surface.sub(0, 0, width, summary_height), state);
    let summary = summarize(&state.transactions);
    let mut controls_y = summary_height;
    for (currency, flow) in &summary.currencies {
        if currency != "BRL" {
            text(
                surface,
                controls_y,
                &format!(
                    "{} original · R {}  D {}  Saldo {}",
                    currency,
                    money_cents(flow.income, currency),
                    money_cents(flow.expenses, currency),
                    money_cents(flow.result(), currency)
                ),
                surface.theme.muted,
            );
            controls_y += 1;
        }
    }
    if summary.invalid_amounts > 0 {
        text(
            surface,
            controls_y,
            &format!(
                "{} valores inválidos excluídos dos totais",
                summary.invalid_amounts
            ),
            surface.theme.warning,
        );
        controls_y += 1;
    }
    buttons(
        surface,
        controls_y,
        &[
            ("n Nova", "n"),
            ("f Filtros", "f"),
            ("/ Busca", "/"),
            ("r Atualizar", "r"),
            ("E CSV", "E"),
        ],
        hits,
    );
    let chips = chips(state);
    let chip_text = if chips.is_empty() {
        "Período completo · sem ignorados nos totais".into()
    } else {
        chips.join("  ")
    };
    let chip_lines = wrap(&chip_text, width.saturating_sub(2));
    for (index, line) in chip_lines.iter().enumerate() {
        text(surface, controls_y + 1 + index, line, surface.theme.muted);
        if !chips.is_empty() {
            hit(
                surface,
                0,
                controls_y + 1 + index,
                width,
                Action::Key("f".into()),
                hits,
            );
        }
    }
    let table_y = controls_y + 1 + chip_lines.len();
    let table_height = height.saturating_sub(table_y + 1);
    let table = surface
        .sub(0, coord(table_y), width, table_height)
        .draw_box(
            &BoxOptions::new()
                .title(if state.loading || state.error.is_some() {
                    "Lançamentos".into()
                } else {
                    format!("Lançamentos · {}", state.transactions.len())
                })
                .border(BorderStyle::Rounded),
        );
    hits.add(&table, Action::Scroll);
    let width = table.width();
    let rows_visible = table.height().saturating_sub(1);
    let amount_width = if width >= 75 { 18 } else { 16 };
    let category_width = if width >= 65 { 16 } else { 0 };
    let account_width = if width >= 85 { 17 } else { 0 };
    let gaps = usize::from(category_width > 0) + usize::from(account_width > 0) + 1;
    let description_width =
        width.saturating_sub(amount_width + category_width + account_width + gaps + 2);
    let widths = [
        0,
        description_width,
        category_width,
        account_width,
        amount_width,
    ];
    text(
        &table,
        0,
        &row_text("", "DESCRIÇÃO", "CATEGORIA", "CONTA", "VALOR", widths),
        surface.theme.muted,
    );
    if let Some(error) = &state.error {
        text(&table, 1, error, RED);
        text(&table, 2, "r para tentar novamente", surface.theme.muted);
        return;
    }
    if state.loading {
        text(
            &table,
            1,
            "Carregando período completo…",
            surface.theme.muted,
        );
        return;
    }
    if state.transactions.is_empty() {
        text(
            &table,
            2,
            "Nenhuma transação neste período.",
            surface.theme.foreground,
        );
        text(
            &table,
            3,
            "[ ] Outro mês · p Período · c Limpar filtros",
            surface.theme.muted,
        );
    }
    let mut display = Vec::new();
    for group in day_groups(&state.transactions) {
        let date = group.date.map_or_else(
            || "Sem data válida".into(),
            |date| date.format("%d/%m/%Y").to_string(),
        );
        if group.summary.currencies.is_empty() {
            display.push((None, format!("{date} · Sem movimento nos totais")));
        } else {
            for (currency, flow) in &group.summary.currencies {
                display.push((
                    None,
                    format!(
                        "{date} · R {}  D {}  Saldo {}",
                        money_cents(flow.income, currency),
                        money_cents(flow.expenses, currency),
                        money_cents(flow.result(), currency)
                    ),
                ));
            }
        }
        display.extend(
            group
                .indices
                .into_iter()
                .map(|index| (Some(index), String::new())),
        );
    }
    let selected_row = display
        .iter()
        .position(|(index, _)| index.is_some() && *index == state.selected_index)
        .unwrap_or(0);
    let offset = selected_row.saturating_sub(rows_visible.saturating_sub(1));
    for (row, (index, heading)) in display.iter().enumerate().skip(offset).take(rows_visible) {
        let y = 1 + row - offset;
        let Some(index) = index else {
            table.style_rect(
                0,
                coord(y),
                width,
                1,
                &Style::new().with_bg(surface.theme.surface),
            );
            text(&table, y, heading, surface.theme.accent);
            continue;
        };
        let tx = &state.transactions[*index];
        if Some(*index) == state.selected_index {
            table.style_rect(
                0,
                coord(y),
                width,
                1,
                &Style::new().with_bg(surface.theme.selection),
            );
            table.text(
                0,
                coord(y),
                "▸",
                &TextOptions::new().fg(surface.theme.accent),
            );
        }
        let description = tx
            .description
            .as_deref()
            .or(tx.merchant_name.as_deref())
            .unwrap_or("Sem descrição");
        let description = if tx.ignored {
            format!("○ {description}")
        } else {
            description.into()
        };
        let category = state
            .categories
            .iter()
            .find(|c| Some(c.id.as_str()) == tx.category_id.as_deref())
            .map_or("Sem categoria", |c| c.name.as_str());
        let account = state
            .accounts
            .iter()
            .find(|a| a.id == tx.account_id)
            .map_or("—", |a| a.name.as_str());
        text(
            &table,
            y,
            &row_text("", &description, category, account, "", widths),
            if tx.ignored {
                surface.theme.muted
            } else {
                surface.theme.foreground
            },
        );
        table.text(
            coord(width.saturating_sub(amount_width + 1)),
            coord(y),
            &fit(&transaction_amount(tx), amount_width, Align::Right),
            &TextOptions::new().fg(if tx.ignored {
                surface.theme.muted
            } else if tx.tx_type.eq_ignore_ascii_case("credit") {
                surface.theme.success
            } else {
                surface.theme.danger
            }),
        );
        hit(&table, 0, y, width, Action::Row(*index), hits);
    }
    text(
        surface,
        height - 1,
        "j/k Mover · Enter Detalhes · e Editar · d Excluir · ? Ajuda",
        surface.theme.muted,
    );
}

fn transaction_amount(tx: &Transaction) -> String {
    let amount = format_amount(&tx.amount, &tx.tx_type);
    match tx.currency_code.as_deref().filter(|code| !code.is_empty()) {
        Some(code) if code != "BRL" => amount.replacen("R$", code, 1),
        Some(_) => amount,
        None => amount.replacen("R$", "?", 1),
    }
}

fn draw_summary(surface: &Surface, state: &TransactionsScreen) {
    if state.loading || state.error.is_some() {
        text(
            surface,
            0,
            if state.loading {
                "Carregando período completo…"
            } else {
                "Totais indisponíveis · r para tentar novamente"
            },
            surface.theme.muted,
        );
        return;
    }
    let summary = summarize(&state.transactions);
    let flow = summary.currencies.get("BRL").copied().unwrap_or_default();
    let income = money_cents(flow.income, "BRL");
    let expenses = money_cents(flow.expenses, "BRL");
    let result = money_cents(flow.result(), "BRL");
    let count = format!("{} lançamentos", state.transactions.len());
    if surface.height() < 4 {
        text(
            surface,
            0,
            &format!("Receitas {income}  ·  Despesas {expenses}"),
            surface.theme.foreground,
        );
        text(
            surface,
            1,
            &format!("Saldo do período {result}  ·  {count}"),
            if flow.result() < 0 {
                surface.theme.danger
            } else {
                surface.theme.success
            },
        );
        return;
    }
    let summaries = [
        ("Receitas", income, surface.theme.success),
        ("Despesas", expenses, surface.theme.danger),
        (
            "Saldo do período",
            result,
            if flow.result() < 0 {
                surface.theme.danger
            } else {
                surface.theme.success
            },
        ),
    ];
    let card_width = surface.width().saturating_sub(2) / 3;
    for (index, (label, value, color)) in summaries.into_iter().enumerate() {
        let x = index * (card_width + 1);
        let width = if index == 2 {
            surface.width().saturating_sub(x)
        } else {
            card_width
        };
        let card = surface
            .sub(coord(x), 0, width, 4)
            .draw_box(&BoxOptions::new().title(label).border(BorderStyle::Rounded));
        card.text(
            1,
            0,
            &value,
            &TextOptions::new()
                .fg(color)
                .attrs(Attrs::BOLD)
                .max_width(card.width().saturating_sub(2)),
        );
        text(
            &card,
            1,
            if index == 2 {
                &count
            } else {
                "BRL · sem ignorados"
            },
            surface.theme.muted,
        );
    }
}

fn row_text(
    date: &str,
    description: &str,
    category: &str,
    account: &str,
    amount: &str,
    widths: [usize; 5],
) -> String {
    [date, description, category, account, amount]
        .into_iter()
        .zip(widths)
        .enumerate()
        .filter(|(_, (_, width))| *width > 0)
        .map(|(index, (value, width))| {
            fit(
                value,
                width,
                if index == 4 {
                    Align::Right
                } else {
                    Align::Left
                },
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn chips(state: &TransactionsScreen) -> Vec<String> {
    let f = &state.filters;
    let mut chips = Vec::new();
    if !f.search.is_empty() {
        chips.push(format!("[Busca: {}]", f.search));
    }
    for (label, id, kind) in [
        ("Conta", &f.account_id, PickerKind::Account),
        ("Categoria", &f.category_id, PickerKind::Category),
        ("Tag", &f.tag_id, PickerKind::Tags),
    ] {
        if let Some(id) = id {
            chips.push(format!("[{label}: {}]", choice_name(state, kind, Some(id))));
        }
    }
    if f.tx_type.api_value().is_some() {
        chips.push(format!("[{}]", f.tx_type.label()));
    }
    if f.ignored.api_value().is_some() {
        chips.push(format!("[{}]", f.ignored.label()));
    }
    chips
}

fn choice_name(state: &TransactionsScreen, kind: PickerKind, id: Option<&str>) -> String {
    let Some(id) = id else {
        return "Não selecionado".into();
    };
    let name = match kind {
        PickerKind::Account => state.accounts.iter().find(|a| a.id == id).map(|a| &a.name),
        PickerKind::Category => state
            .categories
            .iter()
            .find(|c| c.id == id)
            .map(|c| &c.name),
        PickerKind::Tags => state.tags.iter().find(|t| t.id == id).map(|t| &t.name),
    };
    name.cloned().unwrap_or_else(|| id.into())
}

fn form_value(state: &TransactionsScreen, form: &TransactionForm, field: FormField) -> String {
    match field {
        FormField::Amount => form.amount.clone(),
        FormField::Description => form.description.clone(),
        FormField::Type => format!("{}  [Space alterna]", form.tx_type.label()),
        FormField::Account => choice_name(state, PickerKind::Account, form.account_id.as_deref()),
        FormField::Category => {
            choice_name(state, PickerKind::Category, form.category_id.as_deref())
        }
        FormField::Tags => {
            if form.tag_ids.is_empty() {
                "Nenhuma · Enter seleciona".into()
            } else {
                form.tag_ids
                    .iter()
                    .map(|id| choice_name(state, PickerKind::Tags, Some(id)))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        }
        FormField::Merchant => form.merchant.clone(),
        FormField::Notes => form.notes.clone(),
        FormField::Date => form.date.clone(),
        FormField::Ignored => if form.ignored { "Sim" } else { "Não" }.into(),
    }
}

fn draw_overlay(
    surface: &Surface,
    state: &TransactionsScreen,
    hits: &crate::interaction::HitMap<Action>,
) {
    if let Some(picker) = state.picker() {
        let title = match picker.kind {
            PickerKind::Account => "Selecionar conta",
            PickerKind::Category => "Selecionar categoria",
            PickerKind::Tags => "Selecionar tags",
        };
        let inner = draw_modal(
            surface,
            &ModalOptions {
                title: Some(title.into()),
                width: Some(68),
                height: Some(20),
                ..Default::default()
            },
        );
        text(
            &inner,
            0,
            &format!("Buscar: {}▏", picker.query),
            inner.theme.accent,
        );
        hits.add(&inner, Action::Scroll);
        let choices = state.choices(picker);
        let visible = inner.height().saturating_sub(3);
        let offset = picker.selected.saturating_sub(visible.saturating_sub(1));
        if choices.is_empty() {
            text(
                &inner,
                2,
                "Nenhum resultado. Ctrl-U limpa a busca.",
                inner.theme.muted,
            );
        }
        for (index, choice) in choices.iter().enumerate().skip(offset).take(visible) {
            let y = index - offset + 1;
            let selected = if let TransactionMode::Form(form) = &state.mode {
                choice
                    .id
                    .as_ref()
                    .is_some_and(|id| form.tag_ids.contains(id))
                    && picker.kind == PickerKind::Tags
            } else {
                false
            };
            let label = format!(
                "{} {} {}",
                if index == picker.selected { "›" } else { " " },
                if selected { "●" } else { " " },
                choice.label
            );
            text(
                &inner,
                y,
                &label,
                if index == picker.selected {
                    inner.theme.accent
                } else {
                    inner.theme.foreground
                },
            );
            hit(
                &inner,
                0,
                y,
                inner.width(),
                Action::Choice(choice.clone()),
                hits,
            );
        }
        text(
            &inner,
            inner.height().saturating_sub(2),
            "↑/↓ escolher · Enter selecionar",
            inner.theme.muted,
        );
        buttons(
            &inner,
            inner.height().saturating_sub(1),
            &[("Esc Voltar", "escape")],
            hits,
        );
        return;
    }
    match &state.mode {
        TransactionMode::Form(form) => {
            let title = if form.original.is_some() {
                "Editar transação"
            } else {
                "Nova transação"
            };
            let inner = draw_modal(
                surface,
                &ModalOptions {
                    title: Some(title.into()),
                    width: Some(84),
                    height: Some(29),
                    ..Default::default()
                },
            );
            let hint = if form.pending_create.is_some() {
                "Envio incerto · rascunho bloqueado para evitar duplicação"
            } else if form.connected() {
                "Importada · campos de origem são somente leitura"
            } else {
                "Valor positivo · Data AAAA-MM-DD ou DD/MM/AAAA"
            };
            text(&inner, 0, hint, inner.theme.muted);
            let reserved = if form.pending_create.is_some() { 5 } else { 4 };
            hits.add(&inner, Action::Scroll);
            let visible = inner.height().saturating_sub(reserved) / 2;
            let focused_index = FormField::ALL
                .iter()
                .position(|field| *field == form.focused)
                .unwrap_or(0);
            let offset = focused_index.saturating_sub(visible.saturating_sub(1));
            for (index, field) in FormField::ALL.iter().enumerate().skip(offset).take(visible) {
                let y = 1 + (index - offset) * 2;
                let label = if *field == FormField::Amount {
                    let currency = if let Some(original) = &form.original {
                        original.currency_code.as_deref()
                    } else {
                        state
                            .accounts
                            .iter()
                            .find(|account| Some(&account.id) == form.account_id.as_ref())
                            .and_then(|account| account.currency_code.as_deref())
                    };
                    format!(
                        "Valor ({})",
                        currency
                            .map(str::trim)
                            .filter(|code| !code.is_empty())
                            .unwrap_or("?")
                    )
                } else {
                    field.label().to_owned()
                };
                field_row(
                    &inner,
                    y,
                    &label,
                    &form_value(state, form, *field),
                    *field == form.focused,
                    form.pending_create.is_some() || !form.editable(*field),
                    form.errors.get(field).map(String::as_str),
                );
                hit(&inner, 0, y, inner.width(), Action::Field(*field), hits);
            }
            if let Some(error) = &form.error {
                let error_y = inner
                    .height()
                    .saturating_sub(if form.pending_create.is_some() { 4 } else { 3 });
                text(&inner, error_y, error, RED);
            }
            if form.pending_create.is_some() {
                text(
                    &inner,
                    inner.height().saturating_sub(3),
                    "Ctrl-S repetir · Esc sair; confira a lista antes de N",
                    inner.theme.muted,
                );
            }
            buttons(
                &inner,
                inner.height().saturating_sub(2),
                &[("Ctrl-S Salvar", "ctrl+s"), ("Esc Cancelar", "escape")],
                hits,
            );
            text(
                &inner,
                inner.height().saturating_sub(1),
                "Tab / Shift-Tab campo · Enter confirmar · Ctrl-U limpar",
                inner.theme.muted,
            );
        }
        TransactionMode::Filters(form) => {
            let inner = draw_modal(
                surface,
                &ModalOptions {
                    title: Some("Filtrar transações".into()),
                    width: Some(80),
                    height: Some(23),
                    ..Default::default()
                },
            );
            hits.add(&inner, Action::Scroll);
            let visible = inner.height().saturating_sub(4) / 2;
            let selected = FilterField::ALL
                .iter()
                .position(|field| *field == form.focused)
                .unwrap_or(0);
            let offset = selected.saturating_sub(visible.saturating_sub(1));
            for (index, field) in FilterField::ALL
                .iter()
                .enumerate()
                .skip(offset)
                .take(visible)
            {
                let value = match field {
                    FilterField::Account => {
                        choice_name(state, PickerKind::Account, form.draft.account_id.as_deref())
                    }
                    FilterField::Category => choice_name(
                        state,
                        PickerKind::Category,
                        form.draft.category_id.as_deref(),
                    ),
                    FilterField::Tag => {
                        choice_name(state, PickerKind::Tags, form.draft.tag_id.as_deref())
                    }
                    FilterField::Type => form.draft.tx_type.label().into(),
                    FilterField::Ignored => form.draft.ignored.label().into(),
                    FilterField::Search => form.draft.search.clone(),
                };
                let y = 1 + (index - offset) * 2;
                field_row(
                    &inner,
                    y,
                    field.label(),
                    &value,
                    *field == form.focused,
                    false,
                    None,
                );
                hit(&inner, 0, y, inner.width(), Action::Filter(*field), hits);
            }
            if let Some(error) = &form.error {
                text(&inner, inner.height().saturating_sub(3), error, RED);
            }
            buttons(
                &inner,
                inner.height().saturating_sub(2),
                &[
                    ("Ctrl-S Aplicar", "ctrl+s"),
                    ("Ctrl-L Limpar", "ctrl+l"),
                    ("Esc Voltar", "escape"),
                ],
                hits,
            );
            text(
                &inner,
                inner.height().saturating_sub(1),
                "Tab campo · Enter selecionar · Ctrl-U limpar texto",
                inner.theme.muted,
            );
        }
        TransactionMode::Search(query) => {
            let inner = draw_modal(
                surface,
                &ModalOptions {
                    title: Some("Buscar transações".into()),
                    width: Some(70),
                    height: Some(8),
                    ..Default::default()
                },
            );
            field_row(&inner, 1, "Busca", query, true, false, None);
            buttons(
                &inner,
                inner.height().saturating_sub(1),
                &[("Enter Buscar", "enter"), ("Esc Cancelar", "escape")],
                hits,
            );
        }
        TransactionMode::DeleteConfirm {
            transaction,
            confirmed,
        } => {
            let inner = draw_modal(
                surface,
                &ModalOptions {
                    title: Some("Excluir transação?".into()),
                    width: Some(66),
                    height: Some(10),
                    color: Some(RED),
                    ..Default::default()
                },
            );
            text(
                &inner,
                1,
                transaction
                    .description
                    .as_deref()
                    .unwrap_or("Transação selecionada"),
                inner.theme.foreground,
            );
            text(&inner, 2, &transaction_amount(transaction), RED);
            text(
                &inner,
                3,
                state.error.as_deref().unwrap_or("Esta ação é permanente."),
                if state.error.is_some() {
                    RED
                } else {
                    inner.theme.muted
                },
            );
            text(
                &inner,
                4,
                if *confirmed {
                    "  Cancelar      › Excluir"
                } else {
                    "› Cancelar        Excluir"
                },
                inner.theme.accent,
            );
            buttons(
                &inner,
                inner.height().saturating_sub(1),
                &[("Esc Cancelar", "escape"), ("y Excluir", "y")],
                hits,
            );
        }
        TransactionMode::Detail(tx) => {
            let message = format!(
                "{}\n{}\nData: {}\nConta: {}\nCategoria: {}\nTags: {}\nStatus: {} · Origem: {}\nIgnorado: {}\nNotas: {}",
                tx.description.as_deref().unwrap_or("Sem descrição"),
                transaction_amount(tx),
                tx.transacted_at.as_deref().unwrap_or("—"),
                choice_name(state, PickerKind::Account, Some(&tx.account_id)),
                choice_name(state, PickerKind::Category, tx.category_id.as_deref()),
                tx.tag_ids
                    .iter()
                    .map(|id| choice_name(state, PickerKind::Tags, Some(id)))
                    .collect::<Vec<_>>()
                    .join(", "),
                tx.status.as_deref().unwrap_or("—"),
                tx.source.as_deref().unwrap_or("—"),
                if tx.ignored { "Sim" } else { "Não" },
                tx.notes.as_deref().unwrap_or("—")
            );
            let inner = draw_modal(
                surface,
                &ModalOptions {
                    title: Some("Detalhes da transação".into()),
                    width: Some(84),
                    height: Some(18),
                    message: Some(message),
                    align: Some(Align::Left),
                    ..Default::default()
                },
            );
            buttons(
                &inner,
                inner.height().saturating_sub(1),
                &[
                    ("e Editar", "e"),
                    ("d Excluir", "d"),
                    ("Esc Voltar", "escape"),
                ],
                hits,
            );
        }
        TransactionMode::ExportSuccess(path) => {
            let inner = draw_modal(
                surface,
                &ModalOptions {
                    title: Some("CSV exportado".into()),
                    message: Some(format!("Salvo no diretório atual:\n{path}")),
                    width: Some(78),
                    height: Some(9),
                    ..Default::default()
                },
            );
            buttons(
                &inner,
                inner.height().saturating_sub(1),
                &[("Enter Fechar", "enter")],
                hits,
            );
        }
        TransactionMode::List => {}
    }
}

fn field_row(
    surface: &Surface,
    y: usize,
    label: &str,
    value: &str,
    focused: bool,
    readonly: bool,
    error: Option<&str>,
) {
    let label_width = if surface.width() < 44 { 12 } else { 19 };
    let label = format!("{} {}:", if focused { "›" } else { " " }, label);
    text(
        surface,
        y,
        &fit(&label, label_width - 1, Align::Left),
        if focused {
            surface.theme.accent
        } else {
            surface.theme.muted
        },
    );
    let value_width = surface.width().saturating_sub(label_width + 3);
    let value = if readonly {
        format!("{value}  [fixo]")
    } else if focused {
        format!("{value}▏")
    } else if value.is_empty() {
        "—".into()
    } else {
        value.into()
    };
    surface.text(
        coord(label_width + 1),
        coord(y),
        &fit(&value, value_width, Align::Left),
        &TextOptions {
            fg: Some(if readonly {
                surface.theme.muted
            } else {
                surface.theme.foreground
            }),
            bg: focused.then_some(surface.theme.selection),
            attrs: focused.then_some(Attrs::BOLD),
            ..Default::default()
        },
    );
    if let Some(error) = error {
        text(surface, y + 1, error, RED);
    }
}
