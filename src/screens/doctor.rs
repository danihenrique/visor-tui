use hqtui::color::Color;
use hqtui::prelude::*;
use hqtui::widgets::{ButtonOptions, ButtonVariant, TextStyle};

use crate::api::ApiClient;
use crate::auth;
use crate::config;

const COLOR_OK: Color = Color::rgb(34, 197, 94);
const COLOR_WARN: Color = Color::rgb(234, 179, 8);
const COLOR_FAIL: Color = Color::rgb(239, 68, 68);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Ok,
    Warn,
    Fail,
}

impl Outcome {
    fn indicator(self) -> &'static str {
        match self {
            Self::Ok => "\u{2713}",
            Self::Warn => "\u{26a0}",
            Self::Fail => "\u{2717}",
        }
    }

    fn color(self) -> Color {
        match self {
            Self::Ok => COLOR_OK,
            Self::Warn => COLOR_WARN,
            Self::Fail => COLOR_FAIL,
        }
    }
}

#[derive(Debug, Clone)]
struct CheckResult {
    name: String,
    outcome: Outcome,
    message: String,
}

/// Cached state for the doctor screen.
#[derive(Debug, Clone)]
pub struct DoctorState {
    checks: Vec<CheckResult>,
}

impl DoctorState {
    #[must_use]
    pub fn new() -> Self {
        Self { checks: Vec::new() }
    }

    /// Run all four health checks and cache the results.
    pub fn run_checks(&mut self, api: Option<&ApiClient>) {
        self.checks.clear();
        self.checks.push(check_version());
        let login_result = check_login();
        self.checks.push(login_result.clone());

        // API and space checks depend on having a valid credential
        if login_result.outcome == Outcome::Ok {
            if let Some(client) = api {
                let api_result = check_api(client);
                self.checks.push(api_result.clone());
                if api_result.outcome == Outcome::Ok {
                    self.checks.push(check_space(client));
                }
            } else {
                self.checks.push(CheckResult {
                    name: "Visor API".to_string(),
                    outcome: Outcome::Fail,
                    message: "API client not available".to_string(),
                });
            }
        }
    }
}

impl Default for DoctorState {
    fn default() -> Self {
        Self::new()
    }
}

fn check_version() -> CheckResult {
    let version = env!("CARGO_PKG_VERSION");
    CheckResult {
        name: "Version".to_string(),
        outcome: Outcome::Ok,
        message: version.to_string(),
    }
}

fn check_login() -> CheckResult {
    match auth::load_credential() {
        Some(cred) => {
            let kind = cred.kind_label();
            CheckResult {
                name: "Login".to_string(),
                outcome: Outcome::Ok,
                message: format!("Sessão autenticada · {kind}"),
            }
        }
        None => CheckResult {
            name: "Login".to_string(),
            outcome: Outcome::Fail,
            message: "Not logged in. Use the login screen or set VISOR_TOKEN.".to_string(),
        },
    }
}

fn check_api(api: &ApiClient) -> CheckResult {
    match api.me() {
        Ok(me) => CheckResult {
            name: "Visor API".to_string(),
            outcome: Outcome::Ok,
            message: format!(
                "{} ({} plan)",
                me.email,
                me.plan.as_deref().unwrap_or("unknown")
            ),
        },
        Err(e) => {
            let first_line = e.to_string().lines().next().unwrap_or_default().to_string();
            CheckResult {
                name: "Visor API".to_string(),
                outcome: Outcome::Fail,
                message: first_line,
            }
        }
    }
}

fn check_space(api: &ApiClient) -> CheckResult {
    match config::active_space_id() {
        Some(id) => {
            // Verify the space still exists by listing spaces
            match api.spaces() {
                Ok(spaces) => match spaces.iter().find(|s| s.id == id) {
                    Some(space) => CheckResult {
                        name: "Space".to_string(),
                        outcome: Outcome::Ok,
                        message: format!("{} ({})", space.name, id),
                    },
                    None => CheckResult {
                        name: "Space".to_string(),
                        outcome: Outcome::Warn,
                        message: format!(
                            "Configured space {id} no longer exists. Choose a new one."
                        ),
                    },
                },
                Err(_) => CheckResult {
                    name: "Space".to_string(),
                    outcome: Outcome::Warn,
                    message: "Could not list spaces to verify active space.".to_string(),
                },
            }
        }
        None => CheckResult {
            name: "Space".to_string(),
            outcome: Outcome::Warn,
            message: "No active space configured. Select one from Settings.".to_string(),
        },
    }
}

pub fn render(ui: &mut Container<'_>, state: &DoctorState) {
    use crate::presentation::{Metric, Tone, metric_strip};
    let ok = state
        .checks
        .iter()
        .filter(|c| c.outcome == Outcome::Ok)
        .count();
    let warnings = state
        .checks
        .iter()
        .filter(|c| c.outcome == Outcome::Warn)
        .count();
    let failures = state
        .checks
        .iter()
        .filter(|c| c.outcome == Outcome::Fail)
        .count();
    let checks = state.checks.clone();
    let show_metrics = ui.height() >= 15;
    let muted = ui.theme().muted;
    ui.column(Column::new().gap(1), move |col| {
        if show_metrics {
            metric_strip(
                col,
                vec![
                    Metric::new("Verificações OK", ok.to_string(), Tone::Positive),
                    Metric::new("Avisos", warnings.to_string(), Tone::Warning),
                    Metric::new("Falhas", failures.to_string(), Tone::Negative),
                ],
            );
        }
        col.panel(Panel::new().title("Saúde do aplicativo"), move |panel| {
            if checks.is_empty() {
                panel.text("As verificações ainda não foram executadas.");
            }
            for check in checks {
                let name = match check.name.as_str() {
                    "Version" => "Versão instalada",
                    "Login" => "Autenticação",
                    "Space" => "Espaço ativo",
                    other => other,
                };
                panel.styled_text(
                    &format!("{}  {name}", check.outcome.indicator()),
                    TextStyle::new().fg(check.outcome.color()).bold(),
                );
                for line in hqtui::unicode::wrap(&check.message, panel.width())
                    .into_iter()
                    .take(2)
                {
                    panel.styled_text(&line, TextStyle::new().fg(muted));
                }
                if show_metrics {
                    panel.spacer(Size::Cells(1));
                }
            }
        });
        col.row(Row::new().size(Size::Cells(1)), move |row| {
            row.button(
                ButtonOptions::new("r Executar verificações").variant(ButtonVariant::Ghost),
                "doctor_rerun",
            );
        });
    });
}
