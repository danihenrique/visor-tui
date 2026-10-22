use hqtui::prelude::*;
use hqtui::color::Color;
use hqtui::widgets::{KeyValueOptions, KeyValueRow, ButtonOptions, ButtonVariant};

use crate::api::{ApiClient, ApiError};

const COLOR_NEGATIVE: Color = Color::rgb(239, 68, 68);
const COLOR_ACCENT: Color = Color::rgb(34, 197, 94);
const COLOR_NEUTRAL: Color = Color::rgb(148, 163, 184);

fn draw_error(ui: &mut Container<'_>, err: &ApiError) {
    let err_msg = format!("Falha ao carregar configurações: {err}");
    ui.panel(Panel::new().title("Erro"), move |p| {
        p.styled_text(&err_msg, TextStyle::new().fg(COLOR_NEGATIVE));
        p.label("Verifique sua conexão e credenciais.");
    });
}

pub fn render(ui: &mut Container<'_>, api: &ApiClient) {
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
        .and_then(|sid| spaces.iter().find(|s| s.id == sid))
        .map(|s| format!("* {}", s.name))
        .unwrap_or_else(|| "Nenhum espaço ativo".to_string());

    let has_active_space = active_space_id.is_some();

    // Build key-value rows for profile panel
    let profile_rows = vec![
        KeyValueRow::new("Nome", user_name),
        KeyValueRow::new("Email", user_email),
        KeyValueRow::new("Locale", user_locale),
        KeyValueRow::new("Moeda", user_currency),
        KeyValueRow::new("Plano", user_plan),
    ];

    let auth_status = "Token ativo".to_string();

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
                rows: vec![KeyValueRow::new("Status", auth_status.clone())],
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
}