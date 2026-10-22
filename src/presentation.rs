use crate::interaction::{Control, Interactions, Interactive, coord};
use hqtui::prelude::*;

use crate::keymap::Screen;

#[derive(Clone, Copy, Debug)]
pub enum Tone {
    Neutral,
    Positive,
    Negative,
    Warning,
    Accent,
}

impl Tone {
    #[must_use]
    pub fn color(self, theme: &Theme) -> Color {
        match self {
            Self::Neutral => theme.foreground,
            Self::Positive => theme.success,
            Self::Negative => theme.danger,
            Self::Warning => theme.warning,
            Self::Accent => theme.accent,
        }
    }
}

pub struct Metric {
    pub label: String,
    pub value: String,
    pub tone: Tone,
}

impl Metric {
    pub fn new(label: impl Into<String>, value: impl Into<String>, tone: Tone) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            tone,
        }
    }
}

pub fn metric_strip(ui: &mut Container<'_>, metrics: Vec<Metric>) {
    let height = if ui.height() < 14 { 3 } else { 4 };
    ui.row(Row::new().size(Size::Cells(height)).gap(1), move |row| {
        for metric in metrics {
            row.panel(
                Panel::new()
                    .title(metric.label)
                    .border(BorderStyle::Rounded)
                    .size(Size::Fill),
                move |tile| {
                    tile.styled_text(
                        &metric.value,
                        TextStyle::new().fg(metric.tone.color(tile.theme())).bold(),
                    );
                },
            );
        }
    });
}

pub fn period_summary(
    ui: &mut Container<'_>,
    summary: &crate::period::PeriodSummary,
    count: usize,
) {
    let flow = summary.currencies.get("BRL").copied().unwrap_or_default();
    let income = crate::period::money_cents(flow.income, "BRL");
    let expenses = crate::period::money_cents(flow.expenses, "BRL");
    let result = crate::period::money_cents(flow.result(), "BRL");
    if ui.height() < 24 {
        ui.text(&format!("Receitas {income}  ·  Despesas {expenses}"));
        ui.styled_text(
            &format!("Saldo do período {result}  ·  {count} lançamentos"),
            TextStyle::new().fg(if flow.result() < 0 {
                ui.theme().danger
            } else {
                ui.theme().success
            }),
        );
    } else {
        metric_strip(
            ui,
            vec![
                Metric::new("Receitas", income, Tone::Positive),
                Metric::new("Despesas", expenses, Tone::Negative),
                Metric::new(
                    "Saldo do período",
                    result,
                    if flow.result() < 0 {
                        Tone::Negative
                    } else {
                        Tone::Positive
                    },
                ),
            ],
        );
        ui.label(&format!("{count} lançamentos · sem ignorados nos totais"));
    }
    for (currency, flow) in &summary.currencies {
        if currency != "BRL" {
            ui.label(&format!(
                "{currency} original · R {}  D {}  Saldo {}",
                crate::period::money_cents(flow.income, currency),
                crate::period::money_cents(flow.expenses, currency),
                crate::period::money_cents(flow.result(), currency)
            ));
        }
    }
    if summary.invalid_amounts > 0 {
        ui.label(&format!(
            "{} valores inválidos excluídos dos totais",
            summary.invalid_amounts
        ));
    }
}

