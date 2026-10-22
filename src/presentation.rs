use hqtui::prelude::*;

use crate::keymap::Screen;

#[must_use]
pub fn theme() -> Theme {
    Theme {
        name: "visor".into(),
        background: Color::hex(0x000b_1220),
        surface: Color::hex(0x0011_1c2e),
        foreground: Color::hex(0x00e2_e8f0),
        muted: Color::hex(0x0094_a3b8),
        primary: Color::hex(0x0034_d399),
        accent: Color::hex(0x0038_bdf8),
        success: Color::hex(0x0034_d399),
        danger: Color::hex(0x00fb_7185),
        border: Color::hex(0x0033_4155),
        border_focused: Color::hex(0x0034_d399),
        title: Color::hex(0x00f1_f5f9),
        selection: Color::hex(0x0016_4e63),
        selection_text: Color::hex(0x00f0_fdfa),
        ..Theme::default()
    }
}

#[must_use]
pub fn format_currency(value: Option<&str>, currency: Option<&str>) -> String {
    let Some(raw) = value.filter(|value| !value.is_empty()) else {
        return "—".into();
    };
    let code = currency.unwrap_or("BRL");
    let Ok(amount) = raw.parse::<f64>() else {
        return format!("{raw} {code}");
    };
    if !amount.is_finite() {
        return "—".into();
    }
    let rounded = format!("{:.2}", amount.abs());
    let (whole, fraction) = rounded.split_once('.').unwrap_or((&rounded, "00"));
    let mut grouped = String::new();
    for (index, digit) in whole.chars().enumerate() {
        if index > 0 && (whole.len() - index).is_multiple_of(3) {
            grouped.push('.');
        }
        grouped.push(digit);
    }
    let sign = if amount < 0.0 { "-" } else { "" };
    format!("{sign}{grouped},{fraction} {code}")
}

