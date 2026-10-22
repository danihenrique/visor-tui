use hqtui::color::Color;
use hqtui::prelude::*;
use hqtui::unicode::Align;
use hqtui::widgets::{ButtonOptions, ButtonVariant, TableColumn, TableOptions, TableRow};

use crate::api::{ApiClient, ApiError};
use crate::models::InstallmentPlan;

const COLOR_NEGATIVE: Color = Color::rgb(239, 68, 68);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    All,
    Active,
    Completed,
}

fn format_money(amount: Option<&str>, currency: Option<&str>) -> String {
    crate::presentation::format_currency(amount, currency)
}

fn format_date(date: Option<&str>) -> String {
    match date {
        Some(d) if !d.is_empty() => d.to_string(),
        _ => "—".to_string(),
    }
}

fn format_progress(paid: Option<u32>, total: Option<u32>) -> String {
    match (paid, total) {
        (Some(p), Some(t)) => format!("{p}/{t}"),
        (Some(p), None) => format!("{p}/?"),
        (None, Some(t)) => format!("?/{t}"),
        (None, None) => "—".to_string(),
    }
}

fn status_label(status: Option<&str>) -> String {
    match status {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => "—".to_string(),
    }
}

#[derive(Clone)]
struct PayableRowData {
    description: String,
    progress: String,
    installment_amount: String,
    total: String,
    remaining: String,
    next_due: String,
    status: String,
}

fn build_row_data(items: &[InstallmentPlan]) -> Vec<PayableRowData> {
    items
        .iter()
        .map(|item| {
            let currency = item.currency_code.as_deref();
            let description = item
                .description
                .clone()
                .unwrap_or_else(|| "Sem descrição".to_string());
            let progress = format_progress(item.paid_installments, item.total_installments);
            let installment_amount = format_money(item.installment_amount.as_deref(), currency);
            let total = format_money(item.total_amount.as_deref(), currency);
            let remaining = format_money(item.remaining_amount.as_deref(), currency);
            let next_due = format_date(item.next_due_date.as_deref());
            let status = status_label(item.status.as_deref());
            PayableRowData {
                description,
                progress,
                installment_amount,
                total,
                remaining,
                next_due,
                status,
            }
        })
        .collect()
}

fn table_columns() -> Vec<TableColumn> {
    vec![
        TableColumn::new("Descrição").width(Size::Fill),
        TableColumn::new("Parcelas")
            .width(Size::Cells(10))
            .align(Align::Center),
        TableColumn::new("Valor Parcela")
            .width(Size::Cells(18))
            .align(Align::Right),
        TableColumn::new("Total")
            .width(Size::Cells(18))
            .align(Align::Right),
        TableColumn::new("Restante")
            .width(Size::Cells(18))
            .align(Align::Right),
        TableColumn::new("Próximo Vencimento")
            .width(Size::Cells(18))
            .align(Align::Right),
        TableColumn::new("Status")
            .width(Size::Cells(12))
            .align(Align::Center),
    ]
}

fn draw_error(ui: &mut Container<'_>, err: &ApiError) {
    let err_msg = format!("Falha ao carregar contas a pagar: {err}");
    ui.panel(Panel::new().title("Erro"), move |p| {
        p.styled_text(&err_msg, TextStyle::new().fg(COLOR_NEGATIVE));
        p.label("Verifique sua conexão e credenciais.");
    });
}

#[derive(Default)]
pub struct PayablesScreen {
    tab: usize,
    selected: Option<usize>,
}

impl PayablesScreen {
    pub fn handle_input(&mut self, app: &App, events: &[hqtui::input::InputEvent]) {
        for event in events {
            if let hqtui::input::InputEvent::Key(key) = event {
                match key.name.as_str() {
                    "left" | "h" => {
                        self.tab = (self.tab + 2) % 3;
                        self.selected = None;
                    }
                    "right" | "l" => {
                        self.tab = (self.tab + 1) % 3;
                        self.selected = None;
                    }
                    "down" | "j" => {
                        self.selected =
                            Some(self.selected.map_or(0, |index| index.saturating_add(1)));
                    }
                    "up" | "k" => {
                        self.selected = self.selected.map(|index| index.saturating_sub(1));
                    }
                    _ => {}
                }
            }
        }
        for (index, id) in ["tab_all", "tab_active", "tab_completed"]
            .iter()
            .enumerate()
        {
            if app.pressed(id) {
                self.tab = index;
                self.selected = None;
            }
        }
    }
}

pub fn render(ui: &mut Container<'_>, api: &ApiClient) {
    render_with_state(ui, api, &PayablesScreen::default());
}

