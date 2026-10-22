use hqtui::input::KeyEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
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
    Tags,
    Notifications,
    Connections,
    Settings,
    Doctor,
}

impl Screen {
    #[must_use]
    pub fn label(self) -> &'static str {
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
            Self::Tags => "Tags",
            Self::Notifications => "Notifica\u{e7}\u{f5}es",
            Self::Connections => "Conex\u{f5}es",
            Self::Settings => "Configura\u{e7}\u{f5}es",
            Self::Doctor => "Diagn\u{f3}stico",
        }
    }

    #[must_use]
    pub fn shortcut_key(self) -> &'static str {
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
            Self::Tags => "t",
            Self::Notifications => "N",
            Self::Connections => "x",
            Self::Settings => "=",
            Self::Doctor => "D",
        }
    }

    #[must_use]
    pub fn all() -> [Screen; 16] {
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
            Self::Tags,
            Self::Notifications,
            Self::Connections,
            Self::Settings,
            Self::Doctor,
        ]
    }

    #[must_use]
    pub fn from_index(i: usize) -> Option<Screen> {
        Screen::all().get(i).copied()
    }

    #[must_use]
    pub fn index(self) -> usize {
        Screen::all().iter().position(|s| *s == self).unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalAction {
    Screen(Screen),
    Help,
    Quit,
    Refresh,
    PreviousMonth,
    NextMonth,
    CurrentMonth,
    PeriodPicker,
}

#[must_use]
pub fn global_action(current: Screen, key: &KeyEvent, modal_open: bool) -> Option<GlobalAction> {
    if modal_open || key.ctrl || key.alt {
        return None;
    }
    let value = key.char.as_deref().unwrap_or(&key.name);
    match value {
        "?" => Some(GlobalAction::Help),
        "q" => Some(GlobalAction::Quit),
        "r" => Some(GlobalAction::Refresh),
        "[" => Some(GlobalAction::PreviousMonth),
        "]" => Some(GlobalAction::NextMonth),
        "T" => Some(GlobalAction::CurrentMonth),
        "p" => Some(GlobalAction::PeriodPicker),
        "h" | "left" if !current.claims_period_alias(value) => Some(GlobalAction::PreviousMonth),
        "l" | "right" if !current.claims_period_alias(value) => Some(GlobalAction::NextMonth),
        _ if key.name == "tab" || key.name == "backtab" => {
            let count = Screen::all().len();
            let step = if key.shift || key.name == "backtab" {
                count - 1
            } else {
                1
            };
            Screen::from_index((current.index() + step) % count).map(GlobalAction::Screen)
        }
        _ => Screen::all()
            .into_iter()
            .find(|screen| screen.shortcut_key() == value)
            .map(GlobalAction::Screen),
    }
}

impl Screen {
    #[must_use]
    pub fn claims_period_alias(self, key: &str) -> bool {
        matches!(self, Self::Bills | Self::Recurring | Self::Payables)
            || key == "h" && matches!(self, Self::AccountsCards | Self::Categories)
            || key == "l" && self == Self::Settings
    }

    #[must_use]
    pub fn uses_period(self) -> bool {
        matches!(
            self,
            Self::Overview
                | Self::Transactions
                | Self::CashFlow
                | Self::Categories
                | Self::Bills
                | Self::Payables
        )
    }

    #[must_use]
    pub fn help(self) -> &'static str {
        match self {
            Self::Transactions => {
                "j/k ou ↑/↓  Selecionar\n/ Buscar  f Filtros  c Limpar filtros\nn Nova  e Editar  d Excluir\nEnter Detalhes  E CSV\nTotais por moeda original, sem ignorados; R receitas, D despesas"
            }
            Self::Recurring => {
                "j/k ou ↑/↓  Selecionar\n←/→  Assinaturas / Recorrentes\nn Nova assinatura  e Editar  d Excluir"
            }
            Self::Tags | Self::Goals => "j/k ou ↑/↓  Selecionar\nn Criar  e Editar  d Excluir",
            Self::Categories => {
                "j/k ou ↑/↓  Selecionar\nn Criar  e Editar  d Excluir\nh Mostrar categorias ocultas"
            }
            Self::AccountsCards => "h Mostrar contas ocultas\nj/k ou ↑/↓  Rolar contas e cartões",
            Self::Bills => "←/→  Selecionar cartão\nj/k ou ↑/↓  Selecionar fatura",
            Self::Payables => "←/→  Todos / Ativos / Concluídos\nj/k ou ↑/↓  Selecionar parcela",
            Self::Notifications => {
                "j/k ou ↑/↓  Selecionar\nu Somente não lidas\nm ou Enter  Marcar como lida"
            }
            Self::Connections => "j/k ou ↑/↓  Selecionar conexão",
            Self::Settings => "j/k Rolar informações\ns Trocar espaço  l Sair da conta",
            Self::Doctor => "r Executar diagnóstico novamente",
            _ => "j/k ou ↑/↓ Rolar informações\nr Atualizar dados",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(name: &str) -> KeyEvent {
        KeyEvent {
            name: name.into(),
            key: name.into(),
            char: Some(name.into()),
            ctrl: false,
            alt: false,
            shift: false,
            raw: name.into(),
        }
    }

    #[test]
    fn shortcuts_reach_every_screen_without_stealing_actions() {
        let mut keys = std::collections::HashSet::new();
        for screen in Screen::all() {
            assert!(keys.insert(screen.shortcut_key()));
            assert_eq!(
                global_action(Screen::Overview, &key(screen.shortcut_key()), false),
                Some(GlobalAction::Screen(screen))
            );
        }
        for local in ["n", "e", "d", "j", "k", "up", "down", "/", "f"] {
            assert_eq!(
                global_action(Screen::Transactions, &key(local), false),
                None,
                "{local}"
            );
        }
    }

    #[test]
    fn modal_typing_never_triggers_global_actions() {
        for value in [
            "q", "?", "r", "n", "d", "1", "0", "N", "D", "tab", "[", "]", "T", "p", "h", "l",
        ] {
            assert_eq!(global_action(Screen::Transactions, &key(value), true), None);
        }
    }

    #[test]
    fn period_keys_work_everywhere_and_aliases_preserve_local_actions() {
        for screen in Screen::all() {
            for (value, action) in [
                ("[", GlobalAction::PreviousMonth),
                ("]", GlobalAction::NextMonth),
                ("T", GlobalAction::CurrentMonth),
                ("p", GlobalAction::PeriodPicker),
            ] {
                assert_eq!(global_action(screen, &key(value), false), Some(action));
            }
            for value in ["h", "l", "left", "right"] {
                assert_eq!(
                    global_action(screen, &key(value), false).is_some(),
                    !screen.claims_period_alias(value)
                );
            }
        }
    }

    #[test]
    fn tab_cycles_all_screens_in_both_directions() {
        let mut tab = key("tab");
        tab.char = None;
        assert_eq!(
            global_action(Screen::Doctor, &tab, false),
            Some(GlobalAction::Screen(Screen::Overview))
        );
        tab.shift = true;
        assert_eq!(
            global_action(Screen::Overview, &tab, false),
            Some(GlobalAction::Screen(Screen::Doctor))
        );
        for (i, screen) in Screen::all().into_iter().enumerate() {
            assert_eq!(Screen::from_index(i), Some(screen));
            assert_eq!(screen.index(), i);
        }
    }
}
