//! The command palette (Ctrl-K): one searchable list of what the menu can do
//! from anywhere, screens, actions and entries, best match first.

use super::{hints::Hint, theme::Theme};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Clear, List, ListState, Paragraph},
};

/// One thing the palette offers.
#[derive(Clone, Debug)]
pub struct Choice {
    pub label: String,
    /// What it is: a screen, an action, an entry's area.
    pub detail: String,
    /// More words it is found by.
    pub keywords: String,
    /// Shown before anything is typed.
    pub listed: bool,
}

/// What a key did.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Outcome {
    Open,
    Closed,
    /// The index of the chosen item.
    Chosen(usize),
}

pub struct Palette {
    choices: Vec<Choice>,
    query: String,
    /// Indexes of the choices shown, best first.
    shown: Vec<usize>,
    state: ListState,
}

/// At most this many results are shown for a query.
const SHOWN: usize = 50;

impl Palette {
    pub fn new(choices: Vec<Choice>) -> Self {
        let mut palette = Self {
            choices,
            query: String::new(),
            shown: Vec::new(),
            state: ListState::default().with_selected(Some(0)),
        };
        palette.filter();
        palette
    }

    fn filter(&mut self) {
        let query = self.query.trim().to_lowercase();
        if query.is_empty() {
            self.shown = (0..self.choices.len())
                .filter(|&index| self.choices[index].listed)
                .collect();
        } else {
            let mut found: Vec<(u32, usize)> = self
                .choices
                .iter()
                .enumerate()
                .filter_map(|(index, choice)| {
                    let score = helixforge_core::search::score(
                        &query,
                        &choice.label,
                        &choice.keywords,
                        &choice.detail,
                    )?;
                    // Actions and screens before entries of the same score.
                    Some((score * 2 + u32::from(choice.listed), index))
                })
                .collect();
            found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            self.shown = found
                .into_iter()
                .take(SHOWN)
                .map(|(_, index)| index)
                .collect();
        }
        self.state.select(Some(0));
    }

    #[cfg(test)]
    pub fn labels(&self) -> Vec<&str> {
        self.shown
            .iter()
            .map(|&index| self.choices[index].label.as_str())
            .collect()
    }

