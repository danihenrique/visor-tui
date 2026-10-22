use hqtui::prelude::*;
use hqtui::unicode::Align;
use std::collections::BTreeMap;

use crate::api::{ApiClient, ApiError};
use crate::models::{Investment, InvestmentSummary};
use crate::presentation::{Metric, TableSelection, Tone, amount, metric_strip, money_total};

fn money(value: Option<&str>, currency: Option<&str>) -> String {
    crate::presentation::format_currency(value, currency)
}

pub fn render(ui: &mut Container<'_>, api: &ApiClient) {
    render_scrolled(ui, api, &mut TableSelection::default());
}

pub fn render_scrolled(ui: &mut Container<'_>, api: &ApiClient, selection: &mut TableSelection) {
    match api.investments() {
        Ok((investments, summary)) => {
            render_net_worth_data(ui, &investments, summary.as_ref(), selection);
        }
        Err(error) => {
            selection.set_row_count(0);
            render_error(ui, &error);
        }
    }
}

fn render_error(ui: &mut Container<'_>, error: &ApiError) {
    let message = format!("Falha ao carregar patrimônio: {error}");
    let danger = ui.theme().danger;
    ui.panel(Panel::new().title("Erro"), move |panel| {
        panel.styled_text(&message, TextStyle::new().fg(danger));
        panel.label("Verifique sua conexão e credenciais.");
    });
}

fn render_net_worth_data(
    ui: &mut Container<'_>,
    investments: &[Investment],
    summary: Option<&InvestmentSummary>,
    selection: &mut TableSelection,
) {
    let total = portfolio_total(investments, summary);
    let count = summary
        .and_then(|s| s.count)
        .map_or_else(|| investments.len().to_string(), |count| count.to_string());
    let allocation = allocation(investments);
    let allocation_pages = allocation
        .as_ref()
        .map_or(1, |items| items.len().div_ceil(4).max(1));
    let section_count = allocation_pages + investments.len();
    selection.set_row_count(section_count);
    let selected = selection.selected().unwrap_or_default();
    let selected_investment = selected
        .saturating_sub(allocation_pages)
        .min(investments.len().saturating_sub(1));
    let investments = investments.to_vec();
    let full = ui.height() >= 20;
    let dense = ui.height() < 25;
    let allocation_height = if dense {
        allocation.as_ref().map_or(2, |items| items.len().min(4)) + 2
    } else {
        7
    };
    let tiles = ui.height() >= 14;
    let metrics = vec![
        Metric::new("Investimentos", total.clone(), Tone::Accent),
        Metric::new("Posições", count, Tone::Neutral),
    ];
    ui.column(
        Column::new().gap(usize::from(tiles && !dense)),
        move |col| {
            if tiles {
                metric_strip(col, metrics);
            } else {
                col.styled_text(&total, TextStyle::new().fg(col.theme().accent).bold());
            }
            if investments.is_empty() {
                col.panel(Panel::new().title("Carteira"), |panel| {
                    panel.label("Nenhum investimento encontrado neste espaço.");
                });
            } else if full {
                let allocation_index = selected.min(allocation_pages - 1);
                col.panel(
                    Panel::new()
                        .title("Distribuição da carteira")
                        .focused(selected < allocation_pages)
                        .size(Size::Cells(i64::try_from(allocation_height).unwrap_or(7))),
                    move |panel| {
                        allocation_bars(panel, allocation.as_deref(), allocation_index);
                    },
                );
                let detail = investments[selected_investment].clone();
                col.panel(
                    Panel::new()
                        .title("Posições")
                        .focused(selected >= allocation_pages),
                    move |panel| {
                        investment_table(panel, &investments, selected_investment);
                    },
                );
                col.panel(
                    Panel::new()
                        .title(detail.name.clone())
                        .size(Size::Cells(if dense { 5 } else { 6 })),
                    move |panel| investment_detail(panel, &detail),
                );
            } else if selected < allocation_pages {
                col.panel(
                    Panel::new().title("Distribuição da carteira"),
                    move |panel| {
                        allocation_bars(panel, allocation.as_deref(), selected);
                    },
                );
            } else {
                let detail = investments[selected_investment].clone();
                col.panel(Panel::new().title("Posições"), move |panel| {
                    investment_table(panel, &investments, selected_investment);
                });
                if tiles {
                    col.panel(
                        Panel::new().title(detail.name.clone()).size(Size::Cells(5)),
                        move |panel| investment_detail(panel, &detail),
                    );
                } else {
                    col.column(Column::new().size(Size::Cells(3)), move |detail_panel| {
                        investment_detail(detail_panel, &detail);
                    });
                }
            }
            col.label(&format!(
                "j/k Alocação e posições  ·  {}/{}",
                selected + 1,
                section_count
            ));
        },
    );
}