pub fn shell<'a>(
    ui: &mut Container<'a>,
    current: Screen,
    space: String,
    status: String,
    modal_open: bool,
    content: impl FnOnce(&mut Container<'a>) + 'a,
) {
    let compact = ui.width() < 80;
    let wide = ui.width() >= 110 && ui.height() >= 24;
    let muted = ui.theme().muted;
    let accent = ui.theme().accent;
    ui.column(Column::new().gap(0), move |col| {
        col.row(Row::new().size(Size::Cells(2)).gap(1), move |header| {
            header.styled_text(" VISOR", TextStyle::new().fg(accent).bold());
            header.text(&format!("{}  ·  {space}", current.label()));
        });
        col.row(Row::new().size(Size::Fill).gap(1), move |body| {
            if wide {
                body.column(Column::new().size(Size::Cells(26)), move |sidebar| {
                    sidebar.panel(Panel::new().title("Navegação"), move |nav| {
                        for screen in Screen::all() {
                            let active = screen == current;
                            let label = format!("{} {}", screen.shortcut_key(), screen.label());
                            if modal_open {
                                nav.styled_text(
                                    &label,
                                    TextStyle::new().fg(if active { accent } else { muted }),
                                );
                            } else {
                                nav.button(
                                    ButtonOptions::new(label).variant(if active {
                                        ButtonVariant::Primary
                                    } else {
                                        ButtonVariant::Ghost
                                    }),
                                    &format!("nav:{}", screen.index()),
                                );
                            }
                        }
                    });
                });
            }
            body.column(Column::new().size(Size::Fill), move |main| {
                main.panel(Panel::new().title(current.label()), content);
            });
        });
        col.row(Row::new().size(Size::Cells(1)), move |footer| {
            footer.styled_text(&status, TextStyle::new().fg(muted));
        });
        col.row(Row::new().size(Size::Cells(1)), move |footer| {
            let hint = if modal_open {
                " Tab/Shift+Tab Campo  Ctrl+S Salvar  Esc Voltar"
            } else if compact {
                " Tab Telas  ? Ajuda  r Atualizar  q Sair"
            } else {
                " Tab/Shift+Tab Telas  1–0/-/t/N/x/=/D  ? Ajuda  r Atualizar  q Sair"
            };
            footer.styled_text(hint, TextStyle::new().fg(muted));
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use hqtui::testing::render_to_text;

    #[test]
    fn currency_groups_and_carries_rounding() {
        assert_eq!(format_currency(Some("1234.56"), None), "1.234,56 BRL");
        assert_eq!(format_currency(Some("999.999"), None), "1.000,00 BRL");
        assert_eq!(format_currency(Some("-12.50"), None), "-12,50 BRL");
        assert_eq!(format_currency(None, None), "—");
        assert_eq!(format_currency(Some("NaN"), None), "—");
    }

    #[test]
    fn modal_mouse_buttons_match_the_rendered_cells() {
        use hqtui::input::{InputEvent, MouseAction, MouseButton, MouseEvent};
        let options = ModalOptions {
            title: Some("Confirmar".into()),
            message: Some("Deseja continuar?".into()),
            buttons: vec![ModalButton::new("Cancelar"), ModalButton::new("Excluir")],
            ..Default::default()
        };
        let screen = hqtui::testing::render_to_screen(80, 24, "dark", |ui| {
            ui.modal(options.clone());
        });
        for (label, action) in [("Cancelar", "escape"), ("Excluir", "enter")] {
            let (x, y) = screen.find(label).expect("rendered modal button");
            let event = InputEvent::Mouse(MouseEvent {
                action: MouseAction::Press,
                button: MouseButton::Left,
                x,
                y,
                scroll: 0,
                ctrl: false,
                alt: false,
                shift: false,
            });
            let events = modal_events(
                &[event],
                hqtui::terminal::TerminalSize {
                    columns: 80,
                    rows: 24,
                },
                Some(&options),
            );
            assert!(
                events
                    .iter()
                    .any(|event| matches!(event, InputEvent::Key(key) if key.name == action)),
                "{label} at {x},{y}\n{}",
                screen.text()
            );
        }
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
                    false,
                    |panel| {
                        crate::screens::recurring::render_with_state(panel, &mut recurring, &api)
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

#[must_use]
pub fn modal_events(
    events: &[hqtui::input::InputEvent],
    size: hqtui::terminal::TerminalSize,
    dialog: Option<&ModalOptions>,
) -> Vec<hqtui::input::InputEvent> {
    use hqtui::input::{InputEvent, KeyEvent, MouseAction, MouseButton};
    let mut result = events.to_vec();
    let Some(dialog) = dialog else { return result };
    if dialog.buttons.is_empty() {
        return result;
    }
    let width = dialog
        .width
        .unwrap_or(48)
        .min(size.columns.saturating_sub(2));
    let lines = dialog.message.as_deref().map_or(0, |message| {
        hqtui::unicode::wrap(message, width.saturating_sub(4)).len()
    });
    let height = dialog
        .height
        .unwrap_or(lines + 5)
        .min(size.rows.saturating_sub(2));
    let left = size.columns.saturating_sub(width) / 2;
    let top = size.rows.saturating_sub(height) / 2;
    let widths: Vec<usize> = dialog
        .buttons
        .iter()
        .map(|button| hqtui::unicode::string_width(&button.label) + 4)
        .collect();
    let total = widths.iter().sum::<usize>() + widths.len().saturating_sub(1) * 2;
    let mut x = left + 1 + width.saturating_sub(2).saturating_sub(total) / 2;
    let y = top + height.saturating_sub(3);
    for (index, button_width) in widths.into_iter().enumerate() {
        if events.iter().any(|event| matches!(event, InputEvent::Mouse(mouse) if mouse.action == MouseAction::Press && mouse.button == MouseButton::Left && mouse.y == y && mouse.x >= x && mouse.x < x + button_width)) {
            let name = if index == 0 { "escape" } else { "enter" };
            result.push(InputEvent::Key(KeyEvent { name: name.into(), key: name.into(), raw: String::new(), char: None, ctrl: false, alt: false, shift: false }));
        }
        x += button_width + 2;
    }
    result
}