#[must_use]
pub fn amount(value: Option<&str>) -> Option<f64> {
    value?
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

pub(crate) fn minor_units(value: &str) -> Option<i128> {
    let value = value.trim();
    let negative = value.starts_with('-');
    let unsigned = value.strip_prefix(['-', '+']).unwrap_or(value);
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let mut cents = whole.parse::<i128>().ok()?.checked_mul(100)?;
    let mut digits = fraction.bytes();
    cents = cents.checked_add(i128::from(digits.next().unwrap_or(b'0') - b'0') * 10)?;
    cents = cents.checked_add(i128::from(digits.next().unwrap_or(b'0') - b'0'))?;
    if digits.next().is_some_and(|digit| digit >= b'5') {
        cents = cents.checked_add(1)?;
    }
    if negative {
        cents.checked_neg()
    } else {
        Some(cents)
    }
}

#[must_use]
pub fn money_total<'a>(
    items: impl IntoIterator<Item = (Option<&'a str>, Option<&'a str>)>,
) -> String {
    let mut currency = None;
    let mut total = 0_i128;
    let mut invalid = false;
    let mut mixed = false;
    for (value, code) in items {
        let code = code.filter(|code| !code.trim().is_empty()).unwrap_or("BRL");
        if currency.is_some_and(|previous| previous != code) {
            mixed = true;
        }
        currency = Some(code);
        if let Some(next) = value
            .and_then(minor_units)
            .and_then(|value| total.checked_add(value))
        {
            total = next;
        } else {
            invalid = true;
        }
    }
    if mixed {
        return "Várias moedas".into();
    }
    let Some(currency) = currency.filter(|_| !invalid) else {
        return "Indisponível".into();
    };
    format_minor_units(total, currency)
}

fn format_minor_units(value: i128, currency: &str) -> String {
    let unsigned = value.unsigned_abs();
    let whole = (unsigned / 100).to_string();
    let mut grouped = String::new();
    for (index, digit) in whole.chars().enumerate() {
        if index > 0 && (whole.len() - index).is_multiple_of(3) {
            grouped.push('.');
        }
        grouped.push(digit);
    }
    let sign = if value < 0 { "-" } else { "" };
    format!("{sign}{grouped},{:02} {currency}", unsigned % 100)
}

#[derive(Debug, Default)]
pub struct TableSelection {
    pub hits: Interactions,
    index: usize,
    row_count: usize,
}

impl TableSelection {
    pub fn handle_mouse(&mut self, events: &[hqtui::input::InputEvent]) {
        for event in events {
            if let Some(Control::Row(index)) = self.hits.click(event) {
                self.index = index.min(self.row_count.saturating_sub(1));
            }
            if let hqtui::input::InputEvent::Mouse(mouse) = event
                && mouse.action == hqtui::input::MouseAction::Scroll
                && matches!(
                    self.hits.at(mouse.x, mouse.y),
                    Some(Control::Row(_) | Control::Scroll)
                )
            {
                self.move_by(if mouse.scroll > 0 { 1 } else { -1 });
            }
        }
    }

    pub fn set_row_count(&mut self, row_count: usize) {
        self.row_count = row_count;
        self.index = self.index.min(row_count.saturating_sub(1));
    }

    pub fn move_by(&mut self, rows: isize) {
        self.index = self
            .index
            .saturating_add_signed(rows)
            .min(self.row_count.saturating_sub(1));
    }

    #[must_use]
    pub fn selected(&self) -> Option<usize> {
        (self.row_count > 0).then_some(self.index)
    }
}

#[must_use]
pub fn theme() -> Theme {
    Theme {
        name: "visor".into(),
        background: Color::hex(0x000b_1220),
        surface: Color::hex(0x0011_1c2e),
        foreground: Color::hex(0x00e2_e8f0),
        muted: Color::hex(0x0094_a3b8),
        primary: Color::hex(0x005e_c4b6),
        accent: Color::hex(0x008e_b8d8),
        success: Color::hex(0x0034_d399),
        danger: Color::hex(0x00fb_7185),
        border: Color::hex(0x0033_4155),
        border_focused: Color::hex(0x005e_c4b6),
        title: Color::hex(0x00f1_f5f9),
        selection: Color::hex(0x0016_4e63),
        selection_text: Color::hex(0x00f0_fdfa),
        ..Theme::default()
    }
}

#[must_use]
pub fn format_currency(value: Option<&str>, currency: Option<&str>) -> String {
    value.and_then(minor_units).map_or_else(
        || "—".into(),
        |value| format_minor_units(value, currency.unwrap_or("BRL")),
    )
}

