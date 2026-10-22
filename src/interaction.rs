use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

use hqtui::{
    input::{InputEvent, KeyEvent, MouseAction, MouseButton},
    prelude::*,
    unicode::{string_width, wrap},
    widgets::{draw_button, draw_modal, draw_table, resolve_offset},
};

#[derive(Debug)]
pub struct HitMap<A>(Rc<RefCell<Vec<(Rect, A)>>>);

impl<A> Default for HitMap<A> {
    fn default() -> Self {
        Self(Rc::new(RefCell::new(Vec::new())))
    }
}
impl<A> Clone for HitMap<A> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}
impl<A: Clone> HitMap<A> {
    pub fn clear(&self) {
        self.0.borrow_mut().clear();
    }
    pub fn add(&self, surface: &Surface, action: A) {
        let rect = surface.hit_rect().intersect(surface.clip);
        if !rect.is_empty() {
            self.0.borrow_mut().push((rect, action));
        }
    }
    pub fn underlay(&self, surface: &Surface, action: A) {
        let rect = surface.hit_rect().intersect(surface.clip);
        if !rect.is_empty() {
            self.0.borrow_mut().insert(0, (rect, action));
        }
    }
    #[must_use]
    pub fn at(&self, x: usize, y: usize) -> Option<A> {
        let (Ok(x), Ok(y)) = (isize::try_from(x), isize::try_from(y)) else {
            return None;
        };
        self.0
            .borrow()
            .iter()
            .rev()
            .find(|(rect, _)| rect.contains(x, y))
            .map(|(_, action)| action.clone())
    }
    #[must_use]
    pub fn click(&self, event: &InputEvent) -> Option<A> {
        match event {
            InputEvent::Mouse(mouse)
                if mouse.action == MouseAction::Press && mouse.button == MouseButton::Left =>
            {
                self.at(mouse.x, mouse.y)
            }
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Control {
    Button(String),
    Row(usize),
    Field(usize),
    Scroll,
}
pub type Interactions = HitMap<Control>;

impl Interactions {
    #[must_use]
    pub fn pressed(&self, events: &[InputEvent], id: &str) -> bool {
        events
            .iter()
            .any(|event| self.click(event) == Some(Control::Button(id.into())))
    }
    pub fn list_events(
        &self,
        events: &[InputEvent],
        selected: &mut Option<usize>,
    ) -> Vec<InputEvent> {
        events
            .iter()
            .filter_map(|event| {
                if let Some(Control::Row(index)) = self.click(event) {
                    *selected = Some(index);
                    return None;
                }
                if let InputEvent::Mouse(mouse) = event
                    && mouse.action == MouseAction::Scroll
                    && matches!(
                        self.at(mouse.x, mouse.y),
                        Some(Control::Row(_) | Control::Scroll)
                    )
                {
                    return Some(key_event(if mouse.scroll > 0 { "down" } else { "up" }));
                }
                Some(event.clone())
            })
            .collect()
    }
    #[must_use]
    pub fn modal_events(&self, events: &[InputEvent]) -> Vec<InputEvent> {
        events
            .iter()
            .map(|event| match self.click(event) {
                Some(Control::Button(key)) => key_event(&key),
                _ => event.clone(),
            })
            .collect()
    }
    #[must_use]
    pub fn field(&self, events: &[InputEvent]) -> Option<usize> {
        events.iter().find_map(|event| match self.click(event) {
            Some(Control::Field(index)) => Some(index),
            _ => None,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct DoubleClick {
    last: Option<(usize, Instant)>,
}
impl DoubleClick {
    pub fn reset(&mut self) {
        self.last = None;
    }
    pub fn click(&mut self, row: usize, now: Instant) -> bool {
        let double = self.last.is_some_and(|(previous, time)| {
            previous == row && now.saturating_duration_since(time) <= Duration::from_millis(450)
        });
        self.last = if double { None } else { Some((row, now)) };
        double
    }
}

#[must_use]
pub fn key_event(name: &str) -> InputEvent {
    InputEvent::Key(KeyEvent {
        name: name.into(),
        key: name.into(),
        raw: String::new(),
        char: None,
        ctrl: name.starts_with("ctrl+"),
        alt: false,
        shift: name.starts_with("shift+"),
    })
}

#[must_use]
pub fn coord(value: usize) -> isize {
    isize::try_from(value).unwrap_or(isize::MAX)
}

pub trait Interactive {
    fn mouse_button(&mut self, options: ButtonOptions, id: &str, hits: &Interactions);
    fn mouse_table(&mut self, options: TableOptions, hits: &Interactions);
    fn mouse_table_from(&mut self, options: TableOptions, base: usize, hits: &Interactions);
    fn mouse_modal(&mut self, options: ModalOptions, fields: &[&str], hits: &Interactions);
}
impl Interactive for Container<'_> {
    fn mouse_button(&mut self, options: ButtonOptions, id: &str, hits: &Interactions) {
        let hits = hits.clone();
        let id = id.to_owned();
        self.draw(move |surface| {
            let width = draw_button(surface, &options);
            hits.add(
                &surface.sub(0, 0, width.min(surface.width()), 1),
                Control::Button(id),
            );
        });
    }
    fn mouse_table(&mut self, options: TableOptions, hits: &Interactions) {
        self.mouse_table_from(options, 0, hits);
    }
    fn mouse_table_from(&mut self, mut options: TableOptions, base: usize, hits: &Interactions) {
        let hits = hits.clone();
        self.draw(move |surface| {
            let header = usize::from(options.header);
            let capacity = surface.height().saturating_sub(header);
            let offset = resolve_offset(
                options.offset,
                options.selected,
                capacity,
                options.rows.len(),
                options.follow_selection,
            );
            options.offset = Some(offset);
            options.follow_selection = false;
            draw_table(surface, &options);
            hits.add(surface, Control::Scroll);
            for index in offset..options.rows.len().min(offset + capacity) {
                hits.add(
                    &surface.sub(0, coord(header + index - offset), surface.width(), 1),
                    Control::Row(base + index),
                );
            }
        });
    }
    fn mouse_modal(&mut self, options: ModalOptions, fields: &[&str], hits: &Interactions) {
        let hits = hits.clone();
        let fields: Vec<String> = fields.iter().map(|field| (*field).into()).collect();
        self.modal(options.clone());
        self.sized(Size::Cells(1), move |ui| {
            ui.draw(move |surface| {
                hits.clear();
                let root = Surface::root(Rc::clone(surface.buffer()), Rc::clone(&surface.theme));
                let inner = draw_modal(&root, &options);
                if let Some(message) = &options.message {
                    let mut y = 1;
                    for source in message.split('\n') {
                        let field = fields.iter().position(|field| {
                            source.trim_start_matches([' ', '›']).starts_with(field)
                        });
                        let lines = wrap(source, inner.width().saturating_sub(2));
                        for _ in &lines {
                            if y >= inner.height().saturating_sub(2) {
                                break;
                            }
                            if let Some(index) = field {
                                hits.add(
                                    &inner.sub(1, coord(y), inner.width().saturating_sub(2), 1),
                                    Control::Field(index),
                                );
                            }
                            y += 1;
                        }
                    }
                }
                let widths: Vec<_> = options
                    .buttons
                    .iter()
                    .map(|button| string_width(&button.label) + 4)
                    .collect();
                let total = widths.iter().sum::<usize>() + widths.len().saturating_sub(1) * 2;
                let mut x = inner.width().saturating_sub(total) / 2;
                for (index, width) in widths.into_iter().enumerate() {
                    hits.add(
                        &inner.sub(coord(x), coord(inner.height().saturating_sub(2)), width, 1),
                        Control::Button(if index == 0 { "escape" } else { "enter" }.into()),
                    );
                    x += width + 2;
                }
            });
        });
    }
}

pub fn region<'a>(
    ui: &mut Container<'a>,
    hits: Interactions,
    action: Control,
    build: impl FnOnce(&mut Container<'a>) + 'a,
) {
    let capabilities = Rc::new(ui.capabilities().clone());
    ui.draw(move |surface| {
        let services = Rc::new(hqtui::ui::Ctx::new(
            Rc::clone(&surface.theme),
            capabilities,
            surface.width(),
            surface.height(),
        ));
        let mut content = Container::new(
            surface.clone(),
            services,
            hqtui::layout::Direction::Column,
            &hqtui::ui::Layout::default(),
        );
        build(&mut content);
        content.flush();
        hits.underlay(surface, action);
    });
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    pub fn click(x: usize, y: usize) -> InputEvent {
        InputEvent::Mouse(hqtui::input::MouseEvent {
            x,
            y,
            action: MouseAction::Press,
            button: MouseButton::Left,
            scroll: 0,
            ctrl: false,
            alt: false,
            shift: false,
        })
    }
    pub fn click_text(text: &str, target: &str) -> InputEvent {
        text.lines()
            .enumerate()
            .find_map(|(y, line)| {
                line.find(target).map(|offset| {
                    click(string_width(&line[..offset]) + string_width(target) / 2, y)
                })
            })
            .unwrap_or_else(|| panic!("Missing {target:?} in {text}"))
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{click, click_text};
    use super::*;
    use hqtui::{buffer::FrameBuffer, testing::render_to_text};

    #[test]
    fn modal_fields_keep_wrapped_values_clickable_and_replace_background_targets() {
        let hits = Interactions::default();
        for (width, height) in [(80, 24), (60, 18)] {
            let text = render_to_text(width, height, "dark", |ui| {
                ui.mouse_button(ButtonOptions::new("Behind"), "background", &hits);
                ui.mouse_modal(
                    ModalOptions {
                        title: Some("Form".into()),
                        width: Some(58),
                        align: Some(Align::Left),
                        message: Some(format!("Nome: {} CONTINUACAO\nCor: azul", "a".repeat(65))),
                        buttons: vec![ModalButton::new("Cancelar"), ModalButton::new("Salvar")],
                        ..Default::default()
                    },
                    &["Nome:", "Cor:"],
                    &hits,
                );
            });
            assert_eq!(hits.field(&[click_text(&text, "CONTINUACAO")]), Some(0));
            assert_eq!(hits.field(&[click_text(&text, "Cor:")]), Some(1));
            assert_eq!(hits.at(0, 0), None);
            assert!(hits.pressed(&[click_text(&text, "Cancelar")], "escape"));
            assert!(hits.pressed(&[click_text(&text, "Salvar")], "enter"));
        }
    }

    #[test]
    fn hit_regions_intersect_the_surface_clip_and_last_drawn_wins() {
        let root = Surface::root(
            Rc::new(RefCell::new(FrameBuffer::new(20, 10))),
            Rc::new(Theme::default()),
        );
        let clipped = root.sub(5, 3, 4, 2).sub(-3, -2, 20, 20);
        let hits = HitMap::default();
        hits.add(&clipped, 1);
        assert_eq!(hits.at(5, 3), Some(1));
        for point in [(4, 3), (9, 3), (5, 2), (5, 5), (usize::MAX, 0)] {
            assert_eq!(hits.at(point.0, point.1), None);
        }
        hits.add(&root.sub(6, 3, 1, 1), 2);
        assert_eq!(hits.at(6, 3), Some(2));
        hits.clear();
        assert_eq!(hits.at(6, 3), None);
    }

    #[test]
    fn table_clicks_follow_the_rendered_offset_after_resize() {
        let hits = Interactions::default();
        for (width, height, selected) in [(40, 8, 12), (24, 4, 18)] {
            hits.clear();
            let text = render_to_text(width, height, "dark", |ui| {
                ui.mouse_table(
                    TableOptions {
                        columns: vec![TableColumn::new("Name")],
                        rows: (0..20)
                            .map(|index| TableRow::new([format!("record-{index:02}")]))
                            .collect(),
                        selected: Some(selected),
                        follow_selection: true,
                        ..Default::default()
                    },
                    &hits,
                );
            });
            for index in 0..20 {
                let label = format!("record-{index:02}");
                if text.contains(&label) {
                    assert_eq!(
                        hits.click(&click_text(&text, &label)),
                        Some(Control::Row(index))
                    );
                }
            }
            assert_eq!(hits.at(0, height), None);
            assert_eq!(
                hits.click(&click_text(&text, "Name")),
                Some(Control::Scroll)
            );
        }
    }

    #[test]
    fn wheel_only_scrolls_list_regions_and_right_click_does_not_activate() {
        let hits = Interactions::default();
        let root = Surface::root(
            Rc::new(RefCell::new(FrameBuffer::new(20, 10))),
            Rc::new(Theme::default()),
        );
        hits.add(&root.sub(0, 0, 10, 1), Control::Button("new".into()));
        hits.add(&root.sub(0, 2, 10, 4), Control::Scroll);
        let mut selected = Some(0);
        let mut event = click(2, 0);
        if let InputEvent::Mouse(mouse) = &mut event {
            mouse.button = MouseButton::Right;
        }
        assert!(!hits.pressed(&[event.clone()], "new"));
        if let InputEvent::Mouse(mouse) = &mut event {
            mouse.action = MouseAction::Scroll;
            mouse.scroll = 1;
        }
        assert!(matches!(
            hits.list_events(&[event.clone()], &mut selected)[0],
            InputEvent::Mouse(_)
        ));
        if let InputEvent::Mouse(mouse) = &mut event {
            mouse.y = 3;
        }
        assert!(
            matches!(&hits.list_events(&[event], &mut selected)[0], InputEvent::Key(key) if key.name == "down")
        );
    }

    #[test]
    fn double_click_requires_the_same_row_and_resets_after_activation_or_navigation() {
        let mut sequence = DoubleClick::default();
        let now = Instant::now();
        assert!(!sequence.click(1, now));
        assert!(!sequence.click(2, now + Duration::from_millis(20)));
        assert!(sequence.click(2, now + Duration::from_millis(40)));
        assert!(!sequence.click(2, now + Duration::from_millis(60)));
        assert!(!sequence.click(2, now + Duration::from_millis(600)));
        sequence.reset();
        assert!(!sequence.click(2, now + Duration::from_millis(620)));
    }
}
