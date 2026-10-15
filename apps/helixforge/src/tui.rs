//! Preview-only terminal interface. All privileged work happens in the explicit CLI path.

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use helixforge_catalog::Catalog;
use helixforge_core::Tool;
use helixforge_engine::{PlanResult, plan_install};
use helixforge_platform::{LinuxInfo, binary_exists};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

const CYAN: Color = Color::Rgb(104, 232, 246);
const PURPLE: Color = Color::Rgb(177, 139, 255);
const MUTED: Color = Color::Rgb(143, 160, 179);
const FG: Color = Color::Rgb(231, 236, 250);
const BG: Color = Color::Rgb(11, 15, 28);
const SELECTED: Color = Color::Rgb(30, 48, 74);

#[derive(Clone, Copy)]
enum Screen {
    Browse,
    Search,
    Preview,
    Doctor,
}

struct State {
    screen: Screen,
    selected: usize,
    query: String,
}

impl State {
    fn new() -> Self {
        Self {
            screen: Screen::Browse,
            selected: 0,
            query: String::new(),
        }
    }

    fn visible<'a>(&self, catalog: &'a Catalog) -> Vec<&'a Tool> {
        let q = self.query.to_ascii_lowercase();
        catalog
            .all()
            .iter()
            .filter(|t| {
                t.name.to_ascii_lowercase().contains(&q)
                    || t.id.to_ascii_lowercase().contains(&q)
                    || t.category.to_ascii_lowercase().contains(&q)
            })
            .collect()
    }

    fn selected<'a>(&self, catalog: &'a Catalog) -> Option<&'a Tool> {
        self.visible(catalog).get(self.selected).copied()
    }

    fn handle(&mut self, key: KeyEvent, catalog: &Catalog) -> bool {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return true;
        }
        if matches!(self.screen, Screen::Search) {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => self.screen = Screen::Browse,
                KeyCode::Backspace => {
                    self.query.pop();
                    self.selected = 0;
                }
                KeyCode::Char(c) => {
                    self.query.push(c);
                    self.selected = 0;
                }
                _ => {}
            }
            return false;
        }
        match key.code {
            KeyCode::Char('q') => return true,
            KeyCode::Esc => self.screen = Screen::Browse,
            KeyCode::Char('/') => self.screen = Screen::Search,
            KeyCode::Char('d') => self.screen = Screen::Doctor,
            KeyCode::Char('p') | KeyCode::Enter => {
                if self.selected(catalog).is_some() {
                    self.screen = Screen::Preview;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.visible(catalog).len().saturating_sub(1);
                self.selected = (self.selected + 1).min(max);
                self.screen = Screen::Browse;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                self.screen = Screen::Browse;
            }
            _ => {}
        }
        false
    }
}

pub fn run(catalog: &Catalog, os: &LinuxInfo) -> Result<()> {
    let mut terminal = ratatui::init();
    let result = run_event_loop(&mut terminal, catalog, os);
    ratatui::restore();
    result
}

fn run_event_loop(terminal: &mut DefaultTerminal, catalog: &Catalog, os: &LinuxInfo) -> Result<()> {
    let mut state = State::new();
    loop {
        terminal.draw(|frame| render(frame, &state, catalog, os))?;
        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
            && state.handle(key, catalog)
        {
            return Ok(());
        }
    }
}

fn border(title: &'static str) -> Block<'static> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PURPLE))
        .style(Style::default().bg(BG).fg(FG))
}

