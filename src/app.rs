use crate::interaction::{Control, Interactions, Interactive, key_event};
use std::cell::RefCell;
use std::rc::Rc;

use hqtui::input::InputEvent;
use hqtui::prelude::*;

use crate::api::{ApiClient, TransactionFilters, TransactionQuery};
use crate::auth;
use crate::config;
use crate::keymap::{GlobalAction, Screen, global_action};
use crate::period::{Period, brasilia_today};
use crate::presentation;
use crate::screens;
use crate::screens::period_picker::{PeriodPicker, PickerOutcome};

#[allow(clippy::struct_excessive_bools)]
pub struct VisorApp {
    app: App,
    hits: Interactions,
    rendered_size: Option<(usize, usize)>,
    screen: Screen,
    api: Option<Rc<ApiClient>>,
    show_help: bool,
    screen_selector: Option<ScreenSelector>,
    login_token_input: String,
    login_error: Option<String>,
    space_selector_visible: bool,
    selected_space: usize,
    enter_pending: bool,
    period: Period,
    period_picker: Option<PeriodPicker>,
    period_transactions: Result<std::sync::Arc<[crate::models::Transaction]>, String>,
    overview: Result<crate::models::Overview, String>,
    show_hidden_accounts: bool,
    account_selection: presentation::TableSelection,
    summary_selection: presentation::TableSelection,
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

#[derive(Clone, Copy)]
struct ScreenSelector {
    selected: usize,
}

#[derive(Debug, PartialEq, Eq)]
enum SelectorOutcome {
    Editing,
    Cancelled,
    Selected(Screen),
}

impl ScreenSelector {
    fn handle_input(&mut self, event: &InputEvent, hits: &Interactions) -> SelectorOutcome {
        let key =
            match event {
                InputEvent::Key(key) => key.name.as_str(),
                InputEvent::Mouse(mouse) => {
                    match hits.click(event) {
                        Some(Control::Field(index)) => {
                            return Screen::from_index(index)
                                .map_or(SelectorOutcome::Editing, SelectorOutcome::Selected);
                        }
                        Some(Control::Button(_)) => "escape",
                        _ if mouse.action == hqtui::input::MouseAction::Scroll => {
                            if mouse.scroll > 0 { "down" } else { "up" }
                        }
                        _ => return SelectorOutcome::Editing,
                    }
                }
                _ => return SelectorOutcome::Editing,
            };
        match key {
            "escape" => SelectorOutcome::Cancelled,
            "enter" => Screen::from_index(self.selected)
                .map_or(SelectorOutcome::Editing, SelectorOutcome::Selected),
            "down" | "j" => {
                self.selected = (self.selected + 1).min(Screen::all().len() - 1);
                SelectorOutcome::Editing
            }
            "up" | "k" => {
                self.selected = self.selected.saturating_sub(1);
                SelectorOutcome::Editing
            }
            _ => SelectorOutcome::Editing,
        }
    }

