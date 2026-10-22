use std::cell::RefCell;
use std::rc::Rc;

use hqtui::prelude::*;
use hqtui::input::InputEvent;

use crate::api::ApiClient;
use crate::auth;
use crate::config;
use crate::screens;

pub struct VisorApp {
    app: App,
    screen: Screen,
    api: Option<Rc<ApiClient>>,
    show_help: bool,
    login_token_input: String,
    login_error: Option<String>,
    space_selector_visible: bool,
    spaces: Vec<crate::models::Space>,
    transactions_screen: Rc<RefCell<screens::transactions::TransactionsScreen>>,
    settings_modal: screens::settings::SettingsModal,
    doctor_state: screens::doctor::DoctorState,
    doctor_needs_run: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Overview,
    Transactions,
    Recurring,
    CashFlow,
    AccountsCards,
    Categories,
    Goals,
    Projection,
    NetWorth,
    Bills,
    Payables,
    Settings,
    Doctor,
}

impl Screen {
    fn label(self) -> &'static str {
        match self {
            Self::Overview => "Vis\u{e3}o Geral",
            Self::Transactions => "Transa\u{e7}\u{f5}es",
            Self::Recurring => "Recorrentes",
            Self::CashFlow => "Fluxo de Caixa",
            Self::AccountsCards => "Contas e Cart\u{f5}es",
            Self::Categories => "Categorias",
            Self::Goals => "Metas",
            Self::Projection => "Proje\u{e7}\u{e3}o",
            Self::NetWorth => "Patrim\u{f4}nio",
            Self::Bills => "Faturas",
            Self::Payables => "Contas a pagar",
            Self::Settings => "Configura\u{e7}\u{f5}es",
            Self::Doctor => "Diagn\u{f3}stico",
        }
    }

    fn shortcut_key(self) -> &'static str {
        match self {
            Self::Overview => "1",
            Self::Transactions => "2",
            Self::Recurring => "3",
            Self::CashFlow => "4",
            Self::AccountsCards => "5",
            Self::Categories => "6",
            Self::Goals => "7",
            Self::Projection => "8",
            Self::NetWorth => "9",
            Self::Bills => "0",
            Self::Payables => "-",
            Self::Settings => "=",
            Self::Doctor => "d",
        }
    }

    fn all() -> [Screen; 13] {
        [
            Self::Overview,
            Self::Transactions,
            Self::Recurring,
            Self::CashFlow,
            Self::AccountsCards,
            Self::Categories,
            Self::Goals,
            Self::Projection,
            Self::NetWorth,
            Self::Bills,
            Self::Payables,
            Self::Settings,
            Self::Doctor,
        ]
    }

    fn from_index(i: usize) -> Option<Screen> {
        Screen::all().get(i).copied()
    }

    fn index(self) -> usize {
        Screen::all()
            .iter()
            .position(|s| *s == self)
            .unwrap_or(0)
    }
}

/// Load token from `visor.env` and save as credential if no credential exists yet.
/// This avoids runtime env var mutation (forbidden by `unsafe_code = "forbid"`).
fn load_env_token() -> Option<auth::Credential> {
    let path = config::config_dir().join("visor.env");
    let contents = std::fs::read_to_string(&path).ok()?;
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim().trim_matches('"').trim_matches('\'');
            if key == "VISOR_TOKEN" && !value.is_empty() {
                return Some(auth::Credential::PersonalToken {
                    token: value.to_string(),
                });
            }
        }
    }
    None
}

impl VisorApp {
    pub fn new() -> std::io::Result<Self> {
        let app = App::with_options(AppOptions {
            quit_keys: vec!["ctrl+c".into()],
            ..AppOptions::default()
        })?;

        // Try env-file token first, then stored credential
        let api = if let Some(cred) = load_env_token() {
            ApiClient::with_credential(cred).ok().map(Rc::new)
        } else {
            ApiClient::new().ok().map(Rc::new)
        };

        let (space_selector_visible, spaces) = if let Some(ref client) = api {
            match client.spaces() {
                Ok(list) => {
                    if config::active_space_id().is_none() {
                        if list.len() == 1 {
                            let _ = config::set_default_space_id(&list[0].id);
                            (false, list)
                        } else if !list.is_empty() {
                            (true, list)
                        } else {
                            (false, vec![])
                        }
                    } else {
                        (false, list)
                    }
                }
                Err(_) => (false, vec![]),
            }
        } else {
            (false, vec![])
        };

        Ok(Self {
            app,
            screen: Screen::Overview,
            api,
            show_help: false,
            login_token_input: String::new(),
            login_error: None,
            space_selector_visible,
            spaces,
            transactions_screen: Rc::new(RefCell::new(screens::transactions::TransactionsScreen::new())),
            settings_modal: screens::settings::SettingsModal::None,
            doctor_state: screens::doctor::DoctorState::new(),
            doctor_needs_run: true,
        })
    }

