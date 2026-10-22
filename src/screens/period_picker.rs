use chrono::NaiveDate;
use hqtui::{input::InputEvent, prelude::*};

use crate::period::{DateRange, Period};

const PRESETS: [&str; 5] = ["Mês", "Semana", "Trimestre", "Ano", "Personalizado"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickerOutcome {
    Editing,
    Cancelled,
    Accepted(Period),
}

#[derive(Debug, Clone)]
enum PickerMode {
    Presets(usize),
    Custom(usize),
}

#[derive(Debug, Clone)]
pub struct PeriodPicker {
    anchor: NaiveDate,
    mode: PickerMode,
    dates: [String; 2],
    error: Option<String>,
}

impl PeriodPicker {
    #[must_use]
    pub fn new(period: &Period) -> Self {
        let range = period.range();
        let anchor = range.start();
        let selected = [
            Period::month(anchor),
            Period::week(anchor),
            Period::quarter(anchor),
            Period::year(anchor),
        ]
        .iter()
        .position(|preset| preset == period)
        .unwrap_or(4);
        Self {
            anchor,
            mode: PickerMode::Presets(selected),
            dates: [
                range.start().format("%d/%m/%Y").to_string(),
                range.end().format("%d/%m/%Y").to_string(),
            ],
            error: None,
        }
    }

    pub fn handle_input(&mut self, events: &[InputEvent]) -> PickerOutcome {
        for event in events {
            let key = match event {
                InputEvent::Key(key) => key,
                InputEvent::Paste(paste) => {
                    self.insert_text(&paste.text);
                    continue;
                }
                _ => continue,
            };
            if key.name == "escape" {
                return PickerOutcome::Cancelled;
            }
            match self.mode {
                PickerMode::Presets(selected) => match key.key.as_str() {
                    "up" | "k" => self.mode = PickerMode::Presets(selected.saturating_sub(1)),
                    "down" | "j" => {
                        self.mode = PickerMode::Presets((selected + 1).min(PRESETS.len() - 1));
                    }
                    "home" => self.mode = PickerMode::Presets(0),
                    "end" => self.mode = PickerMode::Presets(PRESETS.len() - 1),
                    "enter" => {
                        let period = match selected {
                            0 => Period::month(self.anchor),
                            1 => Period::week(self.anchor),
                            2 => Period::quarter(self.anchor),
                            3 => Period::year(self.anchor),
                            _ => {
                                self.mode = PickerMode::Custom(0);
                                continue;
                            }
                        };
                        return PickerOutcome::Accepted(period);
                    }
                    _ => {}
                },
                PickerMode::Custom(field) => match key.key.as_str() {
                    "tab" | "shift+tab" => self.mode = PickerMode::Custom(1 - field),
                    "backspace" => {
                        self.dates[field].pop();
                        self.error = None;
                    }
                    "ctrl+a" | "ctrl+u" => {
                        self.dates[field].clear();
                        self.error = None;
                    }
                    "enter" => match DateRange::parse(&self.dates[0], &self.dates[1]) {
                        Ok(range) => return PickerOutcome::Accepted(Period::custom(range)),
                        Err(error) => self.error = Some(error.to_string()),
                    },
                    _ if !key.ctrl && !key.alt => {
                        if let Some(text) = &key.char {
                            self.insert_text(text);
                        }
                    }
                    _ => {}
                },
            }
        }
        PickerOutcome::Editing
    }

    fn insert_text(&mut self, text: &str) {
        if let PickerMode::Custom(field) = self.mode {
            let remaining = 32_usize.saturating_sub(self.dates[field].chars().count());
            self.dates[field].extend(text.chars().filter(|ch| !ch.is_control()).take(remaining));
            self.error = None;
        }
    }

    pub fn render(&self, ui: &mut Container<'_>) {
        let message = match self.mode {
            PickerMode::Presets(selected) => {
                let mut lines = vec![
                    format!("Referência: {}", self.anchor.format("%d/%m/%Y")),
                    String::new(),
                ];
                lines.extend(PRESETS.iter().enumerate().map(|(index, label)| {
                    format!("{} {label}", if index == selected { "›" } else { " " })
                }));
                lines.extend([
                    String::new(),
                    "↑/↓ j/k Escolher  Enter Aplicar  Esc Cancelar".into(),
                ]);
                lines.join("\n")
            }
            PickerMode::Custom(field) => {
                let mut lines = vec![
                    "DD/MM/YYYY ou YYYY-MM-DD".into(),
                    String::new(),
                    format!(
                        "{} Início: {}{}",
                        if field == 0 { "›" } else { " " },
                        self.dates[0],
                        if field == 0 { "│" } else { "" }
                    ),
                    format!(
                        "{} Fim:    {}{}",
                        if field == 1 { "›" } else { " " },
                        self.dates[1],
                        if field == 1 { "│" } else { "" }
                    ),
                    String::new(),
                ];
                if let Some(error) = &self.error {
                    lines.push(error.clone());
                    lines.push(String::new());
                }
                lines.extend([
                    "Tab Campo  Ctrl+A Limpar".into(),
                    "Enter Aplicar  Esc Cancelar".into(),
                ]);
                lines.join("\n")
            }
        };
        ui.modal(ModalOptions {
            title: Some(
                match self.mode {
                    PickerMode::Presets(_) => "Selecionar período",
                    PickerMode::Custom(_) => "Período personalizado",
                }
                .into(),
            ),
            message: Some(message),
            width: Some(58),
            align: Some(Align::Left),
            ..Default::default()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::period::parse_date;
    use hqtui::{input::InputParser, testing::render_to_text};

    fn month() -> Period {
        Period::month(parse_date("2026-10-22").unwrap())
    }

    fn input(picker: &mut PeriodPicker, text: &str) -> PickerOutcome {
        let mut parser = InputParser::new();
        let mut events = parser.parse(text);
        events.extend(parser.flush());
        picker.handle_input(&events)
    }

    #[test]
    fn preset_navigation_clamps_and_applies_calendar_range() {
        let mut picker = PeriodPicker::new(&month());
        assert_eq!(
            input(&mut picker, "kkkkk\r"),
            PickerOutcome::Accepted(month())
        );
        let mut picker = PeriodPicker::new(&month());
        assert_eq!(
            input(&mut picker, "j\r"),
            PickerOutcome::Accepted(Period::week(parse_date("2026-10-01").unwrap()))
        );
        let mut picker = PeriodPicker::new(&month());
        assert_eq!(
            input(&mut picker, "jj\r"),
            PickerOutcome::Accepted(Period::quarter(parse_date("2026-10-01").unwrap()))
        );
        let mut picker = PeriodPicker::new(&month());
        assert_eq!(
            input(&mut picker, "jjj\r"),
            PickerOutcome::Accepted(Period::year(parse_date("2026-10-01").unwrap()))
        );
    }

    #[test]
    fn custom_accepts_paste_and_mixed_date_formats() {
        let mut picker = PeriodPicker::new(&month());
        assert_eq!(input(&mut picker, "jjjjj\r\x01"), PickerOutcome::Editing);
        assert_eq!(
            input(&mut picker, "\x1b[200~29/02/2024\x1b[201~\t\x01"),
            PickerOutcome::Editing
        );
        assert_eq!(
            input(&mut picker, "2024-03-01\r"),
            PickerOutcome::Accepted(Period::custom(
                DateRange::parse("2024-02-29", "2024-03-01").unwrap()
            ))
        );
    }

    #[test]
    fn custom_validation_keeps_editor_open_and_escape_cancels() {
        let mut picker = PeriodPicker::new(&month());
        input(&mut picker, "jjjj\r\x01invalid\r");
        assert!(picker.error.as_deref().unwrap().contains("Data inválida"));
        assert_eq!(input(&mut picker, "\x01"), PickerOutcome::Editing);
        assert_eq!(
            input(&mut picker, "31/10/2026\t\x01"),
            PickerOutcome::Editing
        );
        assert_eq!(input(&mut picker, "2026-10-01\r"), PickerOutcome::Editing);
        assert!(picker.error.as_deref().unwrap().contains("data inicial"));
        assert_eq!(input(&mut picker, "\x1b"), PickerOutcome::Cancelled);
    }

    #[test]
    fn picker_labels_and_validation_fit_supported_terminal_sizes() {
        let mut picker = PeriodPicker::new(&month());
        for (width, height) in [(120, 40), (80, 24)] {
            let text = render_to_text(width, height, "dark", |ui| picker.render(ui));
            for label in PRESETS {
                assert!(text.contains(label), "{width}x{height} {label}\n{text}");
            }
            assert!(text.contains("Esc Cancelar"));
        }
        input(&mut picker, "jjjj\r\x01bad\r");
        let text = render_to_text(80, 24, "dark", |ui| picker.render(ui));
        for label in [
            "Início:",
            "Fim:",
            "Data inválida",
            "Enter Aplicar",
            "Esc Cancelar",
        ] {
            assert!(text.contains(label), "{label}\n{text}");
        }
    }
}