    pub fn handle_key(&mut self, key: &KeyEvent) -> Outcome {
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => Outcome::Closed,
            KeyCode::Char('k') | KeyCode::Char('c') if control => Outcome::Closed,
            KeyCode::Enter => match self
                .state
                .selected()
                .and_then(|index| self.shown.get(index))
            {
                Some(&index) => Outcome::Chosen(index),
                None => Outcome::Open,
            },
            KeyCode::Down | KeyCode::Tab => {
                self.step(1);
                Outcome::Open
            }
            KeyCode::Char('n') | KeyCode::Char('j') if control => {
                self.step(1);
                Outcome::Open
            }
            KeyCode::Up | KeyCode::BackTab => {
                self.step(-1);
                Outcome::Open
            }
            KeyCode::Char('p') if control => {
                self.step(-1);
                Outcome::Open
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.filter();
                Outcome::Open
            }
            KeyCode::Char(character) if !control && !character.is_control() => {
                if self.query.chars().count() < 60 {
                    self.query.push(character);
                    self.filter();
                }
                Outcome::Open
            }
            _ => Outcome::Open,
        }
    }

    fn step(&mut self, by: isize) {
        if self.shown.is_empty() {
            return;
        }
        let count = self.shown.len() as isize;
        let current = self.state.selected().unwrap_or(0) as isize;
        self.state
            .select(Some((current + by).rem_euclid(count) as usize));
    }

    /// The palette's place: near the top of `area`, as tall as it needs.
    pub fn fitted(&self, area: Rect) -> Rect {
        let width = area.width.saturating_sub(4).min(84);
        let height = (self.shown.len().max(1) as u16 + 5).min(area.height.saturating_sub(2).max(5));
        Rect::new(
            area.x + (area.width - width) / 2,
            area.y + 1.min(area.height),
            width,
            height.min(area.height),
        )
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect, theme: Theme) {
        let block = Block::bordered()
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(theme.focused()))
            .title(" COMMANDS ")
            .title_alignment(Alignment::Center)
            .title_style(Style::default().fg(theme.title()).bold());
        let inner = block.inner(area);
        frame.render_widget(Clear, area);
        frame.render_widget(Block::default().style(theme.card()), inner);
        frame.render_widget(block, area);
        if inner.height < 3 {
            return;
        }
        let input = Rect::new(inner.x + 1, inner.y, inner.width.saturating_sub(2), 1);
        let prompt = "> ";
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(prompt, Style::default().fg(theme.focused()).bold()),
                if self.query.is_empty() {
                    Span::styled(
                        "Type a command, a screen or an entry",
                        Style::default().fg(theme.muted()),
                    )
                } else {
                    Span::styled(
                        self.query.clone(),
                        Style::default().fg(theme.entry()).bold(),
                    )
                },
            ])),
            input,
        );
        let typed = (prompt.chars().count() + self.query.chars().count()) as u16;
        frame.set_cursor_position((
            (input.x + typed).min(input.right().saturating_sub(1)),
            input.y,
        ));
        frame.render_widget(
            Paragraph::new("─".repeat(inner.width as usize))
                .style(Style::default().fg(theme.line())),
            Rect::new(inner.x, inner.y + 1, inner.width, 1),
        );
        let list_area = Rect::new(
            inner.x,
            inner.y + 2,
            inner.width,
            inner.height.saturating_sub(2),
        );
        if self.shown.is_empty() {
            frame.render_widget(
                Paragraph::new(" Nothing matches.").style(Style::default().fg(theme.muted())),
                list_area,
            );
            return;
        }
        let width = list_area.width.saturating_sub(3) as usize;
        let items: Vec<Line> = self
            .shown
            .iter()
            .map(|&index| {
                let choice = &self.choices[index];
                let label = choice.label.chars().count();
                let detail: String = choice
                    .detail
                    .chars()
                    .take(width.saturating_sub(label + 2))
                    .collect();
                let gap = width.saturating_sub(label + detail.chars().count());
                Line::from(vec![
                    Span::styled(choice.label.clone(), Style::default().fg(theme.entry())),
                    Span::raw(" ".repeat(gap)),
                    Span::styled(detail, Style::default().fg(theme.muted())),
                ])
            })
            .collect();
        let list = List::new(items)
            .highlight_style(theme.selection())
            .highlight_symbol("▌ ")
            .scroll_padding(1);
        frame.render_stateful_widget(list, list_area, &mut self.state);
    }

    pub fn hints(&self) -> (&'static str, Vec<Hint>) {
        (
            "Commands",
            vec![
                Hint::new(&["Enter"], "Do it"),
                Hint::new(&["↑↓"], "Choose"),
                Hint::new(&["Esc"], "Close"),
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choice(label: &str, detail: &str, listed: bool) -> Choice {
        Choice {
            label: label.into(),
            detail: detail.into(),
            keywords: String::new(),
            listed,
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn typing_narrows_and_enter_chooses() {
        let mut palette = Palette::new(vec![
            choice("Open Updates", "screen", true),
            choice("Check updates online", "action", true),
            choice("Install Ghostty", "Applications", false),
        ]);
        // Entries wait for a query.
        assert_eq!(palette.labels(), ["Open Updates", "Check updates online"]);
        for character in "ghost".chars() {
            assert_eq!(
                palette.handle_key(&key(KeyCode::Char(character))),
                Outcome::Open
            );
        }
        assert_eq!(palette.labels(), ["Install Ghostty"]);
        assert_eq!(palette.handle_key(&key(KeyCode::Enter)), Outcome::Chosen(2));
        for _ in 0..5 {
            palette.handle_key(&key(KeyCode::Backspace));
        }
        palette.handle_key(&key(KeyCode::Down));
        assert_eq!(palette.handle_key(&key(KeyCode::Enter)), Outcome::Chosen(1));
        assert_eq!(palette.handle_key(&key(KeyCode::Esc)), Outcome::Closed);
        assert_eq!(
            palette.handle_key(&KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL)),
            Outcome::Closed
        );
    }
}