    pub fn run(&mut self) -> std::io::Result<()> {
        while self.app.running() {
            let events = self.app.poll()?;
            self.handle_input(&events);
            self.draw()?;
        }
        Ok(())
    }

    fn handle_input(&mut self, events: &[InputEvent]) {
        if self.show_help {
            for event in events {
                if let InputEvent::Key(k) = event {
                    if k.key == "escape" || k.name == "q" || k.name == "?" {
                        self.show_help = false;
                        return;
                    }
                }
            }
            return;
        }

        if self.space_selector_visible {
            for event in events {
                if let InputEvent::Key(k) = event {
                    if k.key == "escape" {
                        self.app.quit();
                        return;
                    }
                    if let Some(ch) = &k.char {
                        if let Ok(idx) = ch.parse::<usize>() {
                            if idx >= 1 && idx <= self.spaces.len() {
                                let space_id = &self.spaces[idx - 1].id;
                                let _ = config::set_default_space_id(space_id);
                                self.space_selector_visible = false;
                                self.api = ApiClient::new().ok().map(Rc::new);
                                return;
                            }
                        }
                    }
                }
            }
            for i in 0..self.spaces.len() {
                let id = format!("space:{i}");
                if self.app.pressed(&id) {
                    let space_id = &self.spaces[i].id;
                    let _ = config::set_default_space_id(space_id);
                    self.space_selector_visible = false;
                    self.api = ApiClient::new().ok().map(Rc::new);
                    return;
                }
            }
            return;
        }

        if self.api.is_none() {
            for event in events {
                if let InputEvent::Key(k) = event {
                    if k.key == "ctrl+c" {
                        self.app.quit();
                        return;
                    }
                    if k.name == "enter" {
                        if !self.login_token_input.is_empty() {
                            let token = self.login_token_input.trim().to_string();
                            let cred = auth::Credential::PersonalToken {
                                token: token.clone(),
                            };
                            match ApiClient::with_credential(cred.clone()) {
                                Ok(client) => {
                                    let _ = auth::save_credential(&cred);
                                    self.api = Some(Rc::new(client));
                                    self.login_error = None;
                                    self.login_token_input.clear();
                                    self.check_space_after_login();
                                }
                                Err(e) => {
                                    self.login_error =
                                        Some(format!("Token inv\u{e1}lido: {e}"));
                                }
                            }
                        }
                        return;
                    }
                    if k.key == "backspace" {
                        self.login_token_input.pop();
                        return;
                    }
                    if k.key == "ctrl+u" {
                        self.login_token_input.clear();
                        return;
                    }
                    if let Some(ch) = &k.char {
                        if !k.ctrl && !k.alt {
                            self.login_token_input.push_str(ch);
                        }
                    }
                }
            }
            return;
        }

        for event in events {
            if let InputEvent::Key(k) = event {
                if k.name == "?" {
                    self.show_help = true;
                    return;
                }
                if k.name == "q" {
                    self.app.quit();
                    return;
                }
                if let Some(ch) = &k.char {
                    match ch.as_str() {
                        "1" => {
                            self.screen = Screen::Overview;
                            return;
                        }
                        "2" => {
                            self.screen = Screen::Transactions;
                            return;
                        }
                        "3" => {
                            self.screen = Screen::Recurring;
                            return;
                        }
                        "4" => {
                            self.screen = Screen::CashFlow;
                            return;
                        }
                        "5" => {
                            self.screen = Screen::AccountsCards;
                            return;
                        }
                        "6" => {
                            self.screen = Screen::Categories;
                            return;
                        }
                        "7" => {
                            self.screen = Screen::Goals;
                            return;
                        }
                        "8" => {
                            self.screen = Screen::Projection;
                            return;
                        }
                        "9" => {
                            self.screen = Screen::NetWorth;
                            return;
                        }
                        "0" => {
                            self.screen = Screen::Bills;
                            return;
                        }
                        "-" => {
                            self.screen = Screen::Payables;
                            return;
                        }
                        "=" => {
                            self.screen = Screen::Settings;
                            return;
                        }
                        "d" => {
                            self.screen = Screen::Doctor;
                            self.doctor_needs_run = true;
                            return;
                        }
                        _ => {}
                    }
                }
                if k.name == "down" || k.name == "right" || (k.name == "tab" && !k.shift) {
                    let next = (self.screen.index() + 1) % Screen::all().len();
                    self.screen = Screen::from_index(next).unwrap_or(Screen::Overview);
                    return;
                }
                if k.name == "up" || k.name == "left" || (k.name == "tab" && k.shift) {
                    let len = Screen::all().len();
                    let prev = (self.screen.index() + len - 1) % len;
                    self.screen = Screen::from_index(prev).unwrap_or(Screen::Overview);
                    return;
                }
            }
        }

        for (i, scr) in Screen::all().iter().enumerate() {
            let id = format!("nav:{i}");
            if self.app.pressed(&id) {
                self.screen = *scr;
            }
        }

        // Dispatch input to the transactions screen when it is active.
        if self.screen == Screen::Transactions
            && let Some(ref api) = self.api
        {
            self.transactions_screen.borrow_mut().handle_input(&self.app, api, events);
        }

        // Handle doctor screen re-run.
        if self.screen == Screen::Doctor {
            let mut rerun = false;
            for event in events {
                if let InputEvent::Key(k) = event {
                    if k.name == "r" {
                        rerun = true;
                    }
                }
            }
            if self.app.pressed("doctor_rerun") {
                rerun = true;
            }
            if rerun {
                self.doctor_needs_run = true;
            }
        }

        // Handle settings screen logout flow.
        if self.screen == Screen::Settings {
            // Check for key events in the modal state.
            let mut escape_pressed = false;
            let mut enter_pressed = false;
            for event in events {
                if let InputEvent::Key(k) = event {
                    if k.name == "escape" {
                        escape_pressed = true;
                    }
                    if k.name == "enter" {
                        enter_pressed = true;
                    }
                }
            }

            match self.settings_modal {
                screens::settings::SettingsModal::None => {
                    if self.app.pressed("logout") {
                        self.settings_modal = screens::settings::SettingsModal::LogoutConfirm;
                    }
                }
                screens::settings::SettingsModal::LogoutConfirm => {
                    if self.app.pressed("Cancelar") || escape_pressed {
                        self.settings_modal = screens::settings::SettingsModal::None;
                    }
                    if self.app.pressed("Sair") || enter_pressed {
                        // Perform logout
                        let _ = crate::auth::clear_credential();
                        self.api = None;
                        self.login_token_input.clear();
                        self.settings_modal = screens::settings::SettingsModal::None;
                    }
                }
            }
        }
    }