fn portfolio_total(investments: &[Investment], summary: Option<&InvestmentSummary>) -> String {
    let aggregate = money_total(
        investments
            .iter()
            .map(|i| (i.balance.as_deref(), i.currency_code.as_deref())),
    );
    if aggregate == "Várias moedas" {
        return aggregate;
    }
    if let Some(summary) =
        summary.filter(|summary| amount(summary.total_value.as_deref()).is_some())
    {
        let currency = summary.currency_code.as_deref().unwrap_or("BRL");
        if investments
            .iter()
            .all(|i| i.currency_code.as_deref().unwrap_or("BRL") == currency)
        {
            return money(summary.total_value.as_deref(), Some(currency));
        }
    }
    aggregate
}

fn investment_kind(raw: Option<&str>) -> &str {
    match raw {
        Some("fixed_income") => "Renda fixa",
        Some("stock") => "Ações",
        Some("fund") => "Fundos",
        Some("crypto") => "Cripto",
        Some(value) => value,
        None => "Não informado",
    }
}

fn allocation(investments: &[Investment]) -> Option<Vec<(String, f64)>> {
    let currency = investments
        .first()?
        .currency_code
        .as_deref()
        .unwrap_or("BRL");
    let mut groups = BTreeMap::<String, f64>::new();
    let mut total = 0.0;
    for investment in investments {
        if investment.currency_code.as_deref().unwrap_or("BRL") != currency {
            return None;
        }
        let balance = amount(investment.balance.as_deref())?;
        if balance < 0.0 {
            return None;
        }
        total += balance;
        *groups
            .entry(
                investment
                    .category
                    .as_deref()
                    .unwrap_or_else(|| investment_kind(investment.investment_type.as_deref()))
                    .to_string(),
            )
            .or_default() += balance;
    }
    if total <= 0.0 || !total.is_finite() {
        return None;
    }
    let mut groups: Vec<_> = groups
        .into_iter()
        .map(|(label, value)| (label, value / total))
        .collect();
    groups.sort_by(|a, b| b.1.total_cmp(&a.1));
    Some(groups)
}

fn allocation_bars(ui: &mut Container<'_>, allocation: Option<&[(String, f64)]>, page: usize) {
    let Some(allocation) = allocation else {
        ui.label("Distribuição indisponível.");
        ui.label("Requer saldos conhecidos na mesma moeda.");
        return;
    };
    for (label, ratio) in allocation.iter().skip(page * 4).take(4) {
        ui.meter(MeterOptions {
            value: *ratio,
            label: Some(label.clone()),
            label_width: Some(18),
            text: Some(format!("{:.1}%", ratio * 100.0)),
            value_width: Some(7),
            color: Some(ui.theme().accent),
            heat: Some(false),
            ..Default::default()
        });
    }
}