pub fn shell<'a>(
    ui: &mut Container<'a>,
    current: Screen,
    space: String,
    status: String,
    period: String,
    modal_open: bool,
    content: impl FnOnce(&mut Container<'a>) + 'a,
) {
    shell_interactive(
        ui,
        current,
        space,
        status,
        period,
        modal_open,
        Interactions::default(),
        content,
    );
}

pub fn shell_interactive<'a>(
    ui: &mut Container<'a>,
    current: Screen,
    space: String,
    status: String,
    period: String,
    modal_open: bool,
    hits: Interactions,
    content: impl FnOnce(&mut Container<'a>) + 'a,
) {
    let compact = ui.width() <= 100;
    let wide = ui.width() >= 110 && ui.height() >= 24;
    let grouped = ui.height() >= 30;
    let short = ui.height() < 14;
    let muted = ui.theme().muted;
    let accent = ui.theme().accent;
    let surface = ui.theme().surface;
    let header_hits = hits.clone();
    ui.column(Column::new().gap(0), move |col| {
        col.row(
            Row::new()
                .size(Size::Cells(if short { 1 } else { 2 }))
                .gap(2)
                .background(surface),
            move |header| {
                header.column(Column::new().size(Size::Cells(8)), move |brand| {
                    brand.styled_text(" VISOR", TextStyle::new().fg(accent).bold());
                });
                header.column(Column::new().size(Size::Fill), move |title| {
                    let label = format!("{} ▾ Telas  ·  {space}", current.label());
                    if modal_open {
                        title.text(&label);
                    } else {
                        title.sized(Size::Cells(1), move |line| {
                            line.mouse_button(
                                ButtonOptions::new(label).variant(ButtonVariant::Ghost),
                                "nav:picker",
                                &header_hits,
                            );
                        });
                    }
                });
            },
        );
        col.row(Row::new().size(Size::Fill).gap(1), move |body| {
            let nav_hits = hits.clone();
            if wide {
                body.panel(
                    Panel::new()
                        .title("Seu dinheiro")
                        .border(BorderStyle::Rounded)
                        .size(Size::Cells(26)),
                    move |nav| {
                        for screen in Screen::all() {
                            let nav_hits = nav_hits.clone();
                            if grouped {
                                let group = match screen {
                                    Screen::Overview => Some("FINANÇAS"),
                                    Screen::Categories => Some("PLANEJAMENTO"),
                                    Screen::Notifications => Some("ESPAÇO"),
                                    _ => None,
                                };
                                if let Some(group) = group {
                                    nav.styled_text(group, TextStyle::new().fg(muted));
                                }
                            }
                            let active = screen == current;
                            let label = format!(
                                "{} {} {}",
                                if active { "▎" } else { " " },
                                screen.shortcut_key(),
                                screen.label()
                            );
                            if modal_open {
                                nav.styled_text(
                                    &label,
                                    TextStyle::new().fg(if active { accent } else { muted }),
                                );
                            } else {
                                nav.sized(Size::Cells(1), move |item| {
                                    item.mouse_button(
                                        ButtonOptions::new(label).variant(if active {
                                            ButtonVariant::Primary
                                        } else {
                                            ButtonVariant::Ghost
                                        }),
                                        &format!("nav:{}", screen.index()),
                                        &nav_hits,
                                    );
                                });
                            }
                        }
                    },
                );
            }
            body.column(Column::new().size(Size::Fill).gap(0), move |main| {
                if current.uses_period() {
                    period_bar(main, period, hits, !modal_open);
                }
                main.column(Column::new().size(Size::Fill), content);
            });
        });
        col.row(Row::new().size(Size::Cells(1)), move |footer| {
            footer.styled_text(&status, TextStyle::new().fg(muted));
        });
        col.row(
            Row::new().size(Size::Cells(1)).background(surface),
            move |footer| {
                let hint = if modal_open {
                    " Tab Campo  Ctrl+S Salvar  Esc Voltar".to_owned()
                } else if compact {
                    " [ ] Mês  T Hoje  p Período  Tab Telas  ? Ajuda  q Sair".to_owned()
                } else {
                    let local = match current {
                        Screen::Transactions => "/ Buscar  n Nova  Enter Detalhes",
                        Screen::AccountsCards => "h Ocultas  j/k Conta/cartão",
                        Screen::Recurring | Screen::Payables | Screen::Bills => {
                            "←/→ Grupo  j/k Selecionar"
                        }
                        Screen::Categories | Screen::Goals | Screen::Tags => {
                            "n Criar  e Editar  d Excluir"
                        }
                        Screen::Settings => "s Espaço  l Sair da conta",
                        _ => "j/k Selecionar",
                    };
                    format!(" [ ] Mês  T Hoje  p Período  Tab Telas  {local}  ? Ajuda  r Atualizar")
                };
                footer.styled_text(&hint, TextStyle::new().fg(muted));
            },
        );
    });
}