pub fn render_with_state(ui: &mut Container<'_>, api: &ApiClient, screen: &PayablesScreen) {
    let plans_result = api.installment_plans(None);

    let plans = match plans_result {
        Ok(data) => data,
        Err(ref e) => {
            draw_error(ui, e);
            return;
        }
    };

    // Default tab
    let active_tab = match screen.tab {
        1 => Tab::Active,
        2 => Tab::Completed,
        _ => Tab::All,
    };

    // Pre-compute all row data before entering closures
    let all_rows = build_row_data(&plans);

    // Filter rows by tab
    let active_rows: Vec<PayableRowData> = all_rows
        .iter()
        .filter(|r| {
            r.status == "active"
                || r.status == "Active"
                || r.status == "ativo"
                || r.status == "Ativo"
        })
        .cloned()
        .collect();

    let completed_rows: Vec<PayableRowData> = all_rows
        .iter()
        .filter(|r| {
            r.status == "completed"
                || r.status == "Completed"
                || r.status == "concluído"
                || r.status == "Concluído"
                || r.status == "concluido"
                || r.status == "Concluido"
        })
        .cloned()
        .collect();

    let all_count = all_rows.len();
    let active_count = active_rows.len();
    let completed_count = completed_rows.len();

    let columns = table_columns();

    // Build table rows for each tab
    let all_table_rows: Vec<TableRow> = all_rows
        .iter()
        .map(|r| {
            TableRow::new([
                r.description.clone(),
                r.progress.clone(),
                r.installment_amount.clone(),
                r.total.clone(),
                r.remaining.clone(),
                r.next_due.clone(),
                r.status.clone(),
            ])
        })
        .collect();

    let active_table_rows: Vec<TableRow> = active_rows
        .iter()
        .map(|r| {
            TableRow::new([
                r.description.clone(),
                r.progress.clone(),
                r.installment_amount.clone(),
                r.total.clone(),
                r.remaining.clone(),
                r.next_due.clone(),
                r.status.clone(),
            ])
        })
        .collect();

    let completed_table_rows: Vec<TableRow> = completed_rows
        .iter()
        .map(|r| {
            TableRow::new([
                r.description.clone(),
                r.progress.clone(),
                r.installment_amount.clone(),
                r.total.clone(),
                r.remaining.clone(),
                r.next_due.clone(),
                r.status.clone(),
            ])
        })
        .collect();

    // Pre-compute tab labels with counts
    let all_label = format!("Todos ({all_count})");
    let active_label = format!("Ativos ({active_count})");
    let completed_label = format!("Concluídos ({completed_count})");

    // Determine button variants based on active tab
    let all_variant = if active_tab == Tab::All {
        ButtonVariant::Primary
    } else {
        ButtonVariant::Ghost
    };
    let active_variant = if active_tab == Tab::Active {
        ButtonVariant::Primary
    } else {
        ButtonVariant::Ghost
    };
    let completed_variant = if active_tab == Tab::Completed {
        ButtonVariant::Primary
    } else {
        ButtonVariant::Ghost
    };

    // Select the correct table rows and empty message based on active tab
    let (current_table_rows, empty_msg) = match active_tab {
        Tab::All => (all_table_rows, "Nenhum parcelamento encontrado."),
        Tab::Active => (active_table_rows, "Nenhum parcelamento ativo encontrado."),
        Tab::Completed => (
            completed_table_rows,
            "Nenhum parcelamento concluído encontrado.",
        ),
    };
    let has_rows = !current_table_rows.is_empty();
    let selected = screen
        .selected
        .map(|index| index.min(current_table_rows.len().saturating_sub(1)));

    ui.column(Column::new().gap(1), move |col| {
        // Tab buttons row
        col.row(Row::new().size(Size::Cells(1)).gap(1), move |r| {
            r.button(
                ButtonOptions::new(all_label.clone()).variant(all_variant),
                "tab_all",
            );
            r.button(
                ButtonOptions::new(active_label.clone()).variant(active_variant),
                "tab_active",
            );
            r.button(
                ButtonOptions::new(completed_label.clone()).variant(completed_variant),
                "tab_completed",
            );
        });

        // Data panel
        col.panel(Panel::new().title("Contas a Pagar"), move |p| {
            if has_rows {
                p.table(
                    TableOptions {
                        columns: columns.clone(),
                        rows: current_table_rows.clone(),
                        header: true,
                        zebra: true,
                        selected,
                        follow_selection: true,
                        ..Default::default()
                    },
                    "",
                );
            } else {
                p.label(empty_msg);
            }
        });
    });
}
