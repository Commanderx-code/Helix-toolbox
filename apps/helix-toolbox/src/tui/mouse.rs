//! The mouse: where the sidebar, the main list and the key hints were last
//! drawn, so a click or the wheel becomes the keys that do the same. Nothing
//! here acts by itself; every click goes through the keys' own code.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Position, Rect};

/// What a click landed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// A sidebar item, by its index in the menu.
    Menu(usize),
    /// A row of the main list, by its index in the list.
    Row(usize),
    /// A key hint: the key it names.
    Key(KeyEvent),
}

/// The main list as drawn: where its items start, the first one shown and
/// each item's height.
#[derive(Clone, Debug, Default)]
struct List {
    area: Rect,
    offset: usize,
    heights: Vec<u16>,
}

/// Everything clickable in the last frame.
#[derive(Clone, Debug, Default)]
pub struct Hits {
    menu: Vec<(Rect, usize)>,
    rows: Vec<(Rect, usize)>,
    list: Option<List>,
    keys: Vec<(Rect, KeyEvent)>,
    /// The heights of the list being built, until it is drawn.
    measured: Vec<u16>,
    /// The main area, where the wheel moves the selection.
    main: Rect,
}

impl Hits {
    /// Forget the last frame, before drawing the next.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn main(&mut self, area: Rect) {
        self.main = area;
    }

    pub fn in_main(&self, column: u16, row: u16) -> bool {
        self.main.contains(Position::new(column, row))
    }

    pub fn menu(&mut self, area: Rect, index: usize) {
        self.menu.push((area, index));
    }

    /// A row drawn on its own, such as a quick action's card.
    pub fn row(&mut self, area: Rect, index: usize) {
        self.rows.push((area, index));
    }

    /// The heights of the items of the list drawn next.
    pub fn measured(&mut self, heights: Vec<u16>) {
        self.measured = heights;
    }

    /// The main list, drawn with its items from `area`'s top, the first
    /// shown being `offset`.
    pub fn list(&mut self, area: Rect, offset: usize) {
        self.list = Some(List {
            area,
            offset,
            heights: std::mem::take(&mut self.measured),
        });
    }

    pub fn key(&mut self, area: Rect, key: KeyEvent) {
        self.keys.push((area, key));
    }

    /// What is at a cell, if anything: a key hint, a sidebar item, a row.
    pub fn at(&self, column: u16, row: u16) -> Option<Target> {
        let here = Position::new(column, row);
        let inside = |area: &Rect| area.contains(here);
        if let Some((_, key)) = self.keys.iter().find(|(area, _)| inside(area)) {
            return Some(Target::Key(*key));
        }
        if let Some((_, index)) = self.menu.iter().find(|(area, _)| inside(area)) {
            return Some(Target::Menu(*index));
        }
        if let Some((_, index)) = self.rows.iter().find(|(area, _)| inside(area)) {
            return Some(Target::Row(*index));
        }
        let list = self.list.as_ref().filter(|list| inside(&list.area))?;
        row_at(list.offset, &list.heights, row - list.area.y).map(Target::Row)
    }
}

/// The item at `line` lines below the top of a list showing items from
/// `offset` on, each as tall as `heights` says.
pub fn row_at(offset: usize, heights: &[u16], line: u16) -> Option<usize> {
    let mut top = 0u16;
    for (index, height) in heights.iter().enumerate().skip(offset) {
        let bottom = top.saturating_add(*height);
        if line < bottom {
            return Some(index);
        }
        top = bottom;
    }
    None
}

/// The key a hint names, as pressing it would send; none for a hint that
/// names a kind of key rather than one ("↑↓").
pub fn key_named(name: &str) -> Option<KeyEvent> {
    let plain = |code| Some(KeyEvent::new(code, KeyModifiers::NONE));
    let control = |character| {
        Some(KeyEvent::new(
            KeyCode::Char(character),
            KeyModifiers::CONTROL,
        ))
    };
    match name {
        "Enter" => plain(KeyCode::Enter),
        "Esc" => plain(KeyCode::Esc),
        "Space" => plain(KeyCode::Char(' ')),
        "Tab" => plain(KeyCode::Tab),
        "Del" => plain(KeyCode::Delete),
        "Up" => plain(KeyCode::Up),
        "Down" => plain(KeyCode::Down),
        "Page up" => plain(KeyCode::PageUp),
        "Page down" => plain(KeyCode::PageDown),
        "^K" => control('k'),
        "Ctrl-c" => control('c'),
        _ => {
            let mut characters = name.chars();
            match (characters.next(), characters.next()) {
                (Some(character), None) if character.is_ascii_graphic() => {
                    plain(KeyCode::Char(character))
                }
                _ => None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_of_a_list_is_the_item_drawn_there() {
        // Items of one, two and three lines, the first scrolled away.
        let heights = [1, 2, 3, 1];
        assert_eq!(row_at(1, &heights, 0), Some(1));
        assert_eq!(row_at(1, &heights, 1), Some(1));
        assert_eq!(row_at(1, &heights, 2), Some(2));
        assert_eq!(row_at(1, &heights, 4), Some(2));
        assert_eq!(row_at(1, &heights, 5), Some(3));
        assert_eq!(row_at(1, &heights, 6), None);
        assert_eq!(row_at(0, &[], 0), None);
    }

    #[test]
    fn a_click_finds_the_hint_then_the_menu_then_the_list() {
        let mut hits = Hits::default();
        hits.menu(Rect::new(0, 2, 20, 1), 3);
        hits.measured(vec![1, 1, 2]);
        hits.list(Rect::new(22, 5, 40, 10), 0);
        hits.key(Rect::new(0, 20, 5, 1), key_named("Enter").unwrap());
        assert_eq!(hits.at(4, 2), Some(Target::Menu(3)));
        assert_eq!(hits.at(30, 5), Some(Target::Row(0)));
        assert_eq!(hits.at(30, 8), Some(Target::Row(2)));
        assert_eq!(hits.at(30, 9), None);
        assert_eq!(
            hits.at(2, 20),
            Some(Target::Key(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE
            )))
        );
        assert_eq!(hits.at(70, 30), None);
        hits.clear();
        assert_eq!(hits.at(4, 2), None);
    }

    #[test]
    fn hints_name_the_keys_they_send() {
        assert_eq!(key_named("x").unwrap().code, KeyCode::Char('x'));
        assert_eq!(key_named("Space").unwrap().code, KeyCode::Char(' '));
        assert_eq!(key_named("Page down").unwrap().code, KeyCode::PageDown);
        let control_k = key_named("^K").unwrap();
        assert_eq!(control_k.code, KeyCode::Char('k'));
        assert!(control_k.modifiers.contains(KeyModifiers::CONTROL));
        assert_eq!(key_named("↑↓"), None);
        assert_eq!(key_named("Whatever"), None);
    }
}
