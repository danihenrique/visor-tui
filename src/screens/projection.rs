use hqtui::prelude::*;
use hqtui::color::Color;
use hqtui::widgets::{KeyValueOptions, KeyValueRow};
use crate::api::ApiClient;

pub fn render(ui: &mut Container<'_>, api: &ApiClient) {
    let overview_result = api.overview();

    match overview_result {
        Ok(overview) => {
            let currency_code = overview.currency_code.clone().unwrap_or_else(|| "BRL".to_string());

            if let Some(projection) = overview.projection {
                // Pre-compute all display strings and colors before entering closures
                let current_balance_str = format_money(projection.current_balance.as_deref(), &currency_code);
                let current_balance_color = get_color_for_amount(projection.current_balance.as_deref());

                let expected_income_str = format_money(projection.expected_income.as_deref(), &currency_code);
                let income_color = Color::rgb(0, 200, 0);

                let (bills_str, installments_str, subscriptions_str, scheduled_str) =
                    if let Some(ref outflows) = projection.committed_outflows {
                        (
                            format_money(outflows.bills.as_deref(), &currency_code),
                            format_money(outflows.installments.as_deref(), &currency_code),
                            format_money(outflows.subscriptions.as_deref(), &currency_code),
                            format_money(outflows.scheduled.as_deref(), &currency_code),
                        )
                    } else {
                        (
                            String::from("N/A"),
                            String::from("N/A"),
                            String::from("N/A"),
                            String::from("N/A"),
                        )
                    };

                let estimated_day_to_day_str = format_money(projection.estimated_day_to_day.as_deref(), &currency_code);
                let estimated_day_to_day_color = get_color_for_amount(projection.estimated_day_to_day.as_deref());

                let projected_balance_str = format_money(projection.projected_balance.as_deref(), &currency_code);
                let projected_balance_color = get_color_for_amount(projection.projected_balance.as_deref());

                let outflow_color = Color::rgb(200, 0, 0);

                // Render the projection UI with move closures
                ui.column(Column::new().gap(1), move |col| {
                    col.panel(Panel::new().title("Projeção"), move |p| {
                        p.key_values(KeyValueOptions {
                            rows: vec![
                                KeyValueRow::new("Saldo Atual", current_balance_str).color(current_balance_color),
                                KeyValueRow::new("Receita Esperada", expected_income_str).color(income_color),
                            ],
                            ..Default::default()
                        });

                        p.divider(DividerOptions::default());

                        p.label("Saídas Comprometidas:");
                        p.key_values(KeyValueOptions {
                            rows: vec![
                                KeyValueRow::new("Contas", bills_str).color(outflow_color),
                                KeyValueRow::new("Parcelas", installments_str).color(outflow_color),
                                KeyValueRow::new("Assinaturas", subscriptions_str).color(outflow_color),
                                KeyValueRow::new("Agendadas", scheduled_str).color(outflow_color),
                            ],
                            ..Default::default()
                        });

                        p.divider(DividerOptions::default());

                        p.key_values(KeyValueOptions {
                            rows: vec![
                                KeyValueRow::new("Dia a Dia Estimado", estimated_day_to_day_str).color(estimated_day_to_day_color),
                                KeyValueRow::new("Saldo Projetado", projected_balance_str).color(projected_balance_color),
                            ],
                            ..Default::default()
                        });
                    });
                });
            } else {
                // No projection data available
                let no_data_msg = "Sem dados de projeção";
                ui.column(Column::new().gap(1), move |col| {
                    col.panel(Panel::new().title("Projeção"), move |p| {
                        p.text(no_data_msg);
                    });
                });
            }
        }
        Err(error) => {
            // Handle API errors gracefully
            let error_msg = format!("Erro ao carregar dados: {error}");
            let error_color = Color::rgb(200, 0, 0);
            ui.column(Column::new().gap(1), move |col| {
                col.panel(Panel::new().title("Erro"), move |p| {
                    p.styled_text(&error_msg, TextStyle::new().fg(error_color));
                });
            });
        }
    }
}

/// Format money values with BRL style (dots for thousands, comma for decimal)
fn format_money(value: Option<&str>, currency_code: &str) -> String {
    match value {
        Some(v) => {
            if let Ok(amount) = v.parse::<f64>() {
                let sign = if amount < 0.0 { "-" } else { "" };
                let abs_amount = amount.abs();
                let amount_str = format!("{abs_amount:.2}");
                let parts: Vec<&str> = amount_str.split('.').collect();
                let whole = parts[0];
                let decimal = parts[1];

                // Add thousand separators (dots)
                let mut formatted_whole = String::new();
                let chars: Vec<char> = whole.chars().collect();
                let len = chars.len();
                for (i, c) in chars.iter().enumerate() {
                    if i > 0 && (len - i).is_multiple_of(3) {
                        formatted_whole.push('.');
                    }
                    formatted_whole.push(*c);
                }

                format!("{currency_code} {sign}.{formatted_whole},{decimal}")
            } else {
                // Not a valid number, return as-is with currency code
                format!("{currency_code} {v}")
            }
        }
        None => String::from("N/A"),
    }
}

/// Determine color based on amount value (green for positive, red for negative)
fn get_color_for_amount(value: Option<&str>) -> Color {
    match value {
        Some(v) => {
            if let Ok(amount) = v.parse::<f64>() {
                if amount >= 0.0 {
                    Color::rgb(0, 200, 0)
                } else {
                    Color::rgb(200, 0, 0)
                }
            } else {
                Color::rgb(200, 200, 200) // default gray for invalid values
            }
        }
        None => Color::rgb(200, 200, 200),
    }
}