pub fn period_bar(ui: &mut Container<'_>, label: String, hits: Interactions, enabled: bool) {
    ui.sized(Size::Cells(3), move |ui| {
        ui.draw(move |surface| {
            use hqtui::{
                buffer::{Attrs, Style},
                surface::TextOptions,
                unicode::{Align, fit, string_width},
            };
            surface.fill(&Style::new().with_bg(surface.theme.surface));
            let label_width = string_width(&label).min(surface.width().saturating_sub(12));
            let center_width = label_width + 8;
            let x = surface.width().saturating_sub(center_width) / 2;
            let y = surface.height().min(2).saturating_sub(1);
            let text = format!("◀   {}   ▶", fit(&label, label_width, Align::Center));
            surface.text(
                coord(x),
                coord(y),
                &text,
                &TextOptions {
                    fg: Some(surface.theme.accent),
                    attrs: Some(Attrs::BOLD),
                    ..Default::default()
                },
            );
            let shortcuts = "[ ] T p";
            if x + center_width + 2 + shortcuts.len() <= surface.width() {
                surface.text(
                    coord(x + center_width + 2),
                    coord(y),
                    shortcuts,
                    &TextOptions::new().fg(surface.theme.muted),
                );
            } else {
                surface.text(
                    coord(surface.width().saturating_sub(shortcuts.len()) / 2),
                    coord(y + 1),
                    shortcuts,
                    &TextOptions::new().fg(surface.theme.muted),
                );
            }
            if enabled {
                hits.add(
                    &surface.sub(coord(x), coord(y), 3, 1),
                    Control::Button("period:previous".into()),
                );
                hits.add(
                    &surface.sub(coord(x + 4), coord(y), label_width, 1),
                    Control::Button("period:picker".into()),
                );
                hits.add(
                    &surface.sub(coord(x + center_width - 3), coord(y), 3, 1),
                    Control::Button("period:next".into()),
                );
            }
        });
    });
}

