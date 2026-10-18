//! The change preview window: each step's expected changes, marked by how
//! they are known, and a side-by-side view of a file now and afterwards.

use super::{hints::Hint, theme::Theme};
use crossterm::event::{KeyCode, KeyEvent};
use helixforge_engine::{
    Plan,
    preview::{Certainty, Diff, StepPreview, Tag},
};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Clear, List, ListState, Paragraph},
};

/// A line of the list: a step's name, or one of its changes.
enum Item {
    Step(String),
    Change {
        certainty: Certainty,
        text: String,
        diff: Option<Diff>,
    },
}

pub struct EffectsPopup {
    items: Vec<Item>,
    state: ListState,
    /// The change whose file is shown side by side, and how far down.
    showing: Option<(usize, u16)>,
    /// Whether closing goes back to the confirmation.
    pub from_confirmation: bool,
    visible: u16,
    /// The plan shown, and whether a pinned source it reads can be
    /// downloaded to compare the files it writes.
    pub plan: Plan,
    pub download: bool,
}

/// What a key did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    Open,
    Closed,
    /// Download the pinned sources, then show the changes again.
    Download,
}

impl EffectsPopup {
    pub fn new(
        steps: Vec<StepPreview>,
        from_confirmation: bool,
        plan: Plan,
        download: bool,
    ) -> Self {
        let mut items = Vec::new();
        for step in steps {
            items.push(Item::Step(step.name));
            for change in step.changes {
                items.push(Item::Change {
                    certainty: change.certainty,
                    text: change.text,
                    diff: change.diff,
                });
            }
        }
        let first = items
            .iter()
            .position(|item| matches!(item, Item::Change { .. }))
            .unwrap_or(0);
        Self {
            items,
            state: ListState::default().with_selected(Some(first)),
            showing: None,
            from_confirmation,
            visible: 0,
            plan,
            download,
        }
    }

    fn selected_diff(&self) -> Option<usize> {
        let index = self.state.selected()?;
        match self.items.get(index)? {
            Item::Change { diff: Some(_), .. } => Some(index),
            _ => None,
        }
    }

    pub fn handle_key(&mut self, key: &KeyEvent) -> Outcome {
        if let Some((index, scroll)) = self.showing {
            match key.code {
                KeyCode::Esc
                | KeyCode::Char('q')
                | KeyCode::Enter
                | KeyCode::Left
                | KeyCode::Char('h') => self.showing = None,
                KeyCode::Char('j') | KeyCode::Down => self.showing = Some((index, scroll + 1)),
                KeyCode::Char('k') | KeyCode::Up => {
                    self.showing = Some((index, scroll.saturating_sub(1)))
                }
                KeyCode::PageDown => self.showing = Some((index, scroll + self.visible.max(1))),
                KeyCode::PageUp => {
                    self.showing = Some((index, scroll.saturating_sub(self.visible.max(1))))
                }
                _ => {}
            }
            return Outcome::Open;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return Outcome::Closed,
            KeyCode::Char('d') if self.download => return Outcome::Download,
            KeyCode::Char('j') | KeyCode::Down => self.step(1),
            KeyCode::Char('k') | KeyCode::Up => self.step(-1),
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                // A file side by side, or any other change in full.
                if let Some(index) = self.state.selected()
                    && matches!(self.items.get(index), Some(Item::Change { .. }))
                {
                    self.showing = Some((index, 0));
                }
            }
            _ => {}
        }
        Outcome::Open
    }

    fn step(&mut self, by: isize) {
        let count = self.items.len() as isize;
        if count == 0 {
            return;
        }
        let mut index = self.state.selected().unwrap_or(0) as isize;
        // Step headings are passed over.
        loop {
            index += by;
            if index < 0 || index >= count {
                return;
            }
            if matches!(self.items[index as usize], Item::Change { .. }) {
                self.state.select(Some(index as usize));
                return;
            }
        }
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect, theme: Theme) {
        let title = match self.showing.and_then(|(index, _)| self.items.get(index)) {
            Some(Item::Change { diff: Some(_), .. }) => " SIDE BY SIDE ",
            Some(_) => " CHANGE ",
            None => " CHANGES ",
        };
        let block = Block::bordered()
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(theme.focused()))
            .title(title)
            .title_alignment(Alignment::Center)
            .title_style(Style::default().fg(theme.title()).bold());
        let inner = block.inner(area);
        frame.render_widget(Clear, area);
        frame.render_widget(Block::default().style(theme.card()), inner);
        frame.render_widget(block, area);
        self.visible = inner.height.saturating_sub(2);
        if let Some((index, scroll)) = self.showing {
            match self.items.get(index) {
                Some(Item::Change {
                    diff: Some(diff), ..
                }) => {
                    draw_diff(frame, inner, diff, scroll, theme);
                    return;
                }
                Some(Item::Change {
                    certainty, text, ..
                }) => {
                    let body = format!("{} {}\n\n{}", certainty.mark(), certainty.label(), text);
                    frame.render_widget(
                        Paragraph::new(body)
                            .wrap(ratatui::widgets::Wrap { trim: false })
                            .scroll((scroll, 0))
                            .style(Style::default().fg(theme.entry())),
                        Rect::new(
                            inner.x + 1,
                            inner.y,
                            inner.width.saturating_sub(2),
                            inner.height,
                        ),
                    );
                    return;
                }
                _ => {}
            }
        }
        let legend = Line::from(vec![
            Span::styled("✓ known  ", Style::default().fg(theme.success())),
            Span::styled("~ expected  ", Style::default().fg(theme.warning())),
            Span::styled("? not known in advance", Style::default().fg(theme.muted())),
        ]);
        frame.render_widget(
            Paragraph::new(legend),
            Rect::new(inner.x + 1, inner.y, inner.width.saturating_sub(2), 1),
        );
        let lines: Vec<Line> = self
            .items
            .iter()
            .map(|item| match item {
                Item::Step(name) => {
                    Line::styled(name.clone(), Style::default().fg(theme.entry()).bold())
                }
                Item::Change {
                    certainty,
                    text,
                    diff,
                } => {
                    let color = match certainty {
                        Certainty::Known => theme.success(),
                        Certainty::Expected => theme.warning(),
                        Certainty::Unknown => theme.muted(),
                    };
                    // Where a file can be compared, say so before the text,
                    // which may run past the window's edge.
                    let compare = if diff.as_ref().is_some_and(|diff| diff.changed) {
                        "[side by side] "
                    } else {
                        ""
                    };
                    Line::from(vec![
                        Span::styled(
                            format!("  {} ", certainty.mark()),
                            Style::default().fg(color).bold(),
                        ),
                        Span::styled(compare, Style::default().fg(theme.focused())),
                        Span::styled(text.clone(), Style::default().fg(theme.entry())),
                    ])
                }
            })
            .collect();
        let list = List::new(lines)
            .highlight_style(theme.selection())
            .scroll_padding(1);
        frame.render_stateful_widget(
            list,
            Rect::new(
                inner.x,
                inner.y + 2,
                inner.width,
                inner.height.saturating_sub(2),
            ),
            &mut self.state,
        );
    }

    pub fn hints(&self) -> (&'static str, Vec<Hint>) {
        if self.showing.is_some() {
            return (
                "Change",
                vec![Hint::new(&["↑↓"], "Scroll"), Hint::new(&["Esc"], "Back")],
            );
        }
        let mut hints = vec![Hint::new(&["↑↓"], "Choose")];
        hints.push(Hint::new(
            &["Enter"],
            if self.selected_diff().is_some() {
                "Side by side"
            } else {
                "Read it all"
            },
        ));
        if self.download {
            hints.push(Hint::new(&["d"], "Download the source to compare"));
        }
        hints.push(Hint::new(
            &["Esc"],
            if self.from_confirmation {
                "Back to the confirmation"
            } else {
                "Close"
            },
        ));
        ("Changes", hints)
    }
}

