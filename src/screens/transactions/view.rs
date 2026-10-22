use hqtui::{
    buffer::{Attrs, Style},
    color::Color,
    prelude::*,
    surface::TextOptions,
    unicode::{Align, fit, string_width, wrap},
    widgets::{ModalOptions, draw_modal},
};

use super::{
    TransactionMode, TransactionsScreen,
    model::{Choice, FilterField, FormField, PickerKind, TransactionForm, format_amount},
};

const GREEN: Color = Color::rgb(83, 211, 160);
const RED: Color = Color::rgb(255, 119, 134);

#[derive(Clone)]
pub(super) enum Action {
    Key(String),
    Row(usize),
    Field(FormField),
    Filter(FilterField),
    Choice(Choice),
}

#[derive(Clone)]
pub(super) struct Hitbox {
    rect: Rect,
    pub action: Action,
}

impl Hitbox {
    pub fn contains(&self, x: usize, y: usize) -> bool {
        let (Ok(x), Ok(y)) = (isize::try_from(x), isize::try_from(y)) else {
            return false;
        };
        self.rect.contains(x, y)
    }
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
    hits: &mut Vec<Hitbox>,
) {
    hits.push(Hitbox {
        rect: surface.sub(coord(x), coord(y), width, 1).hit_rect(),
        action,
    });
}

fn buttons(surface: &Surface, y: usize, items: &[(&str, &str)], hits: &mut Vec<Hitbox>) {
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
    let mut hits = Vec::new();
    draw_list(surface, state, &mut hits);
    if state.is_modal_open() {
        hits.clear();
        draw_overlay(surface, state, &mut hits);
    }
    *state.hitboxes.borrow_mut() = hits;
}

