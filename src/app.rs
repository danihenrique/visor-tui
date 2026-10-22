use std::cell::RefCell;
use std::rc::Rc;

use hqtui::input::InputEvent;
use hqtui::prelude::*;

use crate::api::ApiClient;
use crate::auth;
use crate::config;
use crate::keymap::{GlobalAction, Screen, global_action};
use crate::presentation;
use crate::screens;

#[allow(clippy::struct_excessive_bools)]
pub struct VisorApp {
    app: App,
    screen: Screen,
    api: Option<Rc<ApiClient>>,
    show_help: bool,
    login_token_input: String,
    login_error: Option<String>,
    space_selector_visible: bool,
    selected_space: usize,
    enter_pending: bool,
    show_hidden_accounts: bool,
    account_offset: usize,
    bills_screen: screens::bills::BillsScreen,
    payables_screen: screens::payables::PayablesScreen,
    spaces: Vec<crate::models::Space>,
    transactions_screen: Rc<RefCell<screens::transactions::TransactionsScreen>>,
    tags_screen: Rc<RefCell<screens::tags::TagsScreen>>,
    goals_screen: Rc<RefCell<screens::goals::GoalsScreen>>,
    categories_screen: Rc<RefCell<screens::categories::CategoriesScreen>>,
    recurring_screen: Rc<RefCell<screens::recurring::RecurringScreen>>,
    notifications_screen: Rc<RefCell<screens::notifications::NotificationsScreen>>,
    connections_screen: Rc<RefCell<screens::connections::ConnectionsScreen>>,
    settings_modal: screens::settings::SettingsModal,
    doctor_state: screens::doctor::DoctorState,
}

/// Load token from `visor.env` and save as credential if no credential exists yet.
/// This avoids runtime env var mutation (forbidden by `unsafe_code = "forbid"`).
fn load_env_token() -> Option<auth::Credential> {
    let path = config::config_dir().join("visor.env");
    let contents = std::fs::read_to_string(&path).ok()?;
    parse_env_token(&contents)
}