    fn check_space_after_login(&mut self) {
        if let Some(ref client) = self.api
            && config::active_space_id().is_none()
        {
            if let Ok(list) = client.spaces() {
                if list.len() == 1 {
                    let _ = config::set_default_space_id(&list[0].id);
                    self.api = ApiClient::new().ok().map(Rc::new);
                } else if !list.is_empty() {
                    self.spaces = list;
                    self.space_selector_visible = true;
                }
            }
        }
    }

    fn draw(&mut self) -> std::io::Result<()> {
        if self.api.is_none() {
            return self.draw_login_screen();
        }
        if self.space_selector_visible {
            return self.draw_space_selector();
        }

        let current = self.screen;
        let screens_arr = Screen::all();
        let show_help = self.show_help;
        let nav_labels: Vec<String> = screens_arr
            .iter()
            .map(|scr| format!(" {} {}", scr.shortcut_key(), scr.label()))
            .collect();

        // Run doctor checks if needed before drawing
        if self.screen == Screen::Doctor && self.doctor_needs_run {
            self.doctor_state.run_checks(self.api.as_deref());
            self.doctor_needs_run = false;
        }

        let api = Rc::clone(self.api.as_ref().expect("api is Some at this point"));
        let tx_screen = self.transactions_screen.clone();
        let settings_modal = self.settings_modal;
        let doctor_state = self.doctor_state.clone();

        self.app.draw(move |f| {
            f.ui.row(Row::new().gap(0), move |row| {
                row.column(Column::new().size(Size::Cells(24)).gap(0), move |col| {
                    col.panel(Panel::new().title("Visor"), move |p| {
                        p.heading("Visor Finance");
                        p.divider(DividerOptions::default());
                        for (i, label) in nav_labels.iter().enumerate() {
                            let id = format!("nav:{i}");
                            let is_active = screens_arr[i] == current;
                            if is_active {
                                p.styled_text(label, TextStyle::default().bold());
                            } else {
                                p.text(label);
                            }
                            p.button(
                                ButtonOptions::new("").variant(ButtonVariant::Ghost),
                                &id,
                            );
                        }
                        p.divider(DividerOptions::default());
                        p.text(" ? Ajuda  q Sair");
                    });
                });

                row.column(Column::new().size(Size::Fill).gap(0), move |col| {
                    col.panel(Panel::new().title(current.label()), move |p| {
                        match current {
                            Screen::Overview => screens::overview::draw_overview(p, &api),
                            Screen::Transactions => {
                                screens::transactions::render_with_state(p, &mut tx_screen.borrow_mut(), &api);
                            }
                            Screen::Recurring => screens::recurring::render(p, &api),
                            Screen::CashFlow => screens::cash_flow::render(p, &api),
                            Screen::AccountsCards => {
                                screens::accounts_cards::render(p, &api, false);
                            }
                            Screen::Categories => screens::categories::render(p, &api),
                            Screen::Goals => screens::goals::render(p, &api),
                            Screen::Projection => screens::projection::render(p, &api),
                            Screen::NetWorth => screens::net_worth::render(p, &api),
                            Screen::Bills => screens::bills::render(p, &api),
                            Screen::Payables => screens::payables::render(p, &api),
                            Screen::Settings => screens::settings::render(p, &api, settings_modal),
                            Screen::Doctor => {
                                screens::doctor::render(p, &doctor_state);
                            }
                        }
                    });
                });
            });

            if show_help {
                f.ui.modal(ModalOptions {
                    title: Some("Ajuda".to_string()),
                    message: Some(
                        "Navega\u{e7}\u{e3}o:\n\
                         1-9, 0, -, = : Ir para tela\n\
                         \u{2191}/\u{2193} ou Tab/Shift+Tab : Ciclar telas\n\
                         Clique no menu lateral\n\
                         ? : Esta ajuda\n\
                         q : Sair\n\
                         Ctrl+C : Sair"
                            .to_string(),
                    ),
                    buttons: vec![ModalButton::new("Fechar")],
                    ..Default::default()
                });
            }
        })?;

        Ok(())
    }