fn investment_table(ui: &mut Container<'_>, investments: &[Investment], selected: usize) {
    let wide = ui.width() >= 70;
    let theme = ui.theme().clone();
    let mut columns = vec![
        TableColumn::new("Investimento").width(Size::Fill),
        TableColumn::new("Instituição").width(Size::Fill),
    ];
    if wide {
        columns.push(TableColumn::new("Tipo").width(Size::Cells(13)));
    }
    columns.push(
        TableColumn::new("Saldo")
            .width(Size::Cells(20))
            .align(Align::Right),
    );
    let rows = investments
        .iter()
        .map(|investment| {
            let mut cells = vec![
                investment.name.clone(),
                investment
                    .provider_name
                    .clone()
                    .unwrap_or_else(|| "—".into()),
            ];
            if wide {
                cells.push(investment_kind(investment.investment_type.as_deref()).into());
            }
            cells.push(money(
                investment.balance.as_deref(),
                investment.currency_code.as_deref(),
            ));
            let mut row = TableRow::new(cells);
            row.cell_colors = vec![None; row.cells.len()];
            let last = row.cells.len() - 1;
            row.cell_colors[last] = Some(
                if amount(investment.balance.as_deref()).is_some_and(|a| a < 0.0) {
                    theme.danger
                } else {
                    theme.foreground
                },
            );
            row
        })
        .collect();
    ui.table(
        TableOptions {
            rows,
            columns,
            selected: Some(selected),
            follow_selection: true,
            zebra: true,
            scrollbar: true,
            ..Default::default()
        },
        "net_worth:positions",
    );
}

fn investment_detail(ui: &mut Container<'_>, investment: &Investment) {
    let source = match investment.source.as_deref() {
        Some("connected") => "● conectado",
        Some("manual") => "○ manual",
        Some(source) => source,
        None => "Origem não informada",
    };
    let contribution = if investment.contributes_to_net_worth {
        "Incluído no patrimônio"
    } else {
        "Fora do patrimônio"
    };
    ui.label(&format!(
        "{} · {} · {}",
        investment
            .provider_name
            .as_deref()
            .unwrap_or("Instituição não informada"),
        investment_kind(investment.investment_type.as_deref()),
        source
    ));
    ui.label(&format!(
        "{} · {}",
        investment.category.as_deref().unwrap_or("Sem categoria"),
        investment.subtype.as_deref().unwrap_or("Sem subtipo")
    ));
    ui.label(&format!(
        "{contribution} · {}",
        investment
            .updated_at
            .as_deref()
            .unwrap_or("Atualização não informada")
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use hqtui::testing::render_to_text;

    fn investment(name: &str, balance: Option<&str>, currency: &str) -> Investment {
        serde_json::from_value(serde_json::json!({"id":name,"name":name,"balance":balance,"currency_code":currency,"type":"fixed_income","provider_name":"Corretora","category":"Renda fixa"})).unwrap()
    }

    #[test]
    fn mixed_currencies_and_unknown_balances_have_no_allocation() {
        let investments = vec![
            investment("Brasil", Some("100"), "BRL"),
            investment("Exterior", Some("100"), "USD"),
        ];
        assert_eq!(portfolio_total(&investments, None), "Várias moedas");
        assert!(allocation(&investments).is_none());
        assert!(allocation(&[investment("Sem saldo", None, "BRL")]).is_none());
        assert!(allocation(&[investment("Negativo", Some("-100"), "BRL")]).is_none());
    }

    #[test]
    fn allocation_uses_known_balances_instead_of_position_counts() {
        let mut stocks = investment("Ações", Some("300"), "BRL");
        stocks.category = Some("Ações".into());
        let groups = allocation(&[investment("Reserva", Some("100"), "BRL"), stocks]).unwrap();
        assert_eq!(
            groups,
            vec![("Ações".into(), 0.75), ("Renda fixa".into(), 0.25)]
        );
    }

    #[test]
    fn last_investment_and_its_detail_remain_reachable() {
        let investments: Vec<_> = (0..8)
            .map(|index| investment(&format!("Fundo {index}"), Some("4444"), "BRL"))
            .collect();
        for (width, height) in [(56, 10), (60, 18), (80, 24)] {
            let mut selection = TableSelection::default();
            render_to_text(width, height, "dark", |ui| {
                render_net_worth_data(ui, &investments, None, &mut selection);
            });
            selection.move_by(isize::MAX);
            let text = render_to_text(width, height, "dark", |ui| {
                render_net_worth_data(ui, &investments, None, &mut selection);
            });
            for expected in ["Fundo 7", "4.444,00 BRL", "Corretora", "Renda fixa"] {
                assert!(
                    text.contains(expected),
                    "{width}x{height}: missing {expected}\n{text}"
                );
            }
        }
    }
}