fn parse_env_token(contents: &str) -> Option<auth::Credential> {
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
        let mut app = App::with_options(AppOptions {
            quit_keys: vec!["ctrl+c".into()],
            focus_navigation: false,
            ..AppOptions::default()
        })?;

        app.theme = Rc::new(presentation::theme());

        // Try env-file token first, then stored credential
        let mut api = if let Some(cred) = load_env_token() {
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
                            api = ApiClient::with_credential(client.credential().clone())
                                .ok()
                                .map(Rc::new);
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
            selected_space: 0,
            enter_pending: true,
            show_hidden_accounts: false,
            account_offset: 0,
            bills_screen: screens::bills::BillsScreen::default(),
            payables_screen: screens::payables::PayablesScreen::default(),
            spaces,
            transactions_screen: Rc::new(RefCell::new(
                screens::transactions::TransactionsScreen::new(),
            )),
            tags_screen: Rc::new(RefCell::new(screens::tags::TagsScreen::new())),
            goals_screen: Rc::new(RefCell::new(screens::goals::GoalsScreen::new())),
            categories_screen: Rc::new(RefCell::new(screens::categories::CategoriesScreen::new())),
            recurring_screen: Rc::new(RefCell::new(screens::recurring::RecurringScreen::new())),
            notifications_screen: Rc::new(RefCell::new(
                screens::notifications::NotificationsScreen::new(),
            )),
            connections_screen: Rc::new(RefCell::new(
                screens::connections::ConnectionsScreen::new(),
            )),
            settings_modal: screens::settings::SettingsModal::None,
            doctor_state: screens::doctor::DoctorState::new(),
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
        for event in events {
            if let InputEvent::Paste(paste) = event {
                if self.api.is_none() {
                    self.login_token_input.push_str(paste.text.trim());
                } else if self.modal_open() && !self.show_help && !self.space_selector_visible {
                    if self.screen == Screen::Transactions {
                        self.dispatch_screen(std::slice::from_ref(event));
                    } else {
                        for character in paste
                            .text
                            .chars()
                            .filter(|character| !character.is_control())
                        {
                            let character = character.to_string();
                            self.dispatch_screen(&[InputEvent::Key(hqtui::input::KeyEvent {
                                name: character.clone(),
                                key: character.clone(),
                                raw: character.clone(),
                                char: Some(character),
                                ctrl: false,
                                alt: false,
                                shift: false,
                            })]);
                        }
                    }
                }
                continue;
            }
            if let InputEvent::Key(key) = event {
                if self.show_help {
                    if matches!(key.name.as_str(), "escape" | "enter" | "q" | "?") {
                        self.show_help = false;
                    }
                    continue;
                }
                if self.space_selector_visible {
                    match key.name.as_str() {
                        "escape" => self.space_selector_visible = false,
                        "down" | "j" => {
                            self.selected_space =
                                (self.selected_space + 1).min(self.spaces.len().saturating_sub(1));
                        }
                        "up" | "k" => self.selected_space = self.selected_space.saturating_sub(1),
                        "enter" => self.select_space(self.selected_space),
                        _ => {
                            if let Some(index) = key
                                .char
                                .as_deref()
                                .and_then(|value| value.parse::<usize>().ok())
                                .filter(|index| *index > 0)
                            {
                                self.select_space(index - 1);
                            }
                        }
                    }
                    continue;
                }
                if self.api.is_none() {
                    self.handle_login_key(key);
                    continue;
                }
                if self.modal_open() {
                    self.dispatch_screen(std::slice::from_ref(event));
                    continue;
                }
                match global_action(self.screen, key, false) {
                    Some(GlobalAction::Help) => self.show_help = true,
                    Some(GlobalAction::Quit) => self.app.quit(),
                    Some(GlobalAction::Screen(screen)) => self.enter_screen(screen),
                    Some(GlobalAction::Refresh) => {
                        if let Some(api) = &self.api {
                            api.invalidate_cache();
                        }
                        self.enter_pending = true;
                    }
                    None => self.dispatch_screen(std::slice::from_ref(event)),
                }
            }
        }
        let mouse_events: Vec<_> = events
            .iter()
            .filter(|event| matches!(event, InputEvent::Mouse(_)))
            .cloned()
            .collect();
        if mouse_events.is_empty() {
            return;
        }
        if self.show_help {
            if mouse_events.iter().any(|event| matches!(event, InputEvent::Mouse(mouse) if mouse.action == hqtui::input::MouseAction::Press)) { self.show_help = false; }
        } else if self.space_selector_visible {
            for index in 0..self.spaces.len() {
                if self.app.pressed(&format!("space:{index}")) {
                    self.select_space(index);
                    break;
                }
            }
        } else if self.modal_open() {
            self.dispatch_screen(&mouse_events);
        } else if self.api.is_some() {
            for screen in Screen::all() {
                if self.app.pressed(&format!("nav:{}", screen.index())) {
                    self.enter_screen(screen);
                    return;
                }
            }
            self.dispatch_screen(&mouse_events);
        }
    }

    fn enter_screen(&mut self, screen: Screen) {
        if self.screen != screen {
            self.screen = screen;
            self.enter_pending = true;
        }
    }

    fn modal_open(&self) -> bool {
        match self.screen {
            Screen::Transactions => self.transactions_screen.borrow().is_modal_open(),
            Screen::Tags => {
                self.tags_screen.borrow().modal_state != screens::tags::TagModalState::None
            }
            Screen::Goals => {
                self.goals_screen.borrow().modal_state != screens::goals::GoalModalState::None
            }
            Screen::Categories => {
                self.categories_screen.borrow().modal_state
                    != screens::categories::CategoryModalState::None
            }
            Screen::Recurring => {
                self.recurring_screen.borrow().modal_state
                    != screens::recurring::RecurringModalState::None
            }
            Screen::Settings => self.settings_modal != screens::settings::SettingsModal::None,
            _ => false,
        }
    }

    fn dispatch_screen(&mut self, events: &[InputEvent]) {
        let Some(api) = self.api.as_ref().map(Rc::clone) else {
            return;
        };
        api.set_fetch_allowed(true);
        match self.screen {
            Screen::Transactions => self
                .transactions_screen
                .borrow_mut()
                .handle_input(&self.app, &api, events),
            Screen::Tags => self
                .tags_screen
                .borrow_mut()
                .handle_input(&self.app, &api, events),
            Screen::Goals => self
                .goals_screen
                .borrow_mut()
                .handle_input(&self.app, &api, events),
            Screen::Categories => self
                .categories_screen
                .borrow_mut()
                .handle_input(&self.app, &api, events),
            Screen::Recurring => self
                .recurring_screen
                .borrow_mut()
                .handle_input(&self.app, &api, events),
            Screen::Notifications => self
                .notifications_screen
                .borrow_mut()
                .handle_input(&self.app, &api, events),
            Screen::Connections => self
                .connections_screen
                .borrow_mut()
                .handle_input(&self.app, &api, events),
            Screen::Bills => self.bills_screen.handle_input(&self.app, &api, events),
            Screen::Payables => self.payables_screen.handle_input(&self.app, events),
            Screen::AccountsCards => {
                if pressed_key(events, "h") || self.app.pressed("toggle_hidden") {
                    self.show_hidden_accounts = !self.show_hidden_accounts;
                    self.enter_pending = true;
                }
                if pressed_key(events, "j") || pressed_key(events, "down") {
                    self.account_offset = self.account_offset.saturating_add(1);
                }
                if pressed_key(events, "k") || pressed_key(events, "up") {
                    self.account_offset = self.account_offset.saturating_sub(1);
                }
            }
            Screen::Doctor => {
                if self.app.pressed("doctor_rerun") {
                    self.enter_pending = true;
                }
            }
            Screen::Settings => self.handle_settings(events),
            _ => {}
        }
        api.set_fetch_allowed(false);
    }

    fn handle_settings(&mut self, events: &[InputEvent]) {
        let modal_events = presentation::modal_events(
            events,
            self.app.terminal.size(),
            Some(&screens::settings::logout_dialog()),
        );
        let events = modal_events.as_slice();
        match self.settings_modal {
            screens::settings::SettingsModal::None => {
                if pressed_key(events, "l") || self.app.pressed("logout") {
                    self.settings_modal = screens::settings::SettingsModal::LogoutConfirm;
                }
                if pressed_key(events, "s") || self.app.pressed("change_space") {
                    self.space_selector_visible = true;
                }
            }
            screens::settings::SettingsModal::LogoutConfirm => {
                if pressed_key(events, "escape")
                    || pressed_key(events, "n")
                    || self.app.pressed("Cancelar")
                {
                    self.settings_modal = screens::settings::SettingsModal::None;
                } else if pressed_key(events, "enter")
                    || pressed_key(events, "y")
                    || self.app.pressed("Sair")
                {
                    let _ = auth::clear_credential();
                    self.api = None;
                    self.login_token_input.clear();
                    self.settings_modal = screens::settings::SettingsModal::None;
                }
            }
        }
    }

    fn handle_login_key(&mut self, key: &hqtui::input::KeyEvent) {
        match key.key.as_str() {
            "backspace" => {
                self.login_token_input.pop();
            }
            "ctrl+u" => self.login_token_input.clear(),
            "enter" if !self.login_token_input.trim().is_empty() => {
                let token = self.login_token_input.trim().to_string();
                if let Err(error) = auth::validate_pat(&token) {
                    self.login_error = Some(error);
                    return;
                }
                let credential = auth::Credential::PersonalToken { token };
                match ApiClient::with_credential(credential.clone()) {
                    Ok(client) => {
                        let _ = auth::save_credential(&credential);
                        self.api = Some(Rc::new(client));
                        self.login_error = None;
                        self.login_token_input.clear();
                        self.enter_pending = true;
                        self.check_space_after_login();
                    }
                    Err(error) => self.login_error = Some(format!("Token inválido: {error}")),
                }
            }
            _ if !key.ctrl && !key.alt => {
                if let Some(character) = &key.char {
                    self.login_token_input.push_str(character);
                }
            }
            _ => {}
        }
    }

    fn select_space(&mut self, index: usize) {
        let Some(space) = self.spaces.get(index) else {
            return;
        };
        if let Err(error) = config::set_default_space_id(&space.id) {
            self.login_error = Some(format!("Falha ao selecionar espaço: {error}"));
            return;
        }
        if let Some(api) = &self.api {
            self.api = ApiClient::with_credential(api.credential().clone())
                .ok()
                .map(Rc::new);
        }
        self.space_selector_visible = false;
        self.enter_pending = true;
    }

    fn load_screen(&mut self) {
        let Some(api) = self.api.as_ref() else { return };
        api.set_fetch_allowed(true);
        match self.screen {
            Screen::Transactions => self.transactions_screen.borrow_mut().on_enter(api),
            Screen::Tags => {
                let mut screen = self.tags_screen.borrow_mut();
                screen.loaded = true;
                screen.fetch_tags(api);
            }
            Screen::Goals => {
                let mut screen = self.goals_screen.borrow_mut();
                screen.loaded = true;
                screen.fetch_goals(api);
            }
            Screen::Categories => {
                let mut screen = self.categories_screen.borrow_mut();
                screen.loaded = true;
                screen.fetch_categories(api);
            }
            Screen::Recurring => {
                let mut screen = self.recurring_screen.borrow_mut();
                screen.loaded = true;
                screen.fetch_data(api);
            }
            Screen::Notifications => {
                let mut screen = self.notifications_screen.borrow_mut();
                screen.loaded = true;
                screen.fetch(api);
            }
            Screen::Connections => {
                let mut screen = self.connections_screen.borrow_mut();
                screen.loaded = true;
                screen.fetch(api);
            }
            Screen::Bills => self.bills_screen.load(api),
            Screen::Doctor => {
                self.doctor_state.run_checks(Some(api));
            }
            _ => {}
        }
    }

    fn check_space_after_login(&mut self) {
        let Some(client) = self.api.as_ref() else {
            return;
        };
        let Ok(spaces) = client.spaces() else { return };
        self.spaces = spaces;
        if config::active_space_id().is_none() {
            if self.spaces.len() == 1 {
                self.select_space(0);
            } else if !self.spaces.is_empty() {
                self.space_selector_visible = true;
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
        if self.enter_pending {
            let current = self.screen;
            self.app.draw(move |frame| {
                presentation::shell(
                    frame.ui,
                    current,
                    "Visor Finance".into(),
                    "Sincronizando dados…".into(),
                    false,
                    |panel| {
                        panel.text("Carregando…");
                    },
                );
            })?;
            self.load_screen();
        }

        let current = self.screen;
        let show_help = self.show_help;
        let modal_open = self.modal_open() || show_help;
        let api = Rc::clone(self.api.as_ref().expect("authenticated screen"));
        let api_after_draw = Rc::clone(&api);
        let active_space = config::active_space_id();
        let space = self
            .spaces
            .iter()
            .find(|space| Some(&space.id) == active_space.as_ref())
            .map_or_else(|| "Espaço pessoal".into(), |space| space.name.clone());
        let status = api.request_status();
        let tx_screen = Rc::clone(&self.transactions_screen);
        let tags_screen = Rc::clone(&self.tags_screen);
        let goals_screen = Rc::clone(&self.goals_screen);
        let categories_screen = Rc::clone(&self.categories_screen);
        let recurring_screen = Rc::clone(&self.recurring_screen);
        let notifications_screen = Rc::clone(&self.notifications_screen);
        let connections_screen = Rc::clone(&self.connections_screen);
        let settings_modal = self.settings_modal;
        let doctor_state = self.doctor_state.clone();
        let show_hidden = self.show_hidden_accounts;
        let account_offset = self.account_offset;
        let bills_screen = &self.bills_screen;
        let payables_screen = &self.payables_screen;
        self.app.draw(move |frame| {
            presentation::shell(frame.ui, current, space, status, modal_open, move |panel| {
                match current {
                    Screen::Overview => screens::overview::draw_overview(panel, &api),
                    Screen::Transactions => screens::transactions::render_with_state(panel, &mut tx_screen.borrow_mut(), &api),
                    Screen::Recurring => screens::recurring::render_with_state(panel, &mut recurring_screen.borrow_mut(), &api),
                    Screen::CashFlow => screens::cash_flow::render(panel, &api),
                    Screen::AccountsCards => screens::accounts_cards::render_scrolled(panel, &api, show_hidden, account_offset),
                    Screen::Categories => screens::categories::render_with_state(panel, &mut categories_screen.borrow_mut(), &api),
                    Screen::Goals => screens::goals::render_with_state(panel, &mut goals_screen.borrow_mut(), &api),
                    Screen::Projection => screens::projection::render(panel, &api),
                    Screen::NetWorth => screens::net_worth::render(panel, &api),
                    Screen::Bills => screens::bills::render_with_state(panel, bills_screen),
                    Screen::Payables => screens::payables::render_with_state(panel, &api, payables_screen),
                    Screen::Tags => screens::tags::render_with_state(panel, &mut tags_screen.borrow_mut(), &api),
                    Screen::Notifications => screens::notifications::render_with_state(panel, &mut notifications_screen.borrow_mut(), &api),
                    Screen::Connections => screens::connections::render_with_state(panel, &mut connections_screen.borrow_mut(), &api),
                    Screen::Settings => screens::settings::render(panel, &api, settings_modal),
                    Screen::Doctor => screens::doctor::render(panel, &doctor_state),
                }
            });
            if show_help {
                frame.ui.modal(ModalOptions {
                    title: Some(format!("Ajuda · {}", current.label())),
                    width: Some(80),
                    align: Some(hqtui::unicode::Align::Left),
                    message: Some(format!("{}\n\nTab / Shift+Tab  Próxima / anterior tela\n1–9,0,-,t,N,x,=,D  Ir para tela\nr Atualizar  ? Ajuda  q Sair\n\nFormulários: Tab / Shift+Tab campos\nEnter / Ctrl+S salvar  Esc voltar\n\nEsc ou Enter para fechar", current.help())),
                    buttons: vec![],
                    ..Default::default()
                });
            }
        })?;
        api_after_draw.set_fetch_allowed(false);
        self.enter_pending = false;
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
        let selected_space = self.selected_space;
        let visible_rows = self.app.terminal.size().rows.saturating_sub(8).max(1);
        let first_space = selected_space.saturating_sub(visible_rows - 1);
        let items: Vec<String> = spaces
            .iter()
            .enumerate()
            .map(|(i, s)| format!(" {}. {} ({})", i + 1, s.name, s.id))
            .collect();

        self.app.draw(move |f| {
            f.ui.column(Column::new().gap(1), move |col| {
                col.panel(Panel::new().title("Selecionar Espa\u{e7}o"), move |p| {
                    p.text("Selecione um espa\u{e7}o para continuar:");
                    p.divider(DividerOptions::default());
                    for (i, item) in items
                        .iter()
                        .enumerate()
                        .skip(first_space)
                        .take(visible_rows)
                    {
                        let id = format!("space:{i}");
                        p.button(
                            ButtonOptions::new(item).variant(if i == selected_space {
                                ButtonVariant::Primary
                            } else {
                                ButtonVariant::Ghost
                            }),
                            &id,
                        );
                    }
                    p.divider(DividerOptions::default());
                    p.text("↑/↓ Selecionar | Enter Abrir | Esc Voltar");
                });
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
                "Duplicate shortcut key for {scr:?}"
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
        assert_eq!(Screen::all().len(), 16);
    }

    #[test]
    fn test_payables_has_distinct_key() {
        let payables = Screen::Payables;
        assert_ne!(payables.shortcut_key(), "1");
        assert_eq!(payables.shortcut_key(), "-");
    }

    #[test]
    fn test_load_env_token_missing_is_ok() {
        assert!(parse_env_token("").is_none());
        assert!(parse_env_token("# no credential\nOTHER=value").is_none());
    }
}
fn pressed_key(events: &[InputEvent], name: &str) -> bool {
    events.iter().any(
        |event| matches!(event, InputEvent::Key(key) if key.name == name && !key.ctrl && !key.alt),
    )
}
