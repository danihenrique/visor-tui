use hqtui::prelude::*;

use crate::api::ApiClient;
use crate::models::InstallmentPlan;
use crate::presentation::{
    Metric, TableSelection, Tone, format_currency, metric_strip, money_total,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Tab {
    #[default]
    All,
    Active,
    Completed,
}

impl Tab {
    fn accepts(self, plan: &InstallmentPlan) -> bool {
        match self {
            Self::All => true,
            Self::Active => matches!(
                plan.status.as_deref(),
                Some("active" | "Active" | "ativo" | "Ativo")
            ),
            Self::Completed => matches!(
                plan.status.as_deref(),
                Some(
                    "completed"
                        | "Completed"
                        | "concluído"
                        | "Concluído"
                        | "concluido"
                        | "Concluido"
                )
            ),
        }
    }

    fn next(self) -> Self {
        match self {
            Self::All => Self::Active,
            Self::Active => Self::Completed,
            Self::Completed => Self::All,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::All => Self::Completed,
            Self::Active => Self::All,
            Self::Completed => Self::Active,
        }
    }
}

fn progress(plan: &InstallmentPlan) -> String {
    match (plan.paid_installments, plan.total_installments) {
        (Some(paid), Some(total)) => format!("{paid}/{total}"),
        (Some(paid), None) => format!("{paid}/?"),
        (None, Some(total)) => format!("?/{total}"),
        (None, None) => "—".into(),
    }
}

fn status_label(plan: &InstallmentPlan) -> &str {
    if Tab::Active.accepts(plan) {
        "Ativo"
    } else if Tab::Completed.accepts(plan) {
        "Concluído"
    } else {
        plan.status.as_deref().unwrap_or("—")
    }
}

#[derive(Default)]
pub struct PayablesScreen {
    tab: Tab,
    selection: TableSelection,
}

impl PayablesScreen {
    pub fn handle_input(&mut self, app: &App, events: &[hqtui::input::InputEvent]) {
        for event in events {
            if let hqtui::input::InputEvent::Key(key) = event {
                match key.name.as_str() {
                    "left" | "h" => {
                        self.tab = self.tab.previous();
                        self.selection = TableSelection::default();
                    }
                    "right" | "l" => {
                        self.tab = self.tab.next();
                        self.selection = TableSelection::default();
                    }
                    "down" | "j" => self.selection.move_by(1),
                    "up" | "k" => self.selection.move_by(-1),
                    _ => {}
                }
            }
        }
        for (tab, id) in [
            (Tab::All, "tab_all"),
            (Tab::Active, "tab_active"),
            (Tab::Completed, "tab_completed"),
        ] {
            if app.pressed(id) {
                self.tab = tab;
                self.selection = TableSelection::default();
            }
        }
    }
}

pub fn render(ui: &mut Container<'_>, api: &ApiClient) {
    render_with_state(ui, api, &mut PayablesScreen::default());
}

pub fn render_with_state(ui: &mut Container<'_>, api: &ApiClient, screen: &mut PayablesScreen) {
    let theme = ui.theme().clone();
    let plans = match api.installment_plans(None) {
        Ok(plans) => plans,
        Err(error) => {
            ui.panel(
                Panel::new()
                    .title("Contas a pagar")
                    .border(BorderStyle::Rounded),
                move |panel| {
                    panel.styled_text(
                        &format!("Não foi possível carregar os parcelamentos. {error}"),
                        TextStyle::new().fg(theme.danger),
                    );
                    panel.label("Pressione r para tentar novamente.");
                },
            );
            return;
        }
    };
    let all_count = plans.len();
    let active_count = plans
        .iter()
        .filter(|plan| Tab::Active.accepts(plan))
        .count();
    let completed_count = plans
        .iter()
        .filter(|plan| Tab::Completed.accepts(plan))
        .count();
    let current: Vec<_> = plans
        .iter()
        .filter(|plan| screen.tab.accepts(plan))
        .collect();
    screen.selection.set_row_count(current.len());
    let selected = screen.selection.selected();
    let detail = selected
        .and_then(|index| current.get(index))
        .map(|plan| (*plan).clone());
    let metrics = vec![
        Metric::new(
            "Saldo restante",
            money_total(current.iter().map(|plan| {
                (
                    plan.remaining_amount.as_deref(),
                    plan.currency_code.as_deref(),
                )
            })),
            Tone::Negative,
        ),
        Metric::new(
            "Total contratado",
            money_total(
                current
                    .iter()
                    .map(|plan| (plan.total_amount.as_deref(), plan.currency_code.as_deref())),
            ),
            Tone::Neutral,
        ),
        Metric::new("Parcelamentos", current.len().to_string(), Tone::Accent),
    ];
    let compact = ui.width() < 85;
    let rows = current
        .into_iter()
        .map(|plan| {
            let mut cells = vec![
                plan.description
                    .clone()
                    .unwrap_or_else(|| "Sem descrição".into()),
                progress(plan),
                format_currency(
                    plan.remaining_amount.as_deref(),
                    plan.currency_code.as_deref(),
                ),
                plan.next_due_date.clone().unwrap_or_else(|| "—".into()),
            ];
            if !compact {
                cells.push(status_label(plan).into());
            }
            let mut row = TableRow::new(cells);
            row.cell_colors = vec![
                None,
                Some(theme.muted),
                Some(theme.danger),
                None,
                Some(if Tab::Completed.accepts(plan) {
                    theme.success
                } else {
                    theme.muted
                }),
            ];
            row
        })
        .collect::<Vec<_>>();
    let active_tab = screen.tab;
    let show_detail = ui.height() >= 16;
    let compact_detail = detail.as_ref().map(|plan| {
        [
            format!(
                "Parcela {} · {}",
                format_currency(
                    plan.installment_amount.as_deref(),
                    plan.currency_code.as_deref()
                ),
                status_label(plan)
            ),
            format!(
                "Total {}",
                format_currency(plan.total_amount.as_deref(), plan.currency_code.as_deref())
            ),
        ]
    });
    ui.column(Column::new().gap(usize::from(show_detail)), move |col| {
        if show_detail {
            metric_strip(col, metrics);
        }
        col.row(Row::new().size(Size::Cells(1)).gap(1), move |row| {
            for (tab, id, label) in [
                (Tab::All, "tab_all", format!("Todos {all_count}")),
                (Tab::Active, "tab_active", format!("Ativos {active_count}")),
                (
                    Tab::Completed,
                    "tab_completed",
                    format!("Concluídos {completed_count}"),
                ),
            ] {
                row.button(
                    ButtonOptions::new(label).variant(if active_tab == tab {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Ghost
                    }),
                    id,
                );
            }
        });
        col.panel(
            Panel::new()
                .title("Contas a pagar")
                .border(BorderStyle::Rounded)
                .footer("←/→ Status  j/k Selecionar  r Atualizar"),
            move |panel| {
                if rows.is_empty() {
                    panel.text("Nenhum parcelamento neste filtro.");
                    panel.label("Use ←/→ para consultar outros status.");
                } else {
                    let mut columns = vec![
                        TableColumn::new("Descrição").width(Size::Fill),
                        TableColumn::new("Pagas")
                            .width(Size::Cells(7))
                            .align(Align::Center),
                        TableColumn::new("Restante")
                            .width(Size::Cells(17))
                            .align(Align::Right),
                        TableColumn::new("Próxima").width(Size::Cells(10)),
                    ];
                    if !compact {
                        columns.push(TableColumn::new("Status").width(Size::Cells(10)));
                    }
                    panel.table(
                        TableOptions {
                            columns,
                            rows,
                            selected,
                            follow_selection: true,
                            scrollbar: true,
                            header_color: Some(theme.muted),
                            ..Default::default()
                        },
                        "payables:table",
                    );
                }
            },
        );
        if show_detail && let Some(plan) = detail {
            col.panel(
                Panel::new()
                    .title(
                        plan.description
                            .clone()
                            .unwrap_or_else(|| "Parcelamento selecionado".into()),
                    )
                    .border(BorderStyle::Rounded)
                    .size(Size::Cells(4)),
                move |panel| {
                    if let (Some(paid), Some(total)) =
                        (plan.paid_installments, plan.total_installments)
                        && total > 0
                    {
                        panel.meter(MeterOptions {
                            value: f64::from(paid) / f64::from(total),
                            label: Some("Pagas".into()),
                            text: Some(progress(&plan)),
                            color: Some(theme.success),
                            ..Default::default()
                        });
                    } else {
                        panel.label("Progresso indisponível");
                    }
                    panel.label(&format!(
                        "Parcela {} · Total {} · {}",
                        format_currency(
                            plan.installment_amount.as_deref(),
                            plan.currency_code.as_deref()
                        ),
                        format_currency(
                            plan.total_amount.as_deref(),
                            plan.currency_code.as_deref()
                        ),
                        status_label(&plan)
                    ));
                },
            );
        }
        if !show_detail && let Some(lines) = compact_detail {
            col.sized(Size::Cells(2), move |details| {
                details.column(Column::new(), move |body| {
                    for line in lines {
                        body.label(&line);
                    }
                });
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_payables_keep_installment_total_and_status_visible() {
        let mut server = mockito::Server::new();
        let mock = server.mock("GET", "/v1/spaces/space/installment-plans").with_status(200).with_body(serde_json::json!({"data":[{"id":"p", "description":"Notebook", "installment_amount":"500", "total_amount":"5000", "remaining_amount":"3000", "paid_installments":4, "total_installments":10, "next_due_date":"2026-11-02", "status":"active"}]}).to_string()).create();
        let api = ApiClient::for_test(
            &server.url(),
            crate::auth::Credential::PersonalToken {
                token: "test-only".into(),
            },
            "space",
        )
        .unwrap();
        let mut screen = PayablesScreen::default();
        for (width, height) in [(56, 10), (60, 18), (80, 24)] {
            let text = hqtui::testing::render_to_text(width, height, "dark", |ui| {
                render_with_state(ui, &api, &mut screen);
            });
            for value in [
                "Notebook",
                "500,00 BRL",
                "5.000,00 BRL",
                "3.000,00 BRL",
                "Ativo",
                "2026-11-02",
            ] {
                assert!(
                    text.contains(value),
                    "Missing {value} at {width}x{height}\n{text}"
                );
            }
        }
        mock.assert();
    }

    #[test]
    fn selection_moves_up_immediately_after_reaching_last_payable() {
        let mut screen = PayablesScreen::default();
        screen.selection.set_row_count(3);
        for _ in 0..40 {
            screen.selection.move_by(1);
        }
        assert_eq!(screen.selection.selected(), Some(2));
        screen.selection.move_by(-1);
        assert_eq!(screen.selection.selected(), Some(1));
        screen.selection.set_row_count(0);
        assert_eq!(screen.selection.selected(), None);
    }

    #[test]
    fn status_tabs_and_unknown_installments_preserve_api_meaning() {
        let plan: InstallmentPlan = serde_json::from_value(
            serde_json::json!({"id":"p", "status":"completed", "total_installments":12}),
        )
        .unwrap();
        assert!(Tab::Completed.accepts(&plan));
        assert!(!Tab::Active.accepts(&plan));
        assert_eq!(progress(&plan), "?/12");
        assert_eq!(status_label(&plan), "Concluído");
    }
}
