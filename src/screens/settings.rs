use hqtui::prelude::*;

use crate::api::ApiClient;

/// Whether the logout confirmation modal is currently visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsModal {
    None,
    LogoutConfirm,
}

pub fn render(ui: &mut Container<'_>, api: &ApiClient, modal: SettingsModal) {
    render_scrolled(ui, api, modal, 0);
}

pub fn render_scrolled(
    ui: &mut Container<'_>,
    api: &ApiClient,
    modal: SettingsModal,
    selected: usize,
) {
    let mut rows = Vec::new();
    match api.me() {
        Ok(me) => {
            for (label, value) in [
                ("Nome", Some(me.name)),
                ("Email", Some(me.email)),
                ("Locale", me.locale),
                ("Moeda", me.display_currency_code),
                ("Plano", me.plan),
            ] {
                rows.push(TableRow::new([
                    label.to_string(),
                    value.unwrap_or_else(|| "—".into()),
                ]));
            }
        }
        Err(error) => rows.push(TableRow::new(["Perfil".into(), format!("{error}")])),
    }
    let space = api.spaces().map_or_else(
        |error| error.to_string(),
        |spaces| {
            spaces
                .into_iter()
                .find(|space| Some(space.id.as_str()) == api.space_id())
                .map_or_else(|| "Nenhum espaço ativo".into(), |space| space.name)
        },
    );
    rows.push(TableRow::new(["Espaço".into(), space]));
    let credential = api.credential();
    for (label, value) in [
        ("Autenticação", credential.kind_label().to_string()),
        ("Token", credential.redacted()),
        ("Expiração", credential.expiry_display()),
        ("Armazenamento", "~/.config/visor/credentials.json".into()),
    ] {
        rows.push(TableRow::new([label.to_string(), value]));
    }
    let selected = Some(selected.min(rows.len().saturating_sub(1)));
    ui.column(Column::new().gap(1), move |col| {
        col.row(Row::new().size(Size::Cells(1)).gap(1), move |row| {
            row.button(ButtonOptions::new("Trocar espaço (s)"), "change_space");
            row.button(
                ButtonOptions::new("Sair da conta (l)").variant(ButtonVariant::Danger),
                "logout",
            );
        });
        col.table(
            TableOptions {
                columns: vec![
                    TableColumn::new("Campo").width(Size::Cells(14)),
                    TableColumn::new("Valor").width(Size::Fill),
                ],
                rows,
                selected,
                follow_selection: true,
                zebra: true,
                ..Default::default()
            },
            "settings:table",
        );
        col.text("j/k ou ↑/↓ Rolar todas as informações");
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
        let first = hqtui::testing::render_to_text(56, 10, "dark", |ui| {
            render_scrolled(ui, &api, SettingsModal::None, 0)
        });
        assert!(first.contains("Daniel"), "{first}");
        assert!(first.contains("daniel@example.test"), "{first}");
        let last = hqtui::testing::render_to_text(56, 10, "dark", |ui| {
            render_scrolled(ui, &api, SettingsModal::None, 9)
        });
        assert!(last.contains("credentials.json"), "{last}");
        me.assert();
        spaces.assert();
    }
}
