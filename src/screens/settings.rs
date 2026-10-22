use hqtui::prelude::*;
use hqtui::color::Color;
use hqtui::widgets::{
    ButtonOptions, ButtonVariant, DividerOptions, KeyValueOptions, KeyValueRow, ModalButton,
    ModalOptions, TextStyle,
};

use crate::api::{ApiClient, ApiError};

const COLOR_NEGATIVE: Color = Color::rgb(239, 68, 68);
const COLOR_ACCENT: Color = Color::rgb(34, 197, 94);
const COLOR_NEUTRAL: Color = Color::rgb(148, 163, 184);

/// Whether the logout confirmation modal is currently visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsModal {
    None,
    LogoutConfirm,
}

fn draw_error(ui: &mut Container<'_>, err: &ApiError) {
    let err_msg = format!("Falha ao carregar configurações: {err}");
    ui.panel(Panel::new().title("Erro"), move |p| {
        p.styled_text(&err_msg, TextStyle::new().fg(COLOR_NEGATIVE));
        p.label("Verifique sua conexão e credenciais.");
    });
}

pub fn render(ui: &mut Container<'_>, api: &ApiClient, modal: SettingsModal) {
    // Load user profile
    let me = match api.me() {
        Ok(data) => data,
        Err(ref e) => {
            draw_error(ui, e);
            return;
        }
    };

    // Load spaces
    let spaces = match api.spaces() {
        Ok(data) => data,
        Err(ref e) => {
            draw_error(ui, e);
            return;
        }
    };

    // Pre-compute all strings before entering closures
    let user_name = me.name.clone();
    let user_email = me.email.clone();
    let user_locale = me.locale.clone().unwrap_or_else(|| "—".to_string());
    let user_currency = me.display_currency_code.clone().unwrap_or_else(|| "—".to_string());
    let user_plan = me.plan.clone().unwrap_or_else(|| "—".to_string());
    let active_space_id = me.active_space_id.clone();

    // Find active space name
    let active_space_name = active_space_id
        .as_deref()
        .and_then(|sid| spaces.iter().find(|s| s.id == sid)).map_or_else(|| "Nenhum espaço ativo".to_string(), |s| format!("* {}", s.name));

    let has_active_space = active_space_id.is_some();

    // Build key-value rows for profile panel
    let profile_rows = vec![
        KeyValueRow::new("Nome", user_name),
        KeyValueRow::new("Email", user_email),
        KeyValueRow::new("Locale", user_locale),
        KeyValueRow::new("Moeda", user_currency),
        KeyValueRow::new("Plano", user_plan),
    ];

    // Auth status details from credential
    let cred = api.credential();
    let token_type = cred.kind_label().to_string();
    let redacted_token = cred.redacted();
    let expiry_display = cred.expiry_display();
    let store_type = "File (~/.config/visor/credentials.json)".to_string();

    let auth_rows = vec![
        KeyValueRow::new("Tipo", token_type),
        KeyValueRow::new("Token", redacted_token),
        KeyValueRow::new("Expiração", expiry_display),
        KeyValueRow::new("Armazenamento", store_type),
    ];

    ui.column(Column::new().gap(1), move |col| {
        // Perfil panel
        col.panel(Panel::new().title("Perfil"), move |p| {
            p.key_values(KeyValueOptions {
                rows: profile_rows.clone(),
                ..Default::default()
            });
        });

        // Espaço Ativo panel
        col.panel(Panel::new().title("Espaço Ativo"), move |p| {
            if has_active_space {
                p.styled_text(&active_space_name, TextStyle::new().fg(COLOR_ACCENT));
            } else {
                p.styled_text(&active_space_name, TextStyle::new().fg(COLOR_NEUTRAL));
            }
        });

        // Autenticação panel
        col.panel(Panel::new().title("Autenticação"), move |p| {
            p.key_values(KeyValueOptions {
                rows: auth_rows.clone(),
                ..Default::default()
            });
        });

        // Divider before logout
        col.row(Row::new(), move |r| {
            r.divider(DividerOptions::default());
        });

        // Sair button
        col.row(Row::new(), move |r| {
            r.button(
                ButtonOptions::new("Sair").variant(ButtonVariant::Danger),
                "logout",
            );
        });
    });

    // Draw logout confirmation modal overlay
    if modal == SettingsModal::LogoutConfirm {
        ui.modal(ModalOptions {
            title: Some("Confirmar Logout".to_string()),
            message: Some(
                "Tem certeza que deseja sair? Isso removerá as credenciais armazenadas."
                    .to_string(),
            ),
            buttons: vec![
                ModalButton::new("Cancelar"),
                ModalButton::new("Sair"),
            ],
            ..Default::default()
        });
    }
}