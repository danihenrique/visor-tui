use hqtui::prelude::*;
use hqtui::color::Color;
use hqtui::widgets::{
    TableOptions, TableColumn, TableRow, CheckboxOptions, CheckboxVariant,
};
use hqtui::unicode::Align;

use crate::api::{ApiClient, ApiError};
use crate::models::Category;

const COLOR_NEGATIVE: Color = Color::rgb(239, 68, 68);
const COLOR_NEUTRAL: Color = Color::rgb(148, 163, 184);

struct CategoryRowData {
    id: String,
    name: String,
    tipo: String,
    chave: String,
    oculta: String,
}

fn build_row_data(categories: &[Category]) -> Vec<CategoryRowData> {
    categories
        .iter()
        .map(|cat| {
            let name = if cat.parent_id.is_some() {
                format!("  └ {}", cat.name)
            } else {
                cat.name.clone()
            };

            let tipo = cat
                .category_type
                .as_deref()
                .or(cat.kind.as_deref())
                .unwrap_or("—")
                .to_string();

            let chave = cat.key.as_deref().unwrap_or("—").to_string();

            let oculta = if cat.hidden {
                "Sim".to_string()
            } else {
                "Não".to_string()
            };

            CategoryRowData {
                id: cat.id.clone(),
                name,
                tipo,
                chave,
                oculta,
            }
        })
        .collect()
}

fn table_columns() -> Vec<TableColumn> {
    vec![
        TableColumn::new("Nome").width(Size::Fill),
        TableColumn::new("Tipo").width(Size::Cells(16)).align(Align::Center),
        TableColumn::new("Chave").width(Size::Cells(20)).align(Align::Left),
        TableColumn::new("Oculta").width(Size::Cells(8)).align(Align::Center),
    ]
}

fn draw_error(ui: &mut Container<'_>, err: &ApiError) {
    let err_msg = format!("Falha ao carregar categorias: {err}");
    ui.panel(Panel::new().title("Erro"), move |p| {
        p.styled_text(&err_msg, TextStyle::new().fg(COLOR_NEGATIVE));
        p.label("Verifique sua conexão e credenciais.");
    });
}

pub fn render(ui: &mut Container<'_>, api: &ApiClient) {
    // Default: do not include hidden categories
    let include_hidden = false;

    let categories = match api.categories(include_hidden) {
        Ok(data) => data,
        Err(ref e) => {
            draw_error(ui, e);
            return;
        }
    };

    // Pre-compute all row data before entering closures
    let row_data = build_row_data(&categories);

    let table_rows: Vec<TableRow> = row_data
        .iter()
        .map(|r| {
            TableRow::new([
                r.name.clone(),
                r.tipo.clone(),
                r.chave.clone(),
                r.oculta.clone(),
            ])
        })
        .collect();

    let columns = table_columns();
    let has_rows = !table_rows.is_empty();
    let toggle_checked = include_hidden;
    let category_count = categories.len();
    let panel_title = format!("Categorias ({category_count})");

    ui.column(Column::new().gap(1), move |col| {
        // Toggle row for showing hidden categories
        col.row(Row::new().gap(1), move |r| {
            r.checkbox(
                CheckboxOptions {
                    variant: CheckboxVariant::Toggle,
                    ..CheckboxOptions::new("Mostrar ocultas", toggle_checked)
                },
                "toggle_hidden_categories",
            );
        });

        // Categories table panel
        col.panel(Panel::new().title(panel_title.clone()), move |p| {
            if has_rows {
                p.table(
                    TableOptions {
                        columns: columns.clone(),
                        rows: table_rows.clone(),
                        header: true,
                        zebra: true,
                        ..Default::default()
                    },
                    "",
                );
            } else {
                p.styled_text(
                    "Nenhuma categoria encontrada.",
                    TextStyle::new().fg(COLOR_NEUTRAL),
                );
            }
        });
    });
}