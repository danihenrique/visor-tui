use hqtui::prelude::*;
use hqtui::widgets::{KeyValueOptions, KeyValueRow};
use crate::api::ApiClient;

/// Format a numeric string as BRL currency (dots for thousands, comma for decimal)
fn format_brl(amount_str: &str) -> String {
    // Try to parse the amount
    let amount_str = amount_str.trim();

    // Handle if it's already formatted or has various separators
    let clean_str = amount_str
        .replace('.', "")
        .replace(',', "")
        .replace(' ', "");

    if let Ok(amount) = clean_str.parse::<f64>() {
        // Format with 2 decimal places
        let formatted = format!("{:.2}", amount);
        let parts: Vec<&str> = formatted.split('.').collect();

        if parts.len() == 2 {
            let integer_part = parts[0];
            let decimal_part = parts[1];

            // Add dots for thousands
            let mut result = String::new();
            let chars: Vec<char> = integer_part.chars().collect();
            for (i, c) in chars.iter().enumerate() {
                if i > 0 && (chars.len() - i) % 3 == 0 {
                    result.push('.');
                }
                result.push(*c);
            }

            // Add decimal part with comma
            format!("{},{}", result, decimal_part)
        } else {
            format!("{},00", parts[0])
        }
    } else {
        // If parsing fails, return original
        amount_str.to_string()
    }
}

/// Get status badge text with color indicator
fn status_badge(status: Option<&str>) -> String {
    match status {
        Some("active") => "[ATIVO]".to_string(),
        Some("archived") => "[ARQUIVADO]".to_string(),
        Some("completed") => "[CONCLUÍDO]".to_string(),
        Some(s) => format!("[{}]", s.to_uppercase()),
        None => "[SEM STATUS]".to_string(),
    }
}

pub fn render(ui: &mut Container<'_>, api: &ApiClient) {
    // Fetch goals from API
    let goals_result = api.goals(None);

    match goals_result {
        Ok(goals) => {
            if goals.is_empty() {
                // Show empty state
                ui.panel(Panel::new().title("Metas"), move |p| {
                    p.text("Nenhuma meta encontrada.");
                });
                return;
            }

            // Pre-compute all display data before entering closures
            let goal_displays: Vec<_> = goals
                .into_iter()
                .map(|goal| {
                    let id = goal.id.clone();
                    let name = goal.name.clone();
                    let kind = goal.kind.clone().unwrap_or_else(|| "Não especificado".to_string());
                    let status_text = status_badge(goal.status.as_deref());

                    // Format currency values
                    let target_display = goal
                        .target_amount
                        .as_ref()
                        .map(|s| format!("R$ {}", format_brl(s)))
                        .unwrap_or_else(|| "Não definido".to_string());

                    let current_display = goal
                        .current_amount
                        .as_ref()
                        .map(|s| format!("R$ {}", format_brl(s)))
                        .unwrap_or_else(|| "R$ 0,00".to_string());

                    let currency = goal
                        .currency_code
                        .clone()
                        .unwrap_or_else(|| "BRL".to_string());

                    // Progress percentage
                    let progress_pct = goal.progress_percent.unwrap_or(0.0);
                    let progress_ratio = (progress_pct / 100.0).max(0.0).min(1.0);
                    let pct_text = format!("{:.1}%", progress_pct);

                    // Dates
                    let target_date = goal
                        .target_date
                        .clone()
                        .unwrap_or_else(|| "Não definido".to_string());

                    let created_at = goal
                        .created_at
                        .clone()
                        .unwrap_or_else(|| "Desconhecido".to_string());

                    // Panel title with status
                    let panel_title = format!("{} - {}", name, status_text);

                    (
                        id,
                        panel_title,
                        kind,
                        target_display,
                        current_display,
                        currency,
                        progress_ratio,
                        pct_text.clone(),
                        target_date,
                        created_at,
                    )
                })
                .collect();

            // Render the goals list
            ui.column(Column::new().gap(1), move |col| {
                for (
                    _id,
                    panel_title,
                    kind,
                    target_display,
                    current_display,
                    currency,
                    progress_ratio,
                    pct_text,
                    target_date,
                    created_at,
                ) in goal_displays.clone()
                {
                    col.panel(Panel::new().title(panel_title), move |p| {
                        // Key-value details
                        p.key_values(KeyValueOptions {
                            rows: vec![
                                KeyValueRow::new("Tipo", &kind),
                                KeyValueRow::new("Valor Alvo", &target_display),
                                KeyValueRow::new("Valor Atual", &current_display),
                                KeyValueRow::new("Moeda", &currency),
                                KeyValueRow::new("Data Alvo", &target_date),
                                KeyValueRow::new("Criado em", &created_at),
                            ],
                            ..Default::default()
                        });

                        // Progress meter
                        p.divider(DividerOptions::default());
                        p.draw(move |s| {
                            hqtui::widgets::draw_meter(
                                s,
                                &hqtui::widgets::MeterOptions {
                                    value: progress_ratio,
                                    label: Some("Progresso".into()),
                                    text: Some(pct_text.clone()),
                                    ..Default::default()
                                },
                            );
                        });
                    });
                }
            });
        }
        Err(error) => {
            // Show error panel
            let error_msg = format!("Erro ao carregar metas: {}", error);
            ui.panel(Panel::new().title("Erro"), move |p| {
                p.text(&error_msg);
            });
        }
    }
}