fn render(frame: &mut Frame<'_>, state: &State, catalog: &Catalog, os: &LinuxInfo) {
    let area = frame.area();
    frame.render_widget(Block::default().style(Style::default().bg(BG)), area);
    let vertical = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(6),
        Constraint::Length(3),
    ])
    .split(area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "  ⬡ HELIXFORGE",
                Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "    BUILD • CONFIGURE • OPTIMIZE",
                Style::default().fg(PURPLE),
            ),
        ]))
        .block(border(" v0.1.0 · TUI Preview ")),
        vertical[0],
    );
    let body = Layout::horizontal([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(vertical[1]);
    draw_catalog(frame, body[0], state, catalog);
    draw_detail(frame, body[1], state, catalog, os);
    let footer = match state.screen {
        Screen::Search => format!(" SEARCH: {}█    Enter Apply   Esc Close", state.query),
        Screen::Preview => " VIEW PLAN  •  Esc Browse   / Search   d Doctor   q Quit".into(),
        Screen::Doctor => " SYSTEM DOCTOR  •  Esc Browse   / Search   q Quit".into(),
        Screen::Browse => " ↑↓/jk Navigate   / Search   Enter Preview   d Doctor   q Quit".into(),
    };
    frame.render_widget(
        Paragraph::new(footer)
            .style(Style::default().fg(MUTED))
            .block(border(" Keyboard shortcuts ")),
        vertical[2],
    );
}

fn draw_catalog(frame: &mut Frame<'_>, area: Rect, state: &State, catalog: &Catalog) {
    let tools = state.visible(catalog);
    let items: Vec<ListItem<'_>> = tools
        .iter()
        .map(|tool| {
            let status = if binary_exists(&tool.detect.binary) {
                "●"
            } else {
                "○"
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {status} "), Style::default().fg(CYAN)),
                Span::raw(format!("{}  ", tool.name)),
                Span::styled(tool.category.clone(), Style::default().fg(MUTED)),
            ]))
        })
        .collect();
    let title = format!(" Applications ({}) ", tools.len());
    let list = List::new(items)
        .block(border(" Catalog ").title(title))
        .highlight_style(
            Style::default()
                .bg(SELECTED)
                .fg(CYAN)
                .add_modifier(Modifier::BOLD),
        );
    let mut selected = ListState::default();
    if !tools.is_empty() {
        selected.select(Some(state.selected));
    }
    frame.render_stateful_widget(list, area, &mut selected);
}

fn draw_detail(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &State,
    catalog: &Catalog,
    os: &LinuxInfo,
) {
    if matches!(state.screen, Screen::Doctor) {
        let manager_name = os
            .manager
            .map(|m| m.label())
            .unwrap_or("Unsupported for installs");
        let available = os
            .manager
            .map(|m| binary_exists(m.executable()))
            .unwrap_or(false);
        let content = format!(
            "\n  Operating system: {}\n\n  Distro ID: {}\n\n  Package manager: {}\n\n  Package command present: {}\n\n  sudo present: {}\n\n  Diagnostic mode is read-only.\n  No system changes are made.",
            os.pretty_name,
            os.id,
            manager_name,
            available,
            binary_exists("sudo"),
        );
        frame.render_widget(
            Paragraph::new(content)
                .wrap(Wrap { trim: false })
                .block(border(" System Doctor ")),
            area,
        );
        return;
    }
    let Some(tool) = state.selected(catalog) else {
        frame.render_widget(
            Paragraph::new("  No matching applications. Try another search.")
                .block(border(" Details ")),
            area,
        );
        return;
    };
    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  {}", tool.name),
            Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
        )),
        Line::from(format!("  {}", tool.category)),
        Line::from(""),
        Line::from(format!("  {}", tool.description)),
        Line::from(""),
        Line::from(format!("  Binary: {}", tool.detect.binary)),
        Line::from(format!(
            "  Detected: {}",
            if binary_exists(&tool.detect.binary) {
                "Yes"
            } else {
                "No"
            }
        )),
        Line::from(format!("  OS: {}", os.pretty_name)),
        Line::from(""),
    ];
    match plan_install(tool, os) {
        Ok(PlanResult::AlreadyDetected { .. }) => {
            lines.push(Line::from(Span::styled(
                "  Executable already detected — no install planned.",
                Style::default().fg(CYAN),
            )));
        }
        Ok(PlanResult::Ready(plan)) => {
            lines.push(Line::from(format!("  Native package: {}", plan.package)));
            if matches!(state.screen, Screen::Preview) {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "  INSTALLATION PLAN",
                    Style::default().fg(PURPLE),
                )));
                lines.push(Line::from(format!("  {}", plan.command.display())));
                lines.push(Line::from(""));
                lines.push(Line::from(
                    "  Preview only. No changes are made in this TUI.",
                ));
                lines.push(Line::from(format!(
                    "  CLI: helixforge install {} --dry-run",
                    tool.id
                )));
            } else {
                lines.push(Line::from("  Press Enter to preview installation."));
            }
        }
        Err(e) => lines.push(Line::from(format!("  Unavailable: {e}"))),
    }
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(border(" Tool details ")),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    #[test]
    fn search_reduces_catalog() {
        let catalog = Catalog::builtin().unwrap();
        let mut state = State::new();
        state.query = "rip".into();
        assert_eq!(state.visible(&catalog).len(), 1);
        assert_eq!(state.selected(&catalog).unwrap().id, "ripgrep");
    }

    #[test]
    fn quit_key_works() {
        let catalog = Catalog::builtin().unwrap();
        let mut state = State::new();
        assert!(state.handle(
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
            &catalog
        ));
    }
}