    fn render(self, ui: &mut Container<'_>, hits: Interactions) {
        ui.sized(Size::Cells(1), move |ui| {
            ui.draw(move |surface| {
                use crate::interaction::coord;
                use hqtui::{
                    surface::TextOptions,
                    widgets::{draw_button, draw_modal},
                };

                hits.clear();
                let root = Surface::root(Rc::clone(surface.buffer()), Rc::clone(&surface.theme));
                let screens = Screen::all();
                let inner = draw_modal(
                    &root,
                    &ModalOptions {
                        title: Some("Telas".into()),
                        width: Some(50),
                        height: Some(screens.len() + 5),
                        align: Some(Align::Left),
                        ..Default::default()
                    },
                );
                let capacity = inner.height().saturating_sub(3);
                let offset = self.selected.saturating_sub(capacity.saturating_sub(1));
                for (index, screen) in screens.iter().enumerate().skip(offset).take(capacity) {
                    let row = inner.sub(
                        1,
                        coord(index - offset + 1),
                        inner.width().saturating_sub(2),
                        1,
                    );
                    row.text(
                        0,
                        0,
                        &format!(
                            "{} {}",
                            if index == self.selected { "›" } else { " " },
                            screen.label()
                        ),
                        &TextOptions::new().fg(if index == self.selected {
                            inner.theme.accent
                        } else {
                            inner.theme.foreground
                        }),
                    );
                    hits.add(&row, Control::Field(index));
                }
                let width = hqtui::unicode::string_width("Cancelar") + 4;
                let button = inner.sub(
                    coord(inner.width().saturating_sub(width) / 2),
                    coord(inner.height().saturating_sub(2)),
                    width,
                    1,
                );
                let drawn_width = draw_button(&button, &ButtonOptions::new("Cancelar"));
                hits.add(
                    &button.sub(0, 0, drawn_width, 1),
                    Control::Button("escape".into()),
                );
            });
        });
    }
}

impl VisorApp {
    pub fn new() -> std::io::Result<Self> {
        let mut app = App::with_options(AppOptions {
            terminal: hqtui::terminal::TerminalOptions {
                mouse: Some(true),
                capabilities: hqtui::capabilities::CapabilityOverrides {
                    mouse: Some(true),
                    ..Default::default()
                },
                ..Default::default()
            },
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
            hits: Interactions::default(),
            rendered_size: None,
            screen: Screen::Overview,
            api,
            show_help: false,
            screen_selector: None,
            login_token_input: String::new(),
            login_error: None,
            space_selector_visible,
            selected_space: 0,
            enter_pending: true,
            period: Period::current_month(brasilia_today()),
            period_picker: None,
            period_transactions: Err("Carregando…".into()),
            overview: Err("Carregando…".into()),
            show_hidden_accounts: false,
            account_selection: presentation::TableSelection::default(),
            summary_selection: presentation::TableSelection::default(),
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
            let size = self.app.terminal.size();
            if self.rendered_size != Some((size.columns, size.rows)) {
                self.transactions_screen.borrow_mut().invalidate_mouse();
                self.draw()?;
            }
            for event in events {
                self.handle_input(std::slice::from_ref(&event));
                self.draw()?;
            }
            self.draw()?;
        }
        Ok(())
    }

    fn handle_input(&mut self, events: &[InputEvent]) {
        for event in events {
            if let Some(selector) = &mut self.screen_selector {
                match selector.handle_input(event, &self.hits) {
                    SelectorOutcome::Editing => {}
                    SelectorOutcome::Cancelled => self.screen_selector = None,
                    SelectorOutcome::Selected(screen) => {
                        self.screen_selector = None;
                        self.enter_screen(screen);
                    }
                }
                continue;
            }
            if matches!(event, InputEvent::Mouse(_)) && self.period_picker.is_none() {
                self.handle_mouse(event);
                continue;
            }
            if let Some(picker) = &mut self.period_picker {
                match picker.handle_input(std::slice::from_ref(event)) {
                    PickerOutcome::Editing => {}
                    PickerOutcome::Cancelled => self.period_picker = None,
                    PickerOutcome::Accepted(period) => {
                        self.period_picker = None;
                        self.set_period(period);
                    }
                }
                continue;
            }
            if let InputEvent::Paste(paste) = event {
                if self.api.is_none() {
                    self.login_token_input.push_str(paste.text.trim());
                } else if self.modal_open() && !self.show_help && !self.space_selector_visible {
                    if self.screen == Screen::Transactions {
                        self.dispatch_screen(std::slice::from_ref(event));
                    } else {
                        match self.screen {
                            Screen::Tags => self.tags_screen.borrow_mut().paste_text(&paste.text),
                            Screen::Categories => {
                                self.categories_screen.borrow_mut().paste_text(&paste.text);
                            }
                            Screen::Goals => self.goals_screen.borrow_mut().paste_text(&paste.text),
                            Screen::Recurring => {
                                self.recurring_screen.borrow_mut().paste_text(&paste.text);
                            }
                            _ => {}
                        }
                    }
                }
                continue;
            }
            if let InputEvent::Key(key) = event {
                self.transactions_screen.borrow_mut().reset_click_sequence();
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
                    Some(GlobalAction::PreviousMonth) => {
                        self.set_period(self.period.previous_month());
                    }
                    Some(GlobalAction::NextMonth) => self.set_period(self.period.next_month()),
                    Some(GlobalAction::CurrentMonth) => {
                        self.set_period(Period::current_month(brasilia_today()));
                    }
                    Some(GlobalAction::PeriodPicker) => {
                        self.period_picker = Some(PeriodPicker::new(&self.period));
                    }
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
    }

    fn handle_mouse(&mut self, event: &InputEvent) {
        let events = std::slice::from_ref(event);
        if self.show_help {
            if self.hits.pressed(events, "enter") || self.hits.pressed(events, "escape") {
                self.show_help = false;
            }
            return;
        }
        if self.space_selector_visible {
            if let Some(Control::Button(id)) = self.hits.click(event)
                && let Some(index) = id
                    .strip_prefix("space:")
                    .and_then(|index| index.parse().ok())
            {
                self.select_space(index);
            }
            if let InputEvent::Mouse(mouse) = event
                && mouse.action == hqtui::input::MouseAction::Scroll
            {
                self.selected_space = self
                    .selected_space
                    .saturating_add_signed(if mouse.scroll > 0 { 1 } else { -1 })
                    .min(self.spaces.len().saturating_sub(1));
            }
            return;
        }
        if self.api.is_none() {
            if self.hits.pressed(events, "login")
                && let InputEvent::Key(key) = key_event("enter")
            {
                self.handle_login_key(&key);
            }
            return;
        }
        if self.modal_open() {
            self.dispatch_screen(events);
            return;
        }
        if let Some(Control::Button(id)) = self.hits.click(event) {
            self.transactions_screen.borrow_mut().reset_click_sequence();
            match id.as_str() {
                "nav:picker" => {
                    self.screen_selector = Some(ScreenSelector {
                        selected: self.screen.index(),
                    });
                }
                "period:previous" => self.set_period(self.period.previous_month()),
                "period:next" => self.set_period(self.period.next_month()),
                "period:picker" => self.period_picker = Some(PeriodPicker::new(&self.period)),
                _ => {
                    if let Some(screen) = id
                        .strip_prefix("nav:")
                        .and_then(|index| index.parse().ok())
                        .and_then(Screen::from_index)
                    {
                        self.enter_screen(screen);
                    }
                }
            }
            return;
        }
        self.dispatch_screen(events);
    }

    fn set_period(&mut self, period: Period) {
        if self.period != period {
            self.transactions_screen.borrow_mut().invalidate_mouse();
            self.period = period;
            self.period_transactions = Err("Carregando…".into());
            self.summary_selection = presentation::TableSelection::default();
            if self.screen.uses_period() {
                self.enter_pending = true;
            }
        }
    }

    fn enter_screen(&mut self, screen: Screen) {
        if self.screen != screen {
            self.transactions_screen.borrow_mut().invalidate_mouse();
            self.screen = screen;
            self.summary_selection = presentation::TableSelection::default();
            self.enter_pending = true;
        }
    }

    fn modal_open(&self) -> bool {
        if self.period_picker.is_some() || self.screen_selector.is_some() {
            return true;
        }
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
        if matches!(
            self.screen,
            Screen::Overview
                | Screen::CashFlow
                | Screen::Projection
                | Screen::NetWorth
                | Screen::Settings
        ) && !self.modal_open()
        {
            self.summary_selection.handle_mouse(events);
            if pressed_key(events, "j") || pressed_key(events, "down") {
                self.summary_selection.move_by(1);
            }
            if pressed_key(events, "k") || pressed_key(events, "up") {
                self.summary_selection.move_by(-1);
            }
        }
        match self.screen {
            Screen::Transactions => self
                .transactions_screen
                .borrow_mut()
                .handle_input(&api, events),
            Screen::Tags => self.tags_screen.borrow_mut().handle_input(&api, events),
            Screen::Goals => self.goals_screen.borrow_mut().handle_input(&api, events),
            Screen::Categories => self
                .categories_screen
                .borrow_mut()
                .handle_input(&api, events),
            Screen::Recurring => self
                .recurring_screen
                .borrow_mut()
                .handle_input(&api, events),
            Screen::Notifications => self
                .notifications_screen
                .borrow_mut()
                .handle_input(&api, events),
            Screen::Connections => self
                .connections_screen
                .borrow_mut()
                .handle_input(&api, events),
            Screen::Bills => self.bills_screen.handle_input(events),
            Screen::Payables => self.payables_screen.handle_input(events),
            Screen::AccountsCards => {
                self.account_selection.handle_mouse(events);
                if pressed_key(events, "h")
                    || self.account_selection.hits.pressed(events, "toggle_hidden")
                {
                    self.show_hidden_accounts = !self.show_hidden_accounts;
                    self.enter_pending = true;
                }
                if pressed_key(events, "j") || pressed_key(events, "down") {
                    self.account_selection.move_by(1);
                }
                if pressed_key(events, "k") || pressed_key(events, "up") {
                    self.account_selection.move_by(-1);
                }
            }
            Screen::Doctor => {
                if self.summary_selection.hits.pressed(events, "doctor_rerun") {
                    self.enter_pending = true;
                }
            }
            Screen::Settings => self.handle_settings(events),
            _ => {}
        }
        self.enter_pending |= match self.screen {
            Screen::Transactions => self.transactions_screen.borrow().reload_requested,
            Screen::Bills => self.bills_screen.reload_requested,
            Screen::Categories => self.categories_screen.borrow().reload_requested,
            _ => false,
        };
        api.set_fetch_allowed(false);
    }

    fn handle_settings(&mut self, events: &[InputEvent]) {
        let modal_events;
        let events = if self.settings_modal == screens::settings::SettingsModal::LogoutConfirm {
            modal_events = self.summary_selection.hits.modal_events(events);
            modal_events.as_slice()
        } else {
            events
        };
        match self.settings_modal {
            screens::settings::SettingsModal::None => {
                if pressed_key(events, "l") || self.summary_selection.hits.pressed(events, "logout")
                {
                    self.settings_modal = screens::settings::SettingsModal::LogoutConfirm;
                }
                if pressed_key(events, "s")
                    || self.summary_selection.hits.pressed(events, "change_space")
                {
                    self.space_selector_visible = true;
                }
            }
            screens::settings::SettingsModal::LogoutConfirm => {
                if pressed_key(events, "escape")
                    || pressed_key(events, "n")
                    || self.summary_selection.hits.pressed(events, "Cancelar")
                {
                    self.settings_modal = screens::settings::SettingsModal::None;
                } else if pressed_key(events, "enter")
                    || pressed_key(events, "y")
                    || self.summary_selection.hits.pressed(events, "Sair")
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
        let Some(api) = &self.api else { return };
        let client = match ApiClient::with_space(api.credential().clone(), &space.id) {
            Ok(client) => client,
            Err(error) => {
                self.login_error = Some(format!("Falha ao selecionar espaço: {error}"));
                return;
            }
        };
        if let Err(error) = config::set_default_space_id(&space.id) {
            self.login_error = Some(format!("Falha ao selecionar espaço: {error}"));
            return;
        }
        self.api = Some(Rc::new(client));
        self.selected_space = index;
        self.login_error = None;
        self.transactions_screen = Rc::new(RefCell::new(
            screens::transactions::TransactionsScreen::new(),
        ));
        self.tags_screen = Rc::new(RefCell::new(screens::tags::TagsScreen::new()));
        self.goals_screen = Rc::new(RefCell::new(screens::goals::GoalsScreen::new()));
        self.categories_screen =
            Rc::new(RefCell::new(screens::categories::CategoriesScreen::new()));
        self.recurring_screen = Rc::new(RefCell::new(screens::recurring::RecurringScreen::new()));
        self.notifications_screen = Rc::new(RefCell::new(
            screens::notifications::NotificationsScreen::new(),
        ));
        self.connections_screen =
            Rc::new(RefCell::new(screens::connections::ConnectionsScreen::new()));
        self.bills_screen = screens::bills::BillsScreen::default();
        self.payables_screen = screens::payables::PayablesScreen::default();
        self.doctor_state = screens::doctor::DoctorState::new();
        self.settings_modal = screens::settings::SettingsModal::None;
        self.show_hidden_accounts = false;
        self.account_selection = presentation::TableSelection::default();
        self.summary_selection = presentation::TableSelection::default();
        self.space_selector_visible = false;
        self.enter_pending = true;
    }

    fn load_screen(&mut self) {
        let Some(api) = self.api.as_ref() else { return };
        api.set_fetch_allowed(true);
        if matches!(
            self.screen,
            Screen::Overview | Screen::CashFlow | Screen::Categories
        ) {
            self.period_transactions = api
                .transactions_in_range(&TransactionQuery {
                    range: self.period.range(),
                    filters: TransactionFilters::default(),
                })
                .map_err(|error| error.to_string());
        }
        match self.screen {
            Screen::Overview | Screen::CashFlow => {
                self.overview = api.overview().map_err(|error| error.to_string());
            }
            Screen::Transactions => self
                .transactions_screen
                .borrow_mut()
                .on_enter(api, &self.period),
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
            Screen::Bills => self.bills_screen.load(api, self.period.range()),
            Screen::Payables => self.payables_screen.load(api, self.period.range()),
            Screen::Doctor => {
                self.doctor_state.run_checks(Some(api));
            }
            Screen::AccountsCards => {
                let _ = api.accounts(self.show_hidden_accounts);
                let _ = api.cards();
            }
            Screen::Projection => {
                let _ = api.overview();
            }
            Screen::NetWorth => {
                let _ = api.investments();
            }
            Screen::Settings => {
                let _ = api.me();
                let _ = api.spaces();
            }
        }
        api.set_fetch_allowed(false);
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
        self.hits.clear();
        let size = self.app.terminal.size();
        self.rendered_size = Some((size.columns, size.rows));
        if self.api.is_none() {
            return self.draw_login_screen();
        }
        if self.space_selector_visible {
            return self.draw_space_selector();
        }
        if self.enter_pending {
            let current = self.screen;
            let period = self.period.label();
            self.app.draw(move |frame| {
                presentation::shell(
                    frame.ui,
                    current,
                    "Visor Finance".into(),
                    "Sincronizando dados…".into(),
                    period,
                    false,
                    |panel| {
                        panel.text("Carregando…");
                    },
                );
            })?;
            self.load_screen();
        }

        let hits = self.hits.clone();
        let help_hits = self.hits.clone();
        let current = self.screen;
        let period = self.period;
        let period_picker = self.period_picker.clone();
        let period_transactions = self.period_transactions.clone();
        let overview = self.overview.clone();
        let show_help = self.show_help;
        let screen_selector = self.screen_selector;
        let menu_hits = self.hits.clone();
        let modal_open = self.modal_open() || show_help;
        let api = Rc::clone(self.api.as_ref().expect("authenticated screen"));
        let api_after_draw = Rc::clone(&api);
        let active_space = api.space_id();
        let space = self
            .spaces
            .iter()
            .find(|space| Some(space.id.as_str()) == active_space)
            .map_or_else(
                || active_space.map_or_else(|| "Nenhum espaço".into(), |id| format!("Espaço {id}")),
                |space| space.name.clone(),
            );
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
        let account_selection = &mut self.account_selection;
        let summary_selection = &mut self.summary_selection;
        account_selection.hits.clear();
        summary_selection.hits.clear();
        let bills_screen = &self.bills_screen;
        let payables_screen = &mut self.payables_screen;
        self.app.draw(move |frame| {
            presentation::shell_interactive(frame.ui, current, space, status, period.label(), modal_open, hits, move |panel| {
                match current {
                    Screen::Overview => crate::interaction::region(panel, summary_selection.hits.clone(), Control::Scroll, move |panel| screens::overview::draw_selected_period(panel, &overview, &period_transactions, summary_selection)),
                    Screen::Transactions => screens::transactions::render_with_state(panel, &mut tx_screen.borrow_mut(), &api),
                    Screen::Recurring => screens::recurring::render_with_state(panel, &mut recurring_screen.borrow_mut(), &api),
                    Screen::CashFlow => crate::interaction::region(panel, summary_selection.hits.clone(), Control::Scroll, move |panel| screens::cash_flow::render_selected_period(panel, &overview, &period_transactions, summary_selection)),
                    Screen::AccountsCards => crate::interaction::region(panel, account_selection.hits.clone(), Control::Scroll, move |panel| screens::accounts_cards::render_scrolled(panel, &api, show_hidden, account_selection)),
                    Screen::Categories => screens::categories::render_with_period(panel, &mut categories_screen.borrow_mut(), &period_transactions),
                    Screen::Goals => screens::goals::render_with_state(panel, &mut goals_screen.borrow_mut(), &api),
                    Screen::Projection => crate::interaction::region(panel, summary_selection.hits.clone(), Control::Scroll, move |panel| screens::projection::render_scrolled(panel, &api, summary_selection)),
                    Screen::NetWorth => crate::interaction::region(panel, summary_selection.hits.clone(), Control::Scroll, move |panel| screens::net_worth::render_scrolled(panel, &api, summary_selection)),
                    Screen::Bills => screens::bills::render_with_state(panel, bills_screen),
                    Screen::Payables => screens::payables::render_with_state(panel, payables_screen),
                    Screen::Tags => screens::tags::render_with_state(panel, &mut tags_screen.borrow_mut(), &api),
                    Screen::Notifications => screens::notifications::render_with_state(panel, &mut notifications_screen.borrow_mut(), &api),
                    Screen::Connections => screens::connections::render_with_state(panel, &mut connections_screen.borrow_mut(), &api),
                    Screen::Settings => {
                        if settings_modal == screens::settings::SettingsModal::None {
                            crate::interaction::region(panel, summary_selection.hits.clone(), Control::Scroll, move |panel| screens::settings::render_scrolled(panel, &api, settings_modal, summary_selection));
                        } else { screens::settings::render_scrolled(panel, &api, settings_modal, summary_selection); }
                    },
                    Screen::Doctor => screens::doctor::render_interactive(panel, &doctor_state, &summary_selection.hits),
                }
            });
            if let Some(picker) = period_picker { picker.render(frame.ui); }
            if let Some(selector) = screen_selector {
                selector.render(frame.ui, menu_hits);
            }
            if show_help {
                frame.ui.mouse_modal(ModalOptions {
                    title: Some(format!("Ajuda · {}", current.label())),
                    width: Some(80),
                    align: Some(hqtui::unicode::Align::Left),
                    message: Some(format!("{}\n\nTab / Shift+Tab  Próxima / anterior tela\n1–9,0,-,t,N,x,=,D  Ir para tela\nr Atualizar  ? Ajuda  q Sair\n[ / ] Mês anterior / seguinte  T Mês atual\np Período: Mês, Semana, Trimestre, Ano, Personalizado\n←/→ ou h/l mudam o mês quando não usados pela tela\nTotais separados por moeda original; sem conversão cambial.\nIgnorados não entram nos totais. R receitas / D despesas.\n\nFormulários: Tab / Shift+Tab campos\nEnter / Ctrl+S salvar  Esc voltar\n\nEsc ou Enter para fechar", current.help())),
                    buttons: vec![ModalButton::new("Fechar")],
                    ..Default::default()
                }, &[], &help_hits);
            }
        })?;
        api_after_draw.set_fetch_allowed(false);
        self.enter_pending = false;
        Ok(())
    }

    fn draw_login_screen(&mut self) -> std::io::Result<()> {
        let token_display = "*".repeat(self.login_token_input.len());
        let error_msg = self.login_error.clone();
        let hits = self.hits.clone();

        self.app.draw(move |f| {
            f.ui.column(Column::new().gap(1), move |col| {
                col.panel(Panel::new().title("Login - Visor Finance"), move |p| {
                    p.text("Digite seu token pessoal (PAT):");
                    p.text(&format!("Token: {token_display}_"));
                    if let Some(ref err) = error_msg {
                        p.styled_text(err, TextStyle::new().fg(Color::rgb(239, 68, 68)));
                    }
                    p.divider(DividerOptions::default());
                    p.sized(Size::Cells(1), move |button| {
                        button.mouse_button(ButtonOptions::new("Entrar · Enter"), "login", &hits);
                    });
                    p.text("Enter para validar | Ctrl+C para sair");
                    p.text("O token ser\u{e1} salvo em ~/.config/visor/credentials.json");
                });
            });
        })?;
        Ok(())
    }

    fn draw_space_selector(&mut self) -> std::io::Result<()> {
        let hits = self.hits.clone();
        let spaces = self.spaces.clone();
        let selected_space = self.selected_space;
        let error = self.login_error.clone();
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
                    if let Some(error) = error {
                        p.text(&error);
                    }
                    p.divider(DividerOptions::default());
                    for (i, item) in items
                        .iter()
                        .enumerate()
                        .skip(first_space)
                        .take(visible_rows)
                    {
                        let id = format!("space:{i}");
                        let item = item.clone();
                        let hits = hits.clone();
                        p.sized(Size::Cells(1), move |button| {
                            button.mouse_button(
                                ButtonOptions::new(item).variant(if i == selected_space {
                                    ButtonVariant::Primary
                                } else {
                                    ButtonVariant::Ghost
                                }),
                                &id,
                                &hits,
                            );
                        });
                    }
                    p.divider(DividerOptions::default());
                    p.text("↑/↓ Selecionar | Enter Abrir | Esc Voltar");
                });
            });
        })?;
        Ok(())
    }
}
fn pressed_key(events: &[InputEvent], name: &str) -> bool {
    events.iter().any(
        |event| matches!(event, InputEvent::Key(key) if key.name == name && !key.ctrl && !key.alt),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::test_support::{click, click_text};
    use hqtui::{input::MouseAction, testing::render_to_text};

    #[test]
    fn screen_menu_scrolls_last_screen_into_view_and_clicks_its_original_index() {
        for (width, height) in [(60, 18), (80, 24), (120, 40)] {
            let hits = Interactions::default();
            let mut selector = ScreenSelector { selected: 0 };
            let text = render_to_text(width, height, "dark", |ui| {
                selector.render(ui, hits.clone());
            });
            let mut wheel = click_text(&text, "Visão Geral");
            if let InputEvent::Mouse(mouse) = &mut wheel {
                mouse.action = MouseAction::Scroll;
                mouse.scroll = 1;
            }
            for _ in 0..15 {
                assert_eq!(
                    selector.handle_input(&wheel, &hits),
                    SelectorOutcome::Editing
                );
            }
            let text = render_to_text(width, height, "dark", |ui| {
                selector.render(ui, hits.clone());
            });
            let event = click_text(&text, "Diagnóstico");
            assert_eq!(
                hits.click(&event),
                Some(Control::Field(Screen::Doctor.index()))
            );
            assert_eq!(
                selector.handle_input(&event, &hits),
                SelectorOutcome::Selected(Screen::Doctor)
            );
            for screen in Screen::all() {
                if text.contains(screen.label()) {
                    assert_eq!(
                        hits.click(&click_text(&text, screen.label())),
                        Some(Control::Field(screen.index()))
                    );
                }
            }
            assert_eq!(
                selector.handle_input(&click_text(&text, "Cancelar"), &hits),
                SelectorOutcome::Cancelled
            );
            assert_eq!(
                selector.handle_input(&key_event("enter"), &hits),
                SelectorOutcome::Selected(Screen::Doctor)
            );
            for (key, index) in [("up", 14), ("k", 13), ("down", 14), ("j", 15)] {
                assert_eq!(
                    selector.handle_input(&key_event(key), &hits),
                    SelectorOutcome::Editing
                );
                assert_eq!(selector.selected, index);
            }
        }
    }

    #[test]
    fn screen_menu_clears_background_targets_and_ignores_outside_clicks() {
        for (width, height) in [(60, 18), (80, 24)] {
            let hits = Interactions::default();
            let mut selector = ScreenSelector { selected: 0 };
            let text = render_to_text(width, height, "dark", |ui| {
                ui.mouse_button(ButtonOptions::new("Background"), "nav:2", &hits);
                selector.render(ui, hits.clone());
            });
            assert_eq!(hits.at(0, 0), None);
            assert_eq!(
                selector.handle_input(&click(0, 0), &hits),
                SelectorOutcome::Editing
            );
            assert_eq!(selector.selected, 0);
            assert_eq!(
                selector.handle_input(&click_text(&text, "Cancelar"), &hits),
                SelectorOutcome::Cancelled
            );
        }
    }

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