#[must_use]
pub fn next_row(selected: Option<usize>, count: usize, direction: i8) -> Option<usize> {
    if count == 0 {
        return None;
    }
    Some(match selected {
        None => 0,
        Some(index) if direction < 0 => index.saturating_sub(1).min(count - 1),
        Some(index) => index.saturating_add(1).min(count - 1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use hqtui::testing::render_to_text;

    #[test]
    fn shared_period_bar_is_centered_above_content_and_uses_rendered_targets() {
        use crate::interaction::test_support::click_text;
        for (width, height) in [(120, 40), (80, 24)] {
            for screen in Screen::all()
                .into_iter()
                .filter(|screen| screen.uses_period())
            {
                let hits = Interactions::default();
                let text = render_to_text(width, height, "dark", |ui| {
                    shell_interactive(
                        ui,
                        screen,
                        "Pessoal".into(),
                        "Pronto".into(),
                        "Outubro 2026".into(),
                        false,
                        hits.clone(),
                        |content| {
                            content.text("Receitas do período");
                        },
                    );
                });
                let period_row = text
                    .lines()
                    .position(|line| line.contains("Outubro 2026"))
                    .unwrap();
                let summary_row = text
                    .lines()
                    .position(|line| line.contains("Receitas do período"))
                    .unwrap();
                assert!(period_row >= 2 && period_row < summary_row, "{text}");
                let line = text.lines().nth(period_row).unwrap();
                let label_x =
                    hqtui::unicode::string_width(&line[..line.find("Outubro 2026").unwrap()]);
                let content_left = if width >= 110 { 27 } else { 0 };
                let expected = content_left + (width - content_left) / 2;
                assert!((label_x + 5).abs_diff(expected) <= 1, "{text}");
                for (label, action) in [
                    ("◀", "period:previous"),
                    ("Outubro 2026", "period:picker"),
                    ("▶", "period:next"),
                ] {
                    assert!(
                        hits.pressed(&[click_text(&text, label)], action),
                        "{label}\n{text}"
                    );
                }
                assert!(hits.pressed(&[click_text(&text, "Telas")], "nav:picker"));
            }
        }
    }

    #[test]
    fn totals_preserve_cents_and_do_not_invent_missing_money() {
        assert_eq!(
            money_total([(Some("0.10"), None), (Some("0.20"), None)]),
            "0,30 BRL"
        );
        assert_eq!(
            money_total([(Some("9999999999999999.99"), None), (Some("0.01"), None)]),
            "10.000.000.000.000.000,00 BRL"
        );
        assert_eq!(
            money_total([(Some("1"), Some("BRL")), (Some("2"), Some("USD"))]),
            "Várias moedas"
        );
        assert_eq!(
            money_total([(Some("1"), None), (None, None)]),
            "Indisponível"
        );
        assert_eq!(money_total([(Some("NaN"), None)]), "Indisponível");
        assert_eq!(money_total([]), "Indisponível");
        assert_eq!(money_total([(Some("-1.005"), None)]), "-1,01 BRL");
        assert_eq!(amount(Some("inf")), None);
    }

    #[test]
    fn scrolling_reports_stop_at_boundaries_before_the_next_frame() {
        use crate::{api::ApiClient, auth::Credential, screens};
        type View = fn(&mut Container<'_>, &ApiClient, &mut TableSelection);

        let mut server = mockito::Server::new();
        let responses = [
            (
                "/v1/me",
                serde_json::json!({"id":"u","name":"Daniel","email":"daniel@example.test"}),
            ),
            (
                "/v1/spaces",
                serde_json::json!({"data":[{"id":"s","name":"Pessoal"}]}),
            ),
            (
                "/v1/spaces/s/overview",
                serde_json::json!({
                    "bank_balance":"12345.67",
                    "period":{"income":"5000","expenses":"1200","result":"3800"},
                    "projection":{"current_balance":"3800","expected_income":"500","projected_balance":"3930"}
                }),
            ),
            (
                "/v1/spaces/s/accounts",
                serde_json::json!({"data":[{"id":"a","name":"Conta","balance":"12345.67"}]}),
            ),
            (
                "/v1/spaces/s/cards",
                serde_json::json!({"data":[{"id":"c","name":"Cartão","credit_limit":"5000","available_credit_limit":"4000"}]}),
            ),
            (
                "/v1/spaces/s/investments",
                serde_json::json!({"data":[{"id":"i","name":"Fundo","balance":"1111"}]}),
            ),
        ];
        let mocks: Vec<_> = responses
            .into_iter()
            .map(|(path, response)| {
                server
                    .mock("GET", path)
                    .with_status(200)
                    .with_body(response.to_string())
                    .expect(1)
                    .create()
            })
            .collect();
        let api = ApiClient::for_test(
            &server.url(),
            Credential::PersonalToken {
                token: "test-only".into(),
            },
            "s",
        )
        .unwrap();
        let views: [(&str, View); 4] = [
            ("projection", screens::projection::render_scrolled),
            ("net_worth", screens::net_worth::render_scrolled),
            ("accounts_cards", |ui, api, selection| {
                screens::accounts_cards::render_scrolled(ui, api, false, selection);
            }),
            ("settings", |ui, api, selection| {
                screens::settings::render_scrolled(
                    ui,
                    api,
                    screens::settings::SettingsModal::None,
                    selection,
                );
            }),
        ];
        for (name, view) in views {
            let mut selection = TableSelection::default();
            api.set_fetch_allowed(true);
            render_to_text(60, 18, "dark", |ui| view(ui, &api, &mut selection));
            api.set_fetch_allowed(false);
            selection.move_by(isize::MAX);
            let Some(bottom) = selection.selected() else {
                selection.move_by(1);
                assert_eq!(selection.selected(), None, "{name}");
                continue;
            };

            for _ in 0..100 {
                selection.move_by(1);
            }
            selection.move_by(-1);
            assert_eq!(
                selection.selected(),
                Some(bottom.saturating_sub(1)),
                "{name}"
            );
            render_to_text(60, 18, "dark", |ui| view(ui, &api, &mut selection));
            assert_eq!(
                selection.selected(),
                Some(bottom.saturating_sub(1)),
                "{name}"
            );
            for _ in 0..100 {
                selection.move_by(-1);
            }
            selection.move_by(1);
            assert_eq!(selection.selected(), Some(bottom.min(1)), "{name}");
        }
        for mock in mocks {
            mock.assert();
        }
    }

    #[test]
    fn selection_tracks_shrinking_and_empty_results() {
        let mut selection = TableSelection::default();
        selection.set_row_count(12);
        selection.move_by(isize::MAX);
        assert_eq!(selection.selected(), Some(11));
        selection.move_by(1);
        assert_eq!(selection.selected(), Some(11));
        selection.move_by(-1);
        assert_eq!(selection.selected(), Some(10));
        selection.set_row_count(3);
        selection.move_by(-1);
        assert_eq!(selection.selected(), Some(1));
        selection.set_row_count(0);
        selection.move_by(1);
        assert_eq!(selection.selected(), None);
        selection.set_row_count(2);
        assert_eq!(selection.selected(), Some(0));
    }

    #[test]
    fn currency_groups_and_carries_rounding() {
        assert_eq!(format_currency(Some("1234.56"), None), "1.234,56 BRL");
        assert_eq!(format_currency(Some("999.999"), None), "1.000,00 BRL");
        assert_eq!(format_currency(Some("-12.50"), None), "-12,50 BRL");
        assert_eq!(format_currency(None, None), "—");
        assert_eq!(format_currency(Some("NaN"), None), "—");
        assert_eq!(
            format_currency(Some("9999999999999999.99"), None),
            "9.999.999.999.999.999,99 BRL"
        );
    }

    #[test]
    fn selected_goal_and_recurring_rows_stay_visible_in_small_terminals() {
        let api = crate::api::ApiClient::for_test(
            "http://127.0.0.1:1",
            crate::auth::Credential::PersonalToken {
                token: "test-only".into(),
            },
            "space",
        )
        .unwrap();
        let mut goals = crate::screens::goals::GoalsScreen::new();
        goals.loaded = true;
        goals.selected_index = Some(29);
        goals.goals = (0..30).map(|index| serde_json::from_value(serde_json::json!({"id": index.to_string(), "name": format!("Meta {index:02}"), "target_amount": "1234.56"})).unwrap()).collect();
        let mut recurring = crate::screens::recurring::RecurringScreen::new();
        recurring.loaded = true;
        recurring.selected_index = Some(29);
        recurring.subscriptions = (0..30).map(|index| serde_json::from_value(serde_json::json!({"id": index.to_string(), "name": format!("Conta {index:02}"), "amount": "1234.56"})).unwrap()).collect();
        for (width, height) in [(80, 24), (60, 18)] {
            let text = render_to_text(width, height, "dark", |ui| {
                shell(
                    ui,
                    Screen::Goals,
                    "Pessoal".into(),
                    "Sincronizado".into(),
                    "Outubro 2026".into(),
                    false,
                    |panel| crate::screens::goals::render_with_state(panel, &mut goals, &api),
                );
            });
            assert!(text.contains("Meta 29"), "{width}x{height}\n{text}");
            let text = render_to_text(width, height, "dark", |ui| {
                shell(
                    ui,
                    Screen::Recurring,
                    "Pessoal".into(),
                    "Sincronizado".into(),
                    "Outubro 2026".into(),
                    false,
                    |panel| {
                        crate::screens::recurring::render_with_state(panel, &mut recurring, &api);
                    },
                );
            });
            assert!(text.contains("Conta 29"), "{width}x{height}\n{text}");
        }
    }

    #[test]
    fn pasted_confirmation_letters_are_only_form_data() {
        use crate::screens::{categories::*, goals::*, recurring::*, tags::*};
        let mut tags = TagsScreen::new();
        let mut categories = CategoriesScreen::new();
        let mut goals = GoalsScreen::new();
        let mut recurring = RecurringScreen::new();
        tags.modal_state = TagModalState::Create;
        categories.modal_state = CategoryModalState::Create;
        goals.modal_state = GoalModalState::Create;
        recurring.modal_state = RecurringModalState::CreateSubscription;
        tags.paste_text("ynq123?\n");
        categories.paste_text("ynq123?\n");
        goals.paste_text("ynq123?\n");
        recurring.paste_text("ynq123?\n");
        assert_eq!(tags.form_name, "ynq123?");
        assert_eq!(categories.form_name, "ynq123?");
        assert_eq!(goals.form_name, "ynq123?");
        assert_eq!(recurring.form_name, "ynq123?");
        tags.modal_state = TagModalState::DeleteConfirm;
        categories.modal_state = CategoryModalState::DeleteConfirm;
        goals.modal_state = GoalModalState::DeleteConfirm;
        recurring.modal_state = RecurringModalState::DeleteConfirm;
        tags.paste_text("ynq123?\n");
        categories.paste_text("ynq123?\n");
        goals.paste_text("ynq123?\n");
        recurring.paste_text("ynq123?\n");
        assert_eq!(tags.modal_state, TagModalState::DeleteConfirm);
        assert_eq!(categories.modal_state, CategoryModalState::DeleteConfirm);
        assert_eq!(goals.modal_state, GoalModalState::DeleteConfirm);
        assert_eq!(recurring.modal_state, RecurringModalState::DeleteConfirm);
        assert_eq!(tags.form_name, "ynq123?");
        assert_eq!(categories.form_name, "ynq123?");
        assert_eq!(goals.form_name, "ynq123?");
        assert_eq!(recurring.form_name, "ynq123?");
    }

    #[test]
    fn shell_keeps_content_and_footer_visible_at_common_sizes() {
        for (width, height) in [(80, 24), (120, 40), (60, 20)] {
            let text = render_to_text(width, height, "dark", |ui| {
                shell(
                    ui,
                    Screen::Transactions,
                    "Pessoal".into(),
                    "Sincronizado".into(),
                    "Outubro 2026".into(),
                    false,
                    |content| {
                        content.text("Conteúdo da tela");
                    },
                );
            });
            assert!(
                text.contains("Conteúdo da tela"),
                "{width}x{height}\n{text}"
            );
            assert!(text.contains("Sincronizado"));
            assert!(text.contains("Pessoal"));
            assert!(text.contains("Ajuda"));
        }
    }
}