    fn draw_login_screen(&mut self) -> std::io::Result<()> {
        let token_display = "*".repeat(self.login_token_input.len());
        let error_msg = self.login_error.clone();

        self.app.draw(move |f| {
            f.ui.column(Column::new().gap(1), move |col| {
                col.panel(Panel::new().title("Login - Visor Finance"), move |p| {
                    p.text("Digite seu token pessoal (PAT):");
                    p.text(&format!("Token: {token_display}_"));
                    if let Some(ref err) = error_msg {
                        p.styled_text(err, TextStyle::new().fg(Color::rgb(239, 68, 68)));
                    }
                    p.divider(DividerOptions::default());
                    p.text("Enter para validar | Ctrl+C para sair");
                    p.text("O token ser\u{e1} salvo em ~/.config/visor/credentials.json");
                });
            });
        })?;
        Ok(())
    }

    fn draw_space_selector(&mut self) -> std::io::Result<()> {
        let spaces = self.spaces.clone();
        let items: Vec<String> = spaces
            .iter()
            .enumerate()
            .map(|(i, s)| format!(" {}. {} ({})", i + 1, s.name, s.id))
            .collect();

        self.app.draw(move |f| {
            f.ui.column(Column::new().gap(1), move |col| {
                col.panel(
                    Panel::new().title("Selecionar Espa\u{e7}o"),
                    move |p| {
                        p.text("Selecione um espa\u{e7}o para continuar:");
                        p.divider(DividerOptions::default());
                        for (i, item) in items.iter().enumerate() {
                            let id = format!("space:{i}");
                            p.text(item);
                            p.button(
                                ButtonOptions::new("").variant(ButtonVariant::Ghost),
                                &id,
                            );
                        }
                        p.divider(DividerOptions::default());
                        p.text("Pressione o n\u{fa}mero ou clique | Esc para sair");
                    },
                );
            });
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screen_shortcut_keys_are_unique() {
        let screens = Screen::all();
        let mut keys = std::collections::HashSet::new();
        for scr in &screens {
            assert!(
                keys.insert(scr.shortcut_key()),
                "Duplicate shortcut key for {:?}",
                scr
            );
        }
    }

    #[test]
    fn test_screen_from_index_roundtrip() {
        for (i, scr) in Screen::all().iter().enumerate() {
            assert_eq!(Screen::from_index(i), Some(*scr));
            assert_eq!(scr.index(), i);
        }
    }

    #[test]
    fn test_screen_count() {
        assert_eq!(Screen::all().len(), 13);
    }

    #[test]
    fn test_payables_has_distinct_key() {
        let payables = Screen::Payables;
        assert_ne!(payables.shortcut_key(), "1");
        assert_eq!(payables.shortcut_key(), "-");
    }

    #[test]
    fn test_load_env_token_missing_is_ok() {
        // Should return None when file doesn't exist
        assert!(load_env_token().is_none());
    }
}