fn draw_list(surface: &Surface, state: &TransactionsScreen, hits: &mut Vec<Hitbox>) {
    let width = surface.width();
    let height = surface.height();
    if width < 20 || height < 6 {
        text(surface, 0, "Amplie o terminal para ver transações.", RED);
        return;
    }
    text(
        surface,
        0,
        &format!("TRANSAÇÕES  ·  {} carregadas", state.transactions.len()),
        surface.theme.accent,
    );
    buttons(
        surface,
        1,
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
        "Todos os lançamentos · f para filtrar".into()
    } else {
        chips.join("  ")
    };
    let mut chip_lines = wrap(&chip_text, width.saturating_sub(2));
    let max_chip_lines = height.saturating_sub(9).max(1);
    if chip_lines.len() > max_chip_lines {
        chip_lines.truncate(max_chip_lines);
        chip_lines[max_chip_lines - 1] = "Mais filtros ativos · f para ver todos".into();
    }
    for (index, line) in chip_lines.iter().enumerate() {
        text(surface, 2 + index, line, surface.theme.muted);
        if !chips.is_empty() {
            hit(
                surface,
                1,
                2 + index,
                width.saturating_sub(2),
                Action::Key("f".into()),
                hits,
            );
        }
    }
    let actions_y = 2 + chip_lines.len();
    if let Some(error) = &state.error {
        text(surface, actions_y, error, RED);
    } else {
        buttons(
            surface,
            actions_y,
            &[
                ("Enter Detalhes", "enter"),
                ("e Editar", "e"),
                ("d Excluir", "d"),
                ("c Limpar filtros", "c"),
            ],
            hits,
        );
    }
    let table_y = actions_y + 2;
    let rows_visible = height.saturating_sub(table_y + 2);
    let selected = state.selected_index.unwrap_or(0);
    let offset = selected.saturating_sub(rows_visible.saturating_sub(1));
    let amount_width = if width >= 65 { 20 } else { 15 };
    let date_width = if width >= 48 { 11 } else { 0 };
    let category_width = if width >= 100 { 18 } else { 0 };
    let account_width = if width >= 120 { 18 } else { 0 };
    let gaps = [date_width, category_width, account_width]
        .iter()
        .filter(|width| **width > 0)
        .count()
        + 1;
    let description_width =
        width.saturating_sub(amount_width + date_width + category_width + account_width + 2 + gaps);
    let header = row_text(
        "DATA",
        "DESCRIÇÃO",
        "CATEGORIA",
        "CONTA",
        "VALOR",
        [
            date_width,
            description_width,
            category_width,
            account_width,
            amount_width,
        ],
    );
    text(surface, table_y - 1, &header, surface.theme.muted);
    if state.loading {
        text(
            surface,
            table_y + 1,
            "Carregando transações…",
            surface.theme.muted,
        );
    } else if state.transactions.is_empty() {
        text(
            surface,
            table_y + 1,
            "Nenhuma transação encontrada.",
            surface.theme.foreground,
        );
        text(
            surface,
            table_y + 2,
            "n para criar · f para ajustar filtros · c para limpar",
            surface.theme.muted,
        );
    }
    for (index, tx) in state
        .transactions
        .iter()
        .enumerate()
        .skip(offset)
        .take(rows_visible)
    {
        let y = table_y + index - offset;
        if Some(index) == state.selected_index {
            surface.style_rect(
                0,
                coord(y),
                width,
                1,
                &Style::new().with_bg(surface.theme.selection),
            );
        }
        let amount = format_amount(&tx.amount, &tx.tx_type);
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
        let date = tx
            .transacted_at
            .as_deref()
            .unwrap_or("—")
            .split('T')
            .next()
            .unwrap_or("—");
        let row = row_text(
            date,
            &description,
            category,
            account,
            "",
            [
                date_width,
                description_width,
                category_width,
                account_width,
                amount_width,
            ],
        );
        text(surface, y, &row, surface.theme.foreground);
        surface.text(
            coord(width.saturating_sub(amount_width + 1)),
            coord(y),
            &fit(&amount, amount_width, Align::Right),
            &TextOptions::new().fg(if tx.tx_type.eq_ignore_ascii_case("credit") {
                GREEN
            } else {
                RED
            }),
        );
        hit(surface, 0, y, width, Action::Row(index), hits);
    }
    let footer = if state.next_cursor.is_some() {
        "j/k mover · m carregar mais · ? atalhos"
    } else {
        "j/k mover · Enter detalhes · ? atalhos"
    };
    text(surface, height - 1, footer, surface.theme.muted);
    if state.next_cursor.is_some() {
        hit(surface, 0, height - 1, width, Action::Key("m".into()), hits);
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
        .filter(|(_, width)| *width > 0)
        .map(|(value, width)| fit(value, width, Align::Left))
        .collect::<Vec<_>>()
        .join(" ")
}

fn chips(state: &TransactionsScreen) -> Vec<String> {
    let f = &state.filters;
    let mut chips = Vec::new();
    for (label, value) in [
        ("De", f.date_from.as_str()),
        ("Até", f.date_to.as_str()),
        ("Busca", f.search.as_str()),
    ] {
        if !value.is_empty() {
            chips.push(format!("[{label}: {value}]"));
        }
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

fn draw_overlay(surface: &Surface, state: &TransactionsScreen, hits: &mut Vec<Hitbox>) {
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
            inner.height().saturating_sub(1),
            "↑/↓ escolher · Enter selecionar · Esc voltar",
            inner.theme.muted,
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
            let visible = inner.height().saturating_sub(reserved) / 2;
            let focused_index = FormField::ALL
                .iter()
                .position(|field| *field == form.focused)
                .unwrap_or(0);
            let offset = focused_index.saturating_sub(visible.saturating_sub(1));
            for (index, field) in FormField::ALL.iter().enumerate().skip(offset).take(visible) {
                let y = 1 + (index - offset) * 2;
                field_row(
                    &inner,
                    y,
                    field.label(),
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
                    FilterField::From => form.draft.date_from.clone(),
                    FilterField::To => form.draft.date_to.clone(),
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
            text(
                &inner,
                2,
                &format_amount(&transaction.amount, &transaction.tx_type),
                RED,
            );
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
                format_amount(&tx.amount, &tx.tx_type),
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
