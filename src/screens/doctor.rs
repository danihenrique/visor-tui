use hqtui::prelude::*;
use hqtui::color::Color;
use hqtui::widgets::{ButtonOptions, ButtonVariant, DividerOptions, TextStyle};

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

fn check_version() -> CheckResult {
    let version = env!("CARGO_PKG_VERSION");
    CheckResult {
        name: "Version".to_string(),
        outcome: Outcome::Ok,
        message: format!("{version}"),
    }
}

fn check_login() -> CheckResult {
    match auth::load_credential() {
        Some(cred) => {
            let kind = cred.kind_label();
            let redacted = cred.redacted();
            CheckResult {
                name: "Login".to_string(),
                outcome: Outcome::Ok,
                message: format!("{kind} ({redacted})"),
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
            message: format!("{} ({} plan)", me.email, me.plan.as_deref().unwrap_or("unknown")),
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

/// Pre-computed display entry for a single check, owned so it can be moved into closures.
#[derive(Debug, Clone)]
struct CheckDisplay {
    line: String,
    color: Color,
}

pub fn render(ui: &mut Container<'_>, state: &DoctorState) {
    // Pre-compute all display data before entering closures.
    let displays: Vec<CheckDisplay> = state
        .checks
        .iter()
        .map(|check| CheckDisplay {
            line: format!(
                "  {}  {:<13} {}",
                check.outcome.indicator(),
                check.name,
                check.message
            ),
            color: check.outcome.color(),
        })
        .collect();
    let is_empty = state.checks.is_empty();

    ui.column(Column::new().gap(1), move |col| {
        col.panel(Panel::new().title("Diagnostics"), move |p| {
            p.text("Health checks for your Visor TUI environment.");
            p.divider(DividerOptions::default());

            for d in &displays {
                p.styled_text(&d.line, TextStyle::new().fg(d.color));
            }

            if is_empty {
                p.text("  No checks have been run yet.");
            }

            p.divider(DividerOptions::default());
            p.text("Press 'r' to re-run checks.");
        });

        col.row(Row::new(), move |r| {
            r.button(
                ButtonOptions::new("Re-run checks").variant(ButtonVariant::Primary),
                "doctor_rerun",
            );
        });
    });
}