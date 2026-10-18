//! The setup wizard: what this machine is, a choice of workstation, then the
//! entries it suggests, ticked where they are not installed. It ends by
//! putting the ticked entries in the Install Plan; it runs nothing.

use super::{hints::Hint, theme::Theme};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Clear, List, ListState, Paragraph, Wrap},
};

/// A suggested entry as the wizard shows it.
pub struct Pick {
    pub id: String,
    pub name: String,
    pub available: bool,
    /// Installed, not available, runs on its own.
    pub note: Option<String>,
    pub ticked: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Page {
    Machine,
    Choose,
    Pick,
}

/// What a key did.
#[derive(Debug, Eq, PartialEq)]
pub enum Outcome {
    Open,
    Closed,
    /// A workstation was chosen, by index: the wizard wants its suggestions.
    Chosen(usize),
    /// Queue these entries.
    Queue(Vec<String>),
}

pub struct Wizard {
    machine: Vec<(&'static str, String)>,
    workstations: Vec<(String, String)>,
    page: Page,
    choice: ListState,
    picks: Vec<Pick>,
    list: ListState,
    chosen_name: String,
    /// Shown on the first start, which says so.
    pub first_run: bool,
}

impl Wizard {
    /// `workstations` are names and descriptions.
    pub fn new(
        machine: Vec<(&'static str, String)>,
        workstations: Vec<(String, String)>,
        first_run: bool,
    ) -> Self {
        Self {
            machine,
            workstations,
            page: Page::Machine,
            choice: ListState::default().with_selected(Some(0)),
            picks: Vec::new(),
            list: ListState::default().with_selected(Some(0)),
            chosen_name: String::new(),
            first_run,
        }
    }

    /// Show the chosen workstation's entries.
    pub fn pick(&mut self, picks: Vec<Pick>) {
        self.chosen_name = self
            .choice
            .selected()
            .and_then(|index| self.workstations.get(index))
            .map(|(name, _)| name.clone())
            .unwrap_or_default();
        self.picks = picks;
        self.list.select(Some(0));
        self.page = Page::Pick;
    }

    pub fn handle_key(&mut self, key: &KeyEvent) -> Outcome {
        match (self.page, key.code) {
            (Page::Machine, KeyCode::Esc | KeyCode::Char('q')) => Outcome::Closed,
            (Page::Machine, KeyCode::Enter) => {
                self.page = Page::Choose;
                Outcome::Open
            }
            (Page::Choose, KeyCode::Esc) => {
                self.page = Page::Machine;
                Outcome::Open
            }
            (Page::Choose, KeyCode::Char('j') | KeyCode::Down) => {
                step(&mut self.choice, self.workstations.len(), 1);
                Outcome::Open
            }
            (Page::Choose, KeyCode::Char('k') | KeyCode::Up) => {
                step(&mut self.choice, self.workstations.len(), -1);
                Outcome::Open
            }
            (Page::Choose, KeyCode::Enter) => match self.choice.selected() {
                Some(index) if index < self.workstations.len() => Outcome::Chosen(index),
                _ => Outcome::Open,
            },
            (Page::Pick, KeyCode::Esc) => {
                self.page = Page::Choose;
                Outcome::Open
            }
            (Page::Pick, KeyCode::Char('j') | KeyCode::Down) => {
                step(&mut self.list, self.picks.len(), 1);
                Outcome::Open
            }
            (Page::Pick, KeyCode::Char('k') | KeyCode::Up) => {
                step(&mut self.list, self.picks.len(), -1);
                Outcome::Open
            }
            (Page::Pick, KeyCode::Char(' ')) => {
                if let Some(pick) = self
                    .list
                    .selected()
                    .and_then(|index| self.picks.get_mut(index))
                    && pick.available
                {
                    pick.ticked = !pick.ticked;
                }
                Outcome::Open
            }
            (Page::Pick, KeyCode::Char('a')) => {
                let all = self
                    .picks
                    .iter()
                    .filter(|pick| pick.available)
                    .all(|pick| pick.ticked);
                for pick in self.picks.iter_mut().filter(|pick| pick.available) {
                    pick.ticked = !all;
                }
                Outcome::Open
            }
            (Page::Pick, KeyCode::Enter) => Outcome::Queue(
                self.picks
                    .iter()
                    .filter(|pick| pick.ticked && pick.available)
                    .map(|pick| pick.id.clone())
                    .collect(),
            ),
            _ => Outcome::Open,
        }
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect, theme: Theme) {
        let title = match self.page {
            Page::Machine => " SET UP THIS MACHINE · 1 of 3 ",
            Page::Choose => " A WORKSTATION · 2 of 3 ",
            Page::Pick => " WHAT IT ADDS · 3 of 3 ",
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
        let inner = Rect::new(
            inner.x + 1,
            inner.y,
            inner.width.saturating_sub(2),
            inner.height,
        );
        match self.page {
            Page::Machine => {
                let mut lines = Vec::new();
                if self.first_run {
                    lines.push(Line::styled(
                        "Welcome to HelixForge. This is what it found on this machine:",
                        Style::default().fg(theme.entry()).bold(),
                    ));
                    lines.push(Line::from(""));
                }
                for (label, value) in &self.machine {
                    lines.push(Line::from(vec![
                        Span::styled(format!("{label:<10}"), Style::default().fg(theme.muted())),
                        Span::styled(value.clone(), Style::default().fg(theme.entry())),
                    ]));
                }
                lines.push(Line::from(""));
                lines.push(Line::styled(
                    "Next, choose a workstation. HelixForge puts what it suggests in the Install Plan for you to review; nothing runs until you confirm it there.",
                    Style::default().fg(theme.muted()),
                ));
                frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
            }
            Page::Choose => {
                let items: Vec<Line> = self
                    .workstations
                    .iter()
                    .map(|(name, description)| {
                        Line::from(vec![
                            Span::styled(
                                format!("{name:<11}"),
                                Style::default().fg(theme.entry()).bold(),
                            ),
                            Span::styled(description.clone(), Style::default().fg(theme.muted())),
                        ])
                    })
                    .collect();
                frame.render_stateful_widget(
                    List::new(items)
                        .highlight_style(theme.selection())
                        .highlight_symbol("▌ "),
                    inner,
                    &mut self.choice,
                );
            }
            Page::Pick => {
                frame.render_widget(
                    Paragraph::new(Line::styled(
                        format!(
                            "{}: Space ticks or unticks, a all; Enter puts the ticked ones in the Install Plan.",
                            self.chosen_name
                        ),
                        Style::default().fg(theme.muted()),
                    )),
                    Rect::new(inner.x, inner.y, inner.width, 1),
                );
                if self.picks.is_empty() {
                    frame.render_widget(
                        Paragraph::new("It suggests nothing for this machine."),
                        Rect::new(inner.x, inner.y + 2, inner.width, 1),
                    );
                    return;
                }
                let items: Vec<Line> = self
                    .picks
                    .iter()
                    .map(|pick| {
                        let (mark, color) = match (pick.available, pick.ticked) {
                            (false, _) => ("[-]", theme.faint()),
                            (true, true) => ("[x]", theme.success()),
                            (true, false) => ("[ ]", theme.muted()),
                        };
                        Line::from(vec![
                            Span::styled(format!("{mark} "), Style::default().fg(color).bold()),
                            Span::styled(
                                pick.name.clone(),
                                Style::default().fg(if pick.available {
                                    theme.entry()
                                } else {
                                    theme.faint()
                                }),
                            ),
                            Span::styled(
                                pick.note
                                    .as_deref()
                                    .map(|note| format!("   {note}"))
                                    .unwrap_or_default(),
                                Style::default().fg(theme.muted()),
                            ),
                        ])
                    })
                    .collect();
                frame.render_stateful_widget(
                    List::new(items)
                        .highlight_style(theme.selection())
                        .highlight_symbol("▌ ")
                        .scroll_padding(1),
                    Rect::new(
                        inner.x,
                        inner.y + 2,
                        inner.width,
                        inner.height.saturating_sub(2),
                    ),
                    &mut self.list,
                );
            }
        }
    }

    pub fn hints(&self) -> (&'static str, Vec<Hint>) {
        let hints = match self.page {
            Page::Machine => vec![
                Hint::new(&["Enter"], "Next"),
                Hint::new(&["Esc"], "Not now"),
            ],
            Page::Choose => vec![
                Hint::new(&["↑↓"], "Choose"),
                Hint::new(&["Enter"], "Next"),
                Hint::new(&["Esc"], "Back"),
            ],
            Page::Pick => vec![
                Hint::new(&["Space"], "Tick"),
                Hint::new(&["a"], "All"),
                Hint::new(&["Enter"], "To the Install Plan"),
                Hint::new(&["Esc"], "Back"),
            ],
        };
        ("Setup", hints)
    }
}

fn step(state: &mut ListState, count: usize, by: isize) {
    if count == 0 {
        return;
    }
    let current = state.selected().unwrap_or(0) as isize;
    state.select(Some((current + by).clamp(0, count as isize - 1) as usize));
}
