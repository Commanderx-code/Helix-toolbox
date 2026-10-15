//! Windows drawn over the entry list: scrollable text, and the confirmation
//! before entries run.

use super::{hints::Hint, theme::Theme};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    symbols::border,
    text::{Line, Span, Text},
    widgets::{Block, Clear, Paragraph, Wrap},
};

/// A centred rectangle covering the given percentages of `area`.
pub fn centered(area: Rect, width_percent: u16, height_percent: u16) -> Rect {
    let horizontal = Layout::horizontal([
        Constraint::Percentage((100 - width_percent) / 2),
        Constraint::Percentage(width_percent),
        Constraint::Percentage((100 - width_percent) / 2),
    ])
    .split(area)[1];
    Layout::vertical([
        Constraint::Percentage((100 - height_percent) / 2),
        Constraint::Percentage(height_percent),
        Constraint::Percentage((100 - height_percent) / 2),
    ])
    .split(horizontal)[1]
}

pub struct TextPopup {
    title: String,
    text: String,
    wrap: bool,
    scroll: (u16, u16),
    line_count: u16,
    visible_lines: u16,
}

impl TextPopup {
    pub fn new(title: impl Into<String>, text: impl Into<String>, wrap: bool) -> Self {
        let text = text.into();
        Self {
            title: title.into(),
            line_count: text.lines().count() as u16,
            text,
            wrap,
            scroll: (0, 0),
            visible_lines: 0,
        }
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect, theme: Theme) {
        let block = Block::bordered()
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(theme.focused()))
            .title(format!(" {} ", self.title))
            .title_alignment(Alignment::Center)
            .title_style(Style::default().fg(theme.title()).bold());
        self.visible_lines = block.inner(area).height;
        let mut paragraph = Paragraph::new(self.text.as_str())
            .block(block)
            .scroll(self.scroll);
        if self.wrap {
            paragraph = paragraph.wrap(Wrap { trim: false });
        }
        frame.render_widget(Clear, area);
        frame.render_widget(
            Block::default().style(theme.card()),
            Block::bordered().inner(area),
        );
        frame.render_widget(paragraph, area);
    }

    /// Returns true when the window should close.
    pub fn handle_key(&mut self, key: &KeyEvent) -> bool {
        let max_scroll = self.line_count.saturating_sub(self.visible_lines.max(1));
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.scroll.0 = (self.scroll.0 + 1).min(max_scroll);
            }
            KeyCode::Char('k') | KeyCode::Up => self.scroll.0 = self.scroll.0.saturating_sub(1),
            KeyCode::PageDown => self.scroll.0 = (self.scroll.0 + 10).min(max_scroll),
            KeyCode::PageUp => self.scroll.0 = self.scroll.0.saturating_sub(10),
            KeyCode::Char('l') | KeyCode::Right if !self.wrap => self.scroll.1 += 4,
            KeyCode::Char('h') | KeyCode::Left if !self.wrap => {
                self.scroll.1 = self.scroll.1.saturating_sub(4);
            }
            KeyCode::Char('g') => self.scroll.0 = 0,
            KeyCode::Char('G') => self.scroll.0 = max_scroll,
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Enter => return true,
            _ => {}
        }
        false
    }

    pub fn hints(&self) -> (&'static str, Vec<Hint>) {
        (
            "Text",
            vec![
                Hint::new(&["Enter", "q", "Esc"], "Close"),
                Hint::new(&["k", "Up"], "Scroll up"),
                Hint::new(&["j", "Down"], "Scroll down"),
                Hint::new(&["Page up", "Page down"], "Scroll by page"),
            ],
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Pending,
    Run,
    Cancel,
}

pub struct ConfirmPopup {
    lines: Vec<String>,
    header_lines: usize,
    scroll: usize,
    visible_lines: usize,
    pub decision: Decision,
}

impl ConfirmPopup {
    /// `names` are the entries, each with the command it will run; `header`
    /// lines come first, such as the risk and whether a password is needed.
    pub fn new(header: &[String], names: &[(String, String)]) -> Self {
        let digits = names.len().to_string().len();
        let mut lines: Vec<String> = header.to_vec();
        if !header.is_empty() {
            lines.push(String::new());
        }
        for (index, (name, command)) in names.iter().enumerate() {
            lines.push(format!("{:>digits$}. {name}", index + 1));
            lines.push(format!("{:>digits$}  {command}", ""));
        }
        Self {
            lines,
            header_lines: header.len(),
            scroll: 0,
            visible_lines: 0,
            decision: Decision::Pending,
        }
    }

    /// `area` shrunk to the confirmation's height, centred in it.
    pub fn fitted(&self, area: Rect) -> Rect {
        // Borders, the lines, a blank line and the buttons with space around.
        let height = (self.lines.len() as u16 + 6).clamp(9, area.height.max(1));
        let height = height.min(area.height);
        Rect::new(
            area.x,
            area.y + (area.height - height) / 2,
            area.width,
            height,
        )
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect, theme: Theme) {
        let block = Block::bordered()
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(theme.focused()))
            .title(" CONFIRM ")
            .title_alignment(Alignment::Center)
            .title_style(Style::default().fg(theme.title()).bold());
        let inner = block.inner(area);
        let sections = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);
        self.visible_lines = sections[0].height as usize;
        frame.render_widget(Clear, area);
        frame.render_widget(
            Block::default().style(theme.card()),
            Block::bordered().inner(area),
        );
        frame.render_widget(block, area);
        let lines: Vec<Line> = self
            .lines
            .iter()
            .enumerate()
            .skip(self.scroll)
            .map(|(index, line)| {
                if index < self.header_lines {
                    Line::styled(line.as_str(), Style::default().fg(theme.title()))
                } else if line.starts_with(' ') {
                    Line::styled(line.as_str(), Style::default().fg(theme.muted()))
                } else {
                    Line::from(line.as_str())
                }
            })
            .collect();
        frame.render_widget(
            Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
            sections[0],
        );
        let button = |label: &'static str, colour| {
            Span::styled(
                label,
                Style::default().bg(colour).fg(theme.on_highlight()).bold(),
            )
        };
        let actions = Text::from(Line::from(vec![
            button(" [Y] Run ", theme.success()),
            Span::raw("    "),
            button(" [N] Cancel ", theme.failure()),
        ]));
        frame.render_widget(
            Paragraph::new(actions).alignment(Alignment::Center),
            sections[2],
        );
    }

    pub fn handle_key(&mut self, key: &KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => self.decision = Decision::Run,
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('q') | KeyCode::Esc => {
                self.decision = Decision::Cancel;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.scroll + self.visible_lines < self.lines.len() {
                    self.scroll += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => self.scroll = self.scroll.saturating_sub(1),
            _ => {}
        }
    }

    pub fn hints(&self) -> (&'static str, Vec<Hint>) {
        (
            "Confirmation",
            vec![
                Hint::new(&["y"], "Run"),
                Hint::new(&["n", "q", "Esc"], "Cancel"),
                Hint::new(&["k", "Up"], "Scroll up"),
                Hint::new(&["j", "Down"], "Scroll down"),
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn the_confirmation_needs_an_explicit_answer() {
        let mut popup = ConfirmPopup::new(&[], &[("Git".into(), "sudo pacman -S git".into())]);
        popup.handle_key(&KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(popup.decision, Decision::Pending);
        popup.handle_key(&KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
        assert_eq!(popup.decision, Decision::Run);
        for code in [KeyCode::Char('n'), KeyCode::Esc, KeyCode::Char('q')] {
            popup.handle_key(&KeyEvent::new(code, KeyModifiers::NONE));
            assert_eq!(popup.decision, Decision::Cancel);
        }
    }

    #[test]
    fn the_confirmation_shows_the_header_names_commands_and_choices() {
        let mut popup = ConfirmPopup::new(
            &["Risk: medium".into()],
            &[("Git".into(), "sudo pacman -S --needed git".into())],
        );
        let mut terminal = Terminal::new(TestBackend::new(40, 11)).unwrap();
        terminal
            .draw(|frame| popup.draw(frame, frame.area(), Theme::Solid))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let text: String = (0..11)
            .map(|y| (0..40).map(|x| buffer[(x, y)].symbol()).collect::<String>() + "\n")
            .collect();
        assert!(text.contains("Risk: medium"));
        assert!(text.contains("1. Git"));
        assert!(text.contains("sudo pacman -S --needed git"));
        assert!(text.contains("[Y] Run"));
        assert!(text.contains("[N] Cancel"));
    }

    #[test]
    fn text_scrolling_stops_at_the_last_line() {
        let mut popup = TextPopup::new("t", "a\nb\nc\nd", false);
        popup.visible_lines = 2;
        for _ in 0..5 {
            popup.handle_key(&KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        }
        assert_eq!(popup.scroll.0, 2);
        assert!(popup.handle_key(&KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)));
    }
}
