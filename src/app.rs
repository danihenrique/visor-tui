use hqtui::prelude::*;

use crate::api::ApiClient;
use crate::screens;

/// Main application state and event loop.
pub struct VisorApp {
    app: App,
    screen: Screen,
    api: ApiClient,
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
}

impl Screen {
    fn label(self) -> &'static str {
        match self {
            Self::Overview => "Visão Geral",
            Self::Transactions => "Transações",
            Self::Recurring => "Recorrentes",
            Self::CashFlow => "Fluxo de Caixa",
            Self::AccountsCards => "Contas e Cartões",
            Self::Categories => "Categorias",
            Self::Goals => "Metas",
            Self::Projection => "Projeção",
            Self::NetWorth => "Patrimônio",
            Self::Bills => "Faturas",
            Self::Payables => "Contas a pagar",
        }
    }

    fn all() -> [Screen; 11] {
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
        ]
    }
}

impl VisorApp {
    pub fn new() -> std::io::Result<Self> {
        let app = App::new()?;
        let api = ApiClient::new().map_err(|e| std::io::Error::other(e))?;
        Ok(Self {
            app,
            screen: Screen::Overview,
            api,
        })
    }

    pub fn run(&mut self) -> std::io::Result<()> {
        while self.app.running() {
            self.app.poll()?;
            self.handle_input();
            self.draw()?;
        }
        Ok(())
    }

    fn handle_input(&mut self) {
        // Check sidebar clicks
        for (i, scr) in Screen::all().iter().enumerate() {
            let id = format!("nav:{i}");
            if self.app.pressed(&id) {
                self.screen = *scr;
            }
        }
    }

    fn draw(&mut self) -> std::io::Result<()> {
        let current = self.screen;
        let screens = Screen::all();
        let nav_labels: Vec<String> = screens
            .iter()
            .enumerate()
            .map(|(i, scr)| format!(" {} {}", (i + 1) % 10, scr.label()))
            .collect();

        self.app.draw(|f| {
            f.ui.row(Row::new().gap(0), |row| {
                // Sidebar
                row.column(Column::new().size(Size::Cells(22)).gap(0), |col| {
                    col.panel(Panel::new().title("Visor"), |p| {
                        p.heading("Visor Finance");
                        p.divider(DividerOptions::default());
                        for (i, label) in nav_labels.iter().enumerate() {
                            let _id = format!("nav:{i}");
                            let is_active = screens[i] == current;
                            if is_active {
                                p.styled_text(label, TextStyle::default().bold());
                            } else {
                                p.text(label);
                            }
                        }
                    });
                });

                // Main content
                row.column(Column::new().size(Size::Fill).gap(1), |col| {
                    match current {
                        Screen::Overview => {
                            screens::overview::draw_overview(col, &self.api);
                        }
                        Screen::AccountsCards => {
                            screens::accounts_cards::render(col, &self.api, false);
                        }
                        _ => {
                            let label = current.label();
                            col.panel(Panel::new().title(label), move |p| {
                                p.heading(label);
                                p.text("Tela em desenvolvimento — disponivel em breve.");
                            });
                        }
                    }
                });
            });

            // Status bar
            f.ui.status_bar(StatusBarOptions {
                items: vec![
                    StatusItem::new("Quit").key("q"),
                    StatusItem::new("Nav").key("1-0"),
                ],
                right: vec![],
                ..Default::default()
            });
        })?;
        Ok(())
    }
}