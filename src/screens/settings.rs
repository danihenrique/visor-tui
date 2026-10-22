use hqtui::prelude::*;

use crate::api::ApiClient;
use crate::presentation::TableSelection;

/// Whether the logout confirmation modal is currently visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsModal {
    None,
    LogoutConfirm,
}

pub fn render(ui: &mut Container<'_>, api: &ApiClient, modal: SettingsModal) {
    render_scrolled(ui, api, modal, &mut TableSelection::default());
}

pub fn render_scrolled(
    ui: &mut Container<'_>,
    api: &ApiClient,
    modal: SettingsModal,
    selection: &mut TableSelection,
) {
    let profile = api.me();
    let space = api.spaces().map_or_else(
        |error| error.to_string(),
        |spaces| {
            spaces
                .into_iter()
                .find(|space| Some(space.id.as_str()) == api.space_id())
                .map_or_else(|| "Nenhum espaço ativo".into(), |space| space.name)
        },
    );
    let credential = api.credential();
    let authentication = credential.kind_label().to_string();
    let expiry = credential.expiry_display();
    let compact = ui.height() < 19;
    selection.set_row_count(3);
    let selected = selection.selected().unwrap_or(0);
    let muted = ui.theme().muted;
    let accent = ui.theme().accent;
    let border = ui.theme().border;
    ui.column(Column::new().gap(1), move |col| {
        if !compact || selected == 0 {
            let title = profile.as_ref().map_or_else(
                |_| "Perfil".into(),
                |me| format!("Perfil · {}", me.plan.as_deref().unwrap_or("Visor")),
            );
            col.panel(
                Panel {
                    border_color: Some(if selected == 0 { accent } else { border }),
                    ..Panel::new().title(title).size(Size::Cells(5))
                },
                move |panel| match profile {
                    Ok(me) => {
                        panel.styled_text(&me.name, TextStyle::new().bold());
                        panel.styled_text(&me.email, TextStyle::new().fg(muted));
                        panel.styled_text(
                            &format!(
                                "Idioma {}   ·   Moeda {}",
                                me.locale.as_deref().unwrap_or("Não informado"),
                                me.display_currency_code
                                    .as_deref()
                                    .unwrap_or("Não informada")
                            ),
                            TextStyle::new().fg(muted),
                        );
                    }
                    Err(error) => {
                        panel.text(&format!("Não foi possível carregar o perfil. {error}"));
                    }
                },
            );
        }
        if !compact || selected == 1 {
            col.panel(
                Panel {
                    border_color: Some(if selected == 1 { accent } else { border }),
                    ..Panel::new().title("Espaço ativo").size(Size::Cells(4))
                },
                move |panel| {
                    panel.styled_text(&space, TextStyle::new().fg(accent).bold());
                    panel.button(
                        ButtonOptions::new("s Trocar espaço").variant(ButtonVariant::Ghost),
                        "change_space",
                    );
                },
            );
        }
        if !compact || selected == 2 {
            col.panel(
                Panel {
                    border_color: Some(if selected == 2 { accent } else { border }),
                    ..Panel::new()
                        .title("Acesso e segurança")
                        .size(Size::Cells(7))
                },
                move |panel| {
                    panel.styled_text(&authentication, TextStyle::new().bold());
                    panel.styled_text(&format!("Validade {expiry}"), TextStyle::new().fg(muted));
                    panel.label("Credencial protegida");
                    panel.label("~/.config/visor/credentials.json");
                    panel.button(
                        ButtonOptions::new("l Sair da conta").variant(ButtonVariant::Ghost),
                        "logout",
                    );
                },
            );
        }
        if compact {
            col.styled_text(
                &format!(
                    "Seção {}/3  ·  j/k Perfil, espaço e acesso  ·  s Trocar  l Sair",
                    selected + 1
                ),
                TextStyle::new().fg(muted),
            );
        }
    });
    if modal == SettingsModal::LogoutConfirm {
        ui.modal(logout_dialog());
    }
}

#[must_use]
pub fn logout_dialog() -> ModalOptions {
    ModalOptions {
            title: Some("Confirmar Logout".to_string()),
            message: Some(
                "Tem certeza que deseja sair? Isso removerá as credenciais armazenadas.\n\nEnter Confirmar | Esc Cancelar"
                    .to_string(),
            ),
            buttons: vec![
                ModalButton::new("Cancelar"),
                ModalButton::new("Sair"),
            ],
            ..Default::default()
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_and_authentication_remain_reachable_in_a_small_terminal() {
        let mut server = mockito::Server::new();
        let me = server
            .mock("GET", "/v1/me")
            .with_status(200)
            .with_body(r#"{"id":"u","name":"Daniel","email":"daniel@example.test","plan":"Pro"}"#)
            .create();
        let spaces = server
            .mock("GET", "/v1/spaces")
            .with_status(200)
            .with_body(r#"{"data":[{"id":"s","name":"Pessoal"}]}"#)
            .create();
        let api = ApiClient::for_test(
            &server.url(),
            crate::auth::Credential::PersonalToken {
                token: "test-only".into(),
            },
            "s",
        )
        .unwrap();
        let mut selection = TableSelection::default();
        let first = hqtui::testing::render_to_text(56, 10, "dark", |ui| {
            render_scrolled(ui, &api, SettingsModal::None, &mut selection);
        });
        assert!(first.contains("Daniel"), "{first}");
        assert!(first.contains("daniel@example.test"), "{first}");
        selection.move_by(isize::MAX);
        let last = hqtui::testing::render_to_text(56, 10, "dark", |ui| {
            render_scrolled(ui, &api, SettingsModal::None, &mut selection);
        });
        assert!(last.contains("credentials.json"), "{last}");
        me.assert();
        spaces.assert();
    }
}