/// A file now on the left and afterwards on the right, the differing rows
/// with a few around them.
fn draw_diff(frame: &mut Frame, area: Rect, diff: &Diff, scroll: u16, theme: Theme) {
    let half = area.width.saturating_sub(3) / 2;
    let heading = Line::from(vec![
        Span::styled(
            format!(
                "{:<width$}",
                format!(" Now: {}", diff.path),
                width = half as usize + 1
            ),
            Style::default().fg(theme.entry()).bold(),
        ),
        Span::styled("│", Style::default().fg(theme.line())),
        Span::styled(" After", Style::default().fg(theme.entry()).bold()),
    ]);
    frame.render_widget(
        Paragraph::new(heading),
        Rect::new(area.x, area.y, area.width, 1),
    );
    if !diff.changed {
        frame.render_widget(
            Paragraph::new(" Nothing in this file changes.")
                .style(Style::default().fg(theme.muted())),
            Rect::new(area.x, area.y + 2, area.width, 1),
        );
        return;
    }
    let clip = |text: &str| -> String {
        let shown: String = text.chars().take(half.saturating_sub(1) as usize).collect();
        format!(" {shown:<width$}", width = half.saturating_sub(1) as usize)
    };
    let rows = diff.hunks(3);
    let lines: Vec<Line> = rows
        .iter()
        .skip(scroll as usize)
        .take(area.height.saturating_sub(1) as usize)
        .map(|row| match row {
            None => Line::styled(" ⋯", Style::default().fg(theme.faint())),
            Some(row) => {
                let (left, right) = match row.tag {
                    Tag::Same => (theme.muted(), theme.muted()),
                    Tag::Changed => (theme.warning(), theme.warning()),
                    Tag::Removed => (theme.failure(), theme.muted()),
                    Tag::Added => (theme.muted(), theme.success()),
                };
                Line::from(vec![
                    Span::styled(
                        clip(row.before.as_deref().unwrap_or("")),
                        Style::default().fg(left),
                    ),
                    Span::styled(" │", Style::default().fg(theme.line())),
                    Span::styled(
                        clip(row.after.as_deref().unwrap_or("")),
                        Style::default().fg(right),
                    ),
                ])
            }
        })
        .collect();
    frame.render_widget(
        Paragraph::new(lines),
        Rect::new(
            area.x,
            area.y + 1,
            area.width,
            area.height.saturating_sub(1),
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn press(popup: &mut EffectsPopup, code: KeyCode) -> Outcome {
        popup.handle_key(&KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn a_missing_source_can_be_downloaded_from_the_changes() {
        let mut popup = EffectsPopup::new(Vec::new(), false, Plan::default(), true);
        assert_eq!(popup.hints().1.len(), 4);
        assert_eq!(press(&mut popup, KeyCode::Char('d')), Outcome::Download);
        let mut popup = EffectsPopup::new(Vec::new(), false, Plan::default(), false);
        assert_eq!(popup.hints().1.len(), 3);
        assert_eq!(press(&mut popup, KeyCode::Char('d')), Outcome::Open);
    }
}
