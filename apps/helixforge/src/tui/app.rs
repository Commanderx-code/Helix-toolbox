//! The screens: a header, a sidebar menu, the current screen and a hint line,
//! after the HelixForge dashboard design.

use super::{
    hints::Hint,
    popup::{ConfirmPopup, Decision, TextPopup, centered},
    running::RunningView,
    sysinfo::Overview,
    theme::Theme,
};
use crate::TuiArgs;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use helixforge_catalog::Catalog;
use helixforge_core::{Action, Entry, Item};
use helixforge_engine::{EngineError, Plan, history, plan, plan_removal};
use helixforge_platform::LinuxInfo;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    symbols::border,
    text::{Line, Span, Text},
    widgets::{Block, List, ListState, Padding, Paragraph, Wrap},
};

const MIN_WIDTH: u16 = 100;
const MIN_HEIGHT: u16 = 25;
const SIDEBAR_WIDTH: u16 = 28;
const HIGHLIGHT: &str = "> ";
const GROUP_ICON: &str = "[D]";
const ENTRY_ICON: &str = "[*]";
const QUEUED_ICON: &str = "*";
const ROOT_WARNING: &str = "You are running HelixForge as root.\n\nEvery entry then has full access to the system. The entries expect to run as your normal user and ask for your password when they need it.";
const FLAGS_GUIDE: &str = "The flags column names what an entry does:

D  - disk changes, such as partitioning (privileged)
FI - Flatpak installation
FM - file changes
I  - installation (privileged)
K  - kernel changes (privileged)
MP - package manager actions
SI - full system installation
SS - service actions (privileged)
RP - package removal

P* - the same action, privileged

Risk in a plan: D, K and SI are high; installs, services and anything privileged are medium; the rest is low.
";
const HELP: &str = "Everywhere
  Tab / Shift-Tab   next / previous screen       t   switch theme
  /                 search every area            ?   this help
  q / Ctrl-C        quit

Sidebar
  j / k             move                         Enter / l   open the screen

Dashboard
  j / k             move                         Enter       quick action

Areas (Applications and the other catalog screens)
  j / k             move                         Enter / l   open a group or run an entry
  h                 parent group, or the sidebar Space       add to or remove from the install plan
  d                 description                  p           preview the script or command
  r                 remove an installed package (entries that offer it)
  g                 flags guide

Install Plan
  Enter             run the plan                 x / Delete  remove the selected entry
  c                 clear the plan

History
  Enter             show the saved log           r           reload

Settings
  Enter / Space     change the selected setting

Running window
  Ctrl-C            stop; again, kill what is left  Enter / q   close when finished
  l                 save the log                 Page up / down   scroll
";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    Dashboard,
    Area(usize),
    Plan,
    History,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    Sidebar,
    Main,
    Search,
}

enum Popup {
    Text(TextPopup),
    Confirm(ConfirmPopup),
    Running(Box<RunningView>),
}

#[derive(Clone)]
enum Row {
    Up,
    Group { index: usize, name: String },
    Entry(Box<Entry>),
}

struct MenuItem {
    label: String,
    screen: Screen,
    section: &'static str,
}

/// Where the user was in one catalog area.
#[derive(Clone, Default)]
struct AreaState {
    path: Vec<(usize, usize)>,
    selected: usize,
}

const QUICK_ACTIONS: [(&str, &str); 4] = [
    ("Browse Applications", "Search and install tools"),
    ("Review the Install Plan", "See what is queued and run it"),
    ("Run System Doctor", "Reports about this machine"),
    ("View History", "Past runs and their logs"),
];

pub struct App {
    catalog: Catalog,
    platform: LinuxInfo,
    theme: Theme,
    focus: Focus,
    screen: Screen,
    menu: Vec<MenuItem>,
    menu_state: ListState,
    main_state: ListState,
    areas: Vec<AreaState>,
    rows: Vec<Row>,
    search: Vec<char>,
    cursor: usize,
    completion: Option<String>,
    queue: Vec<Entry>,
    pending: Option<Plan>,
    popup: Option<Popup>,
    records: Vec<history::Record>,
    overview: Overview,
    size_bypass: bool,
    skip_confirmation: bool,
    show_hidden: bool,
}

impl App {
    pub fn new(catalog: Catalog, platform: LinuxInfo, args: &TuiArgs) -> Self {
        let menu = build_menu(&catalog);
        let areas = vec![AreaState::default(); catalog.tabs.len()];
        let overview = Overview::gather(&platform);
        let mut app = Self {
            catalog,
            platform,
            theme: args.theme,
            focus: Focus::Sidebar,
            screen: Screen::Dashboard,
            menu,
            menu_state: ListState::default().with_selected(Some(0)),
            main_state: ListState::default().with_selected(Some(0)),
            areas,
            rows: Vec::new(),
            search: Vec::new(),
            cursor: 0,
            completion: None,
            queue: Vec::new(),
            pending: None,
            popup: None,
            records: Vec::new(),
            overview,
            size_bypass: args.size_bypass,
            skip_confirmation: args.skip_confirmation,
            show_hidden: args.override_validation,
        };
        app.update_rows();
        if !args.bypass_root && nix::unistd::Uid::effective().is_root() {
            app.popup = Some(Popup::Text(TextPopup::new(
                "Root user warning",
                ROOT_WARNING,
                true,
            )));
        }
        app
    }

    // ----- state helpers -----

    fn current_area(&self) -> Option<usize> {
        match self.screen {
            Screen::Area(index) => Some(index),
            _ => None,
        }
    }

    fn area_mut(&mut self) -> Option<&mut AreaState> {
        self.current_area().map(|index| &mut self.areas[index])
    }

    #[cfg(test)]
    fn at_root(&self) -> bool {
        self.current_area()
            .is_none_or(|index| self.areas[index].path.is_empty())
    }

    fn searching(&self) -> bool {
        !self.search.is_empty()
    }

    fn is_queued(&self, entry: &Entry) -> bool {
        self.queue.iter().any(|other| other.id == entry.id)
    }

    /// The visible rows of an area screen: the current group's items, or
    /// every entry matching the search across all areas.
    fn update_rows(&mut self) {
        let query = self.search.iter().collect::<String>().to_lowercase();
        self.rows = if !query.is_empty() {
            let mut found: Vec<Entry> = self
                .catalog
                .entries()
                .into_iter()
                .filter(|entry| entry.name.to_lowercase().contains(&query))
                .cloned()
                .collect();
            found.sort_by(|a, b| a.name.cmp(&b.name));
            found
                .into_iter()
                .map(|entry| Row::Entry(Box::new(entry)))
                .collect()
        } else if let Some(index) = self.current_area() {
            let area = &self.areas[index];
            let path: Vec<usize> = area.path.iter().map(|(group, _)| *group).collect();
            let mut rows = Vec::new();
            if !area.path.is_empty() {
                rows.push(Row::Up);
            }
            let items = self.catalog.tabs[index].items_at(&path).unwrap_or(&[]);
            rows.extend(items.iter().enumerate().map(|(index, item)| match item {
                Item::Group(group) => Row::Group {
                    index,
                    name: group.name.clone(),
                },
                Item::Entry(entry) => Row::Entry(Box::new(entry.clone())),
            }));
            rows
        } else {
            Vec::new()
        };
        self.completion = if query.is_empty() {
            None
        } else {
            self.rows.iter().find_map(|row| match row {
                Row::Entry(entry) => entry
                    .name
                    .to_lowercase()
                    .strip_prefix(query.as_str())
                    .map(str::to_string),
                _ => None,
            })
        };
        self.clamp_main();
    }

    fn main_len(&self) -> usize {
        match self.screen {
            Screen::Dashboard => QUICK_ACTIONS.len(),
            Screen::Area(_) => self.rows.len(),
            Screen::Plan => self.queue.len(),
            Screen::History => self.records.len(),
            Screen::Settings => 4,
        }
    }

    fn clamp_main(&mut self) {
        let count = self.main_len();
        let selected = self.main_state.selected().unwrap_or(0);
        self.main_state.select(Some(if count == 0 {
            0
        } else {
            selected.min(count - 1)
        }));
    }

    fn selected_row(&self) -> Option<&Row> {
        self.rows.get(self.main_state.selected()?)
    }

    fn selected_entry(&self) -> Option<&Entry> {
        match self.selected_row() {
            Some(Row::Entry(entry)) => Some(entry.as_ref()),
            _ => None,
        }
    }

    fn open_screen(&mut self, screen: Screen) {
        // Remember the row in the area being left.
        if let Some(index) = self.current_area() {
            self.areas[index].selected = self.main_state.selected().unwrap_or(0);
        }
        self.screen = screen;
        self.search.clear();
        self.cursor = 0;
        if let Some(position) = self.menu.iter().position(|item| item.screen == screen) {
            self.menu_state.select(Some(position));
        }
        let selected = match screen {
            Screen::Area(index) => self.areas[index].selected,
            _ => 0,
        };
        self.main_state.select(Some(selected));
        if screen == Screen::History {
            self.reload_history();
        }
        self.update_rows();
    }

    fn reload_history(&mut self) {
        self.records = history::read(200).unwrap_or_default();
        self.clamp_main();
    }

    /// Whether something changed without a key press: a finished command.
    pub fn needs_redraw(&mut self) -> bool {
        match &mut self.popup {
            Some(Popup::Running(running)) => running.needs_redraw(),
            _ => false,
        }
    }

    // ----- drawing -----

    pub fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let too_small = (area.width < MIN_WIDTH || area.height < MIN_HEIGHT)
            && !self.size_bypass
            && self.popup.is_none();
        if too_small {
            let warning = Paragraph::new(format!(
                "The terminal is {} by {}; HelixForge needs {} by {}.\nEnlarge the window, or start with --size-bypass.",
                area.width, area.height, MIN_WIDTH, MIN_HEIGHT
            ))
            .alignment(Alignment::Center)
            .style(Style::default().fg(self.theme.failure()).bold())
            .wrap(Wrap { trim: true });
            let rows = Layout::vertical([
                Constraint::Fill(1),
                Constraint::Length(4),
                Constraint::Fill(1),
            ])
            .split(area);
            frame.render_widget(warning, rows[1]);
            return;
        }
        let rows = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);
        self.draw_header(frame, rows[0]);
        let columns = Layout::horizontal([Constraint::Length(SIDEBAR_WIDTH), Constraint::Fill(1)])
            .split(rows[1]);
        self.draw_sidebar(frame, columns[0]);
        let main = columns[1];
        match self.screen {
            Screen::Dashboard => self.draw_dashboard(frame, main),
            Screen::Area(_) => self.draw_area(frame, main),
            Screen::Plan => self.draw_plan(frame, main),
            Screen::History => self.draw_history(frame, main),
            Screen::Settings => self.draw_settings(frame, main),
        }
        match &mut self.popup {
            Some(Popup::Text(popup)) => popup.draw(frame, centered(main, 95, 95), self.theme),
            Some(Popup::Confirm(popup)) => popup.draw(frame, centered(main, 80, 70), self.theme),
            Some(Popup::Running(popup)) => popup.draw(frame, centered(main, 96, 96), self.theme),
            None => {}
        }
        self.draw_footer(frame, rows[2]);
    }

    fn draw_header(&self, frame: &mut Frame, area: Rect) {
        let status = match &self.popup {
            Some(Popup::Running(running)) if !running.is_finished() => {
                ("● RUNNING", self.theme.title())
            }
            _ => ("● SYSTEM READY", self.theme.success()),
        };
        let block = Block::default()
            .padding(Padding::horizontal(2))
            .style(Style::default());
        let inner = block.inner(area);
        let left = Text::from(vec![
            Line::from(vec![
                Span::styled("⬡ ", Style::default().fg(self.theme.focused())),
                Span::styled("HELIX", Style::default().fg(self.theme.focused()).bold()),
                Span::styled("FORGE", Style::default().fg(self.theme.title()).bold()),
            ]),
            Line::from(Span::styled(
                "BUILD • CONFIGURE • OPTIMIZE",
                Style::default().fg(self.theme.muted()),
            )),
        ]);
        frame.render_widget(Paragraph::new(left), inner);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                status.0,
                Style::default().fg(status.1),
            )))
            .alignment(Alignment::Right),
            inner,
        );
        let rule = Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1);
        frame.render_widget(
            Paragraph::new("─".repeat(area.width as usize))
                .style(Style::default().fg(self.theme.muted())),
            rule,
        );
    }

    fn draw_sidebar(&mut self, frame: &mut Frame, area: Rect) {
        let focused = self.focus == Focus::Sidebar;
        let mut lines: Vec<Line> = Vec::new();
        let mut section = "";
        for item in &self.menu {
            if item.section != section {
                section = item.section;
                if !lines.is_empty() {
                    lines.push(Line::from(""));
                }
                lines.push(Line::styled(
                    format!(" {section}"),
                    Style::default().fg(self.theme.muted()),
                ));
            }
            let current = item.screen == self.screen;
            let style = if current && focused {
                Style::default()
                    .bg(self.theme.focused())
                    .fg(self.theme.on_highlight())
                    .bold()
            } else if current {
                Style::default().fg(self.theme.focused()).bold()
            } else {
                Style::default().fg(self.theme.entry())
            };
            let marker = if current { HIGHLIGHT } else { "  " };
            lines.push(Line::styled(
                format!(
                    " {marker}{:<width$}",
                    item.label,
                    width = area.width as usize - 4
                ),
                style,
            ));
        }
        let block = Block::default()
            .padding(Padding::new(1, 0, 1, 0))
            .border_set(border::PLAIN)
            .borders(ratatui::widgets::Borders::RIGHT)
            .border_style(Style::default().fg(self.theme.muted()));
        frame.render_widget(Paragraph::new(Text::from(lines)).block(block), area);
    }

    fn screen_block(&self, title: &str, subtitle: &str) -> Block<'static> {
        Block::default()
            .padding(Padding::new(2, 2, 1, 0))
            .title(Line::from(vec![
                Span::styled(
                    format!(" {title} "),
                    Style::default().fg(self.theme.entry()).bold(),
                ),
                Span::styled(
                    subtitle.to_string(),
                    Style::default().fg(self.theme.muted()),
                ),
            ]))
    }

    fn card(&self, title: &str) -> Block<'static> {
        Block::bordered()
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(self.theme.muted()))
            .title(format!(" {title} "))
            .title_style(Style::default().fg(self.theme.entry()).bold())
            .padding(Padding::horizontal(1))
    }

    fn draw_dashboard(&mut self, frame: &mut Frame, area: Rect) {
        let block = self.screen_block("Dashboard", "Welcome to your Linux command center.");
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let overview_height = self.overview.rows.len() as u16 + 2;
        let parts = Layout::vertical([
            Constraint::Length(overview_height),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);
        let width = parts[0].width.saturating_sub(4) as usize;
        let lines: Vec<Line> = self
            .overview
            .rows
            .iter()
            .map(|(label, value)| {
                let gap = width.saturating_sub(label.len() + value.chars().count());
                Line::from(vec![
                    Span::styled(*label, Style::default().fg(self.theme.muted())),
                    Span::raw(" ".repeat(gap)),
                    Span::styled(value.clone(), Style::default().fg(self.theme.entry())),
                ])
            })
            .collect();
        frame.render_widget(
            Paragraph::new(Text::from(lines)).block(self.card("System Overview")),
            parts[0],
        );
        frame.render_widget(
            Paragraph::new(Line::styled(
                "Quick Actions",
                Style::default().fg(self.theme.entry()).bold(),
            )),
            parts[2],
        );
        let focused = self.focus == Focus::Main;
        let items: Vec<Line> = QUICK_ACTIONS
            .iter()
            .map(|(title, detail)| {
                Line::from(vec![
                    Span::styled(*title, Style::default().fg(self.theme.entry()).bold()),
                    Span::styled(
                        format!("   {detail}"),
                        Style::default().fg(self.theme.muted()),
                    ),
                ])
            })
            .collect();
        let list = List::new(items)
            .highlight_style(if focused {
                Style::default()
                    .bg(self.theme.focused())
                    .fg(self.theme.on_highlight())
                    .bold()
            } else {
                Style::default()
            })
            .highlight_symbol(if focused { HIGHLIGHT } else { "  " });
        frame.render_stateful_widget(list, parts[3], &mut self.main_state);
    }

    fn draw_area(&mut self, frame: &mut Frame, area: Rect) {
        let Some(index) = self.current_area() else {
            return;
        };
        let name = self.catalog.tabs[index].name.clone();
        let trail: Vec<String> = {
            let tab = &self.catalog.tabs[index];
            let mut names = Vec::new();
            let mut items = tab.items.as_slice();
            for (group, _) in &self.areas[index].path {
                if let Some(Item::Group(group)) = items.get(*group) {
                    names.push(group.name.clone());
                    items = &group.items;
                }
            }
            names
        };
        let subtitle = if trail.is_empty() {
            format!("{} entries", self.catalog.tabs[index].entries().len())
        } else {
            trail.join(" › ")
        };
        let block = self.screen_block(&name, &subtitle);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let parts = Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).split(inner);
        self.draw_search(frame, parts[0]);
        self.draw_list(frame, parts[1]);
    }

    fn draw_search(&self, frame: &mut Frame, area: Rect) {
        let searching = self.focus == Focus::Search;
        let text = if !searching && self.search.is_empty() {
            Line::from(Span::styled(
                "Type to search every area (/)",
                Style::default().fg(self.theme.muted()),
            ))
        } else {
            let mut spans = vec![Span::styled(
                self.search.iter().collect::<String>(),
                Style::default().fg(self.theme.focused()).bold(),
            )];
            if let Some(completion) = &self.completion {
                spans.push(Span::styled(
                    completion.clone(),
                    Style::default().fg(self.theme.muted()),
                ));
            }
            Line::from(spans)
        };
        let colour = if searching {
            self.theme.focused()
        } else {
            self.theme.muted()
        };
        let block = Block::bordered()
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(colour))
            .padding(Padding::horizontal(1));
        if searching {
            let inner = block.inner(area);
            let x = inner.x + self.search[..self.cursor].len() as u16;
            frame.set_cursor_position((x.min(inner.right().saturating_sub(1)), inner.y));
        }
        frame.render_widget(Paragraph::new(text).block(block), area);
    }

    fn draw_list(&mut self, frame: &mut Frame, area: Rect) {
        let focused = self.focus == Focus::Main;
        let block = self
            .card(if self.searching() {
                "Results"
            } else {
                "Entries"
            })
            .title(Line::from(" FLAGS ").right_aligned());
        let content_width = (block.inner(area).width as usize).saturating_sub(HIGHLIGHT.len());
        let lines: Vec<Line> = self
            .rows
            .iter()
            .map(|row| match row {
                Row::Up => Line::from(format!("{GROUP_ICON}  .."))
                    .style(Style::default().fg(self.theme.group())),
                Row::Group { name, .. } => Line::from(format!("{GROUP_ICON}  {name}"))
                    .style(Style::default().fg(self.theme.group())),
                Row::Entry(entry) => {
                    let queued = self.is_queued(entry);
                    let style = if queued {
                        Style::default().fg(self.theme.focused()).bold()
                    } else {
                        Style::default().fg(self.theme.entry())
                    };
                    let left = format!(
                        "{ENTRY_ICON}  {} {}",
                        entry.name,
                        if queued { QUEUED_ICON } else { "" }
                    );
                    let right = format!("{} ", entry.flags);
                    let gap = " "
                        .repeat(content_width.saturating_sub(left.chars().count() + right.len()));
                    Line::from(format!("{left}{gap}{right}")).style(style)
                }
            })
            .collect();
        let highlight = if focused {
            Style::default()
                .bg(self.theme.focused())
                .fg(self.theme.on_highlight())
                .bold()
        } else {
            Style::default()
        };
        let list = List::new(lines)
            .block(block)
            .highlight_style(highlight)
            .highlight_symbol(if focused { HIGHLIGHT } else { "  " })
            .scroll_padding(1);
        frame.render_stateful_widget(list, area, &mut self.main_state);
    }

    fn draw_plan(&mut self, frame: &mut Frame, area: Rect) {
        let block = self.screen_block("Install Plan", "Review planned changes before they run.");
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if self.queue.is_empty() {
            frame.render_widget(
                Paragraph::new(
                    "Nothing is queued. In an area, press Space on an entry to add it here.",
                )
                .style(Style::default().fg(self.theme.muted()))
                .wrap(Wrap { trim: true }),
                inner,
            );
            return;
        }
        let planned = plan(self.queue.iter(), &self.platform, &|id| {
            self.catalog.find(id)
        });
        let summary = match &planned {
            Ok(planned) => {
                let mut lines = vec![format!(
                    "{} step{}, risk {}{}",
                    planned.steps.len(),
                    if planned.steps.len() == 1 { "" } else { "s" },
                    planned.risk.label(),
                    if planned.privileged {
                        ", asks for your password"
                    } else {
                        ""
                    }
                )];
                if !planned.requirements.is_empty() {
                    lines.push(format!("Needs: {}", planned.requirements.join(", ")));
                }
                lines.extend(planned.notes.iter().cloned());
                lines.extend(
                    planned
                        .blocked
                        .iter()
                        .map(|blocked| format!("Not run: {blocked}")),
                );
                lines
            }
            Err(error) => vec![format!("Cannot plan: {error}")],
        };
        let summary_height = summary.len() as u16 + 2;
        let parts =
            Layout::vertical([Constraint::Length(summary_height), Constraint::Min(1)]).split(inner);
        frame.render_widget(
            Paragraph::new(Text::from(
                summary.into_iter().map(Line::from).collect::<Vec<_>>(),
            ))
            .block(self.card("Summary"))
            .wrap(Wrap { trim: true }),
            parts[0],
        );
        let focused = self.focus == Focus::Main;
        let steps = planned.map(|planned| planned.steps).unwrap_or_default();
        let items: Vec<Line> = self
            .queue
            .iter()
            .map(|entry| {
                let command = steps
                    .iter()
                    .find(|step| step.id() == entry.id)
                    .map(|step| step.display())
                    .unwrap_or_else(|| "nothing to do".to_string());
                Line::from(vec![
                    Span::styled(
                        entry.name.clone(),
                        Style::default().fg(self.theme.entry()).bold(),
                    ),
                    Span::styled(
                        format!("   {command}"),
                        Style::default().fg(self.theme.muted()),
                    ),
                ])
            })
            .collect();
        let list = List::new(items)
            .block(self.card("Queued entries"))
            .highlight_style(if focused {
                Style::default()
                    .bg(self.theme.focused())
                    .fg(self.theme.on_highlight())
                    .bold()
            } else {
                Style::default()
            })
            .highlight_symbol(if focused { HIGHLIGHT } else { "  " });
        frame.render_stateful_widget(list, parts[1], &mut self.main_state);
    }

    fn draw_history(&mut self, frame: &mut Frame, area: Rect) {
        let block = self.screen_block("History", "Past runs, newest first.");
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if self.records.is_empty() {
            frame.render_widget(
                Paragraph::new("No runs recorded yet.")
                    .style(Style::default().fg(self.theme.muted())),
                inner,
            );
            return;
        }
        let focused = self.focus == Focus::Main;
        let items: Vec<Line> = self
            .records
            .iter()
            .map(|record| {
                let (status, colour) = if record.success {
                    ("ok", self.theme.success())
                } else {
                    ("failed", self.theme.failure())
                };
                Line::from(vec![
                    Span::styled(format!("{:<7}", status), Style::default().fg(colour).bold()),
                    Span::styled(
                        format!("{:<22}", timestamp(record.unix_timestamp)),
                        Style::default().fg(self.theme.muted()),
                    ),
                    Span::styled(
                        record.names.join(", "),
                        Style::default().fg(self.theme.entry()),
                    ),
                    Span::styled(
                        format!("   {}", record.result),
                        Style::default().fg(self.theme.muted()),
                    ),
                    Span::styled(
                        if record.log.is_some() { "   log" } else { "" },
                        Style::default().fg(self.theme.focused()),
                    ),
                ])
            })
            .collect();
        let list = List::new(items)
            .block(self.card("Runs"))
            .highlight_style(if focused {
                Style::default()
                    .bg(self.theme.focused())
                    .fg(self.theme.on_highlight())
                    .bold()
            } else {
                Style::default()
            })
            .highlight_symbol(if focused { HIGHLIGHT } else { "  " });
        frame.render_stateful_widget(list, inner, &mut self.main_state);
    }

    fn settings(&self) -> Vec<(String, String)> {
        vec![
            (
                "Theme".to_string(),
                match self.theme {
                    Theme::Default => "default".to_string(),
                    Theme::Compatible => "compatible".to_string(),
                },
            ),
            (
                "Ask before running".to_string(),
                if self.skip_confirmation {
                    "no".to_string()
                } else {
                    "yes".to_string()
                },
            ),
            (
                "Show entries hidden on this machine".to_string(),
                if self.show_hidden {
                    "yes".to_string()
                } else {
                    "no".to_string()
                },
            ),
            (
                "Log level (HELIXFORGE_LOG)".to_string(),
                std::env::var("HELIXFORGE_LOG").unwrap_or_else(|_| "info".to_string()),
            ),
        ]
    }

    fn draw_settings(&mut self, frame: &mut Frame, area: Rect) {
        let block = self.screen_block("Settings", "Enter changes the selected setting.");
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let focused = self.focus == Focus::Main;
        let width = inner.width.saturating_sub(6) as usize;
        let items: Vec<Line> = self
            .settings()
            .into_iter()
            .map(|(label, value)| {
                let gap = width.saturating_sub(label.len() + value.len());
                Line::from(vec![
                    Span::styled(label, Style::default().fg(self.theme.entry())),
                    Span::raw(" ".repeat(gap)),
                    Span::styled(value, Style::default().fg(self.theme.focused()).bold()),
                ])
            })
            .collect();
        let list = List::new(items)
            .block(self.card("Settings"))
            .highlight_style(if focused {
                Style::default()
                    .bg(self.theme.focused())
                    .fg(self.theme.on_highlight())
                    .bold()
            } else {
                Style::default()
            })
            .highlight_symbol(if focused { HIGHLIGHT } else { "  " });
        frame.render_stateful_widget(list, inner, &mut self.main_state);
    }

    fn draw_footer(&self, frame: &mut Frame, area: Rect) {
        let (_, hints) = self.hints();
        let mut spans: Vec<Span> = vec![Span::raw(" ")];
        for (index, hint) in hints.iter().enumerate() {
            if index > 0 {
                spans.push(Span::styled(" · ", Style::default().fg(self.theme.muted())));
            }
            spans.push(Span::styled(
                hint.keys().join("/"),
                Style::default().fg(self.theme.entry()).bold(),
            ));
            spans.push(Span::styled(
                format!(" {}", hint.text()),
                Style::default().fg(self.theme.muted()),
            ));
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    fn hints(&self) -> (&'static str, Vec<Hint>) {
        match &self.popup {
            Some(Popup::Text(popup)) => return popup.hints(),
            Some(Popup::Confirm(popup)) => return popup.hints(),
            Some(Popup::Running(popup)) => return popup.hints(),
            None => {}
        }
        let mut list = Vec::new();
        match self.focus {
            Focus::Search => {
                list.push(Hint::new(&["Enter"], "Keep results"));
                list.push(Hint::new(&["Esc"], "Clear"));
                return ("Search", list);
            }
            Focus::Sidebar => {
                list.push(Hint::new(&["↑↓"], "Navigate"));
                list.push(Hint::new(&["Enter"], "Open"));
            }
            Focus::Main => match self.screen {
                Screen::Dashboard => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    list.push(Hint::new(&["Enter"], "Select"));
                }
                Screen::Area(_) => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    match self.selected_row() {
                        Some(Row::Up) => list.push(Hint::new(&["Enter", "h"], "Parent")),
                        Some(Row::Group { .. }) => list.push(Hint::new(&["Enter"], "Open")),
                        Some(Row::Entry(entry)) => {
                            list.push(Hint::new(&["Enter"], "Run"));
                            if removable(entry) {
                                list.push(Hint::new(&["r"], "Remove"));
                            }
                            list.push(Hint::new(&["Space"], "Plan"));
                            list.push(Hint::new(&["d"], "Describe"));
                            list.push(Hint::new(&["p"], "Preview"));
                        }
                        None => {}
                    }
                }
                Screen::Plan => {
                    list.push(Hint::new(&["Enter"], "Run plan"));
                    list.push(Hint::new(&["x"], "Remove"));
                    list.push(Hint::new(&["c"], "Clear"));
                }
                Screen::History => {
                    list.push(Hint::new(&["Enter"], "Show log"));
                    list.push(Hint::new(&["r"], "Reload"));
                }
                Screen::Settings => list.push(Hint::new(&["Enter"], "Change")),
            },
        }
        list.push(Hint::new(&["/"], "Search"));
        list.push(Hint::new(&["?"], "Help"));
        list.push(Hint::new(&["q"], "Quit"));
        ("HelixForge", list)
    }

    // ----- keys -----

    /// Returns false when HelixForge should exit.
    pub fn handle_key(&mut self, key: &KeyEvent) -> bool {
        let control_c =
            key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c');
        match &mut self.popup {
            Some(Popup::Text(popup)) => {
                if popup.handle_key(key) {
                    self.popup = None;
                }
                return true;
            }
            Some(Popup::Confirm(popup)) => {
                if control_c {
                    return false;
                }
                popup.handle_key(key);
                match popup.decision {
                    Decision::Run => {
                        self.popup = None;
                        self.run_pending();
                    }
                    Decision::Cancel => {
                        self.popup = None;
                        self.pending = None;
                    }
                    Decision::Pending => {}
                }
                return true;
            }
            Some(Popup::Running(popup)) => {
                if popup.handle_key(key) {
                    self.popup = popup
                        .take_problem()
                        .map(|problem| Popup::Text(TextPopup::new("Not recorded", problem, true)));
                    if self.screen == Screen::History {
                        self.reload_history();
                    }
                }
                return true;
            }
            None => {}
        }
        if self.focus == Focus::Search {
            self.handle_search_key(key);
            return true;
        }
        if control_c || key.code == KeyCode::Char('q') {
            return false;
        }
        match key.code {
            KeyCode::Tab => self.next_menu(1),
            KeyCode::BackTab => self.next_menu(-1),
            KeyCode::Char('/') => {
                if self.current_area().is_none() {
                    self.open_screen(Screen::Area(self.first_area()));
                }
                self.focus = Focus::Search;
                self.cursor = self.search.len();
            }
            KeyCode::Char('?') => {
                self.popup = Some(Popup::Text(TextPopup::new("Keys", HELP, false)));
            }
            KeyCode::Char('t') | KeyCode::Char('T') => self.theme = self.theme.next(),
            KeyCode::Char('g') | KeyCode::Char('G') if matches!(self.screen, Screen::Area(_)) => {
                self.popup = Some(Popup::Text(TextPopup::new("Flags", FLAGS_GUIDE, true)));
            }
            _ => match self.focus {
                Focus::Sidebar => self.handle_sidebar_key(key),
                Focus::Main => self.handle_main_key(key),
                Focus::Search => {}
            },
        }
        true
    }

    fn first_area(&self) -> usize {
        self.menu
            .iter()
            .find_map(|item| match item.screen {
                Screen::Area(index) => Some(index),
                _ => None,
            })
            .unwrap_or(0)
    }

    fn handle_sidebar_key(&mut self, key: &KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.next_menu(1),
            KeyCode::Char('k') | KeyCode::Up => self.next_menu(-1),
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => self.focus = Focus::Main,
            _ => {}
        }
    }

    fn next_menu(&mut self, step: isize) {
        let count = self.menu.len() as isize;
        let current = self.menu_state.selected().unwrap_or(0) as isize;
        let next = ((current + step).rem_euclid(count)) as usize;
        let screen = self.menu[next].screen;
        self.open_screen(screen);
    }

    fn handle_main_key(&mut self, key: &KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            _ => {}
        }
        match self.screen {
            Screen::Dashboard => match key.code {
                KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                    let screen = match self.main_state.selected().unwrap_or(0) {
                        0 => Screen::Area(self.first_area()),
                        1 => Screen::Plan,
                        2 => self
                            .catalog
                            .tabs
                            .iter()
                            .position(|tab| tab.id == "doctor")
                            .map(Screen::Area)
                            .unwrap_or(Screen::Dashboard),
                        _ => Screen::History,
                    };
                    self.open_screen(screen);
                }
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Area(_) => match key.code {
                KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => self.enter(),
                KeyCode::Char('h') | KeyCode::Left => self.back(),
                KeyCode::Char('p') | KeyCode::Char('P') => self.preview(),
                KeyCode::Char('d') | KeyCode::Char('D') => self.describe(),
                KeyCode::Char(' ') => self.toggle_queue(),
                KeyCode::Char('r') | KeyCode::Char('R') => {
                    if let Some(entry) = self.selected_entry().cloned()
                        && removable(&entry)
                    {
                        let planned = plan_removal([&entry], &self.platform);
                        self.confirm_plan(planned);
                    }
                }
                _ => {}
            },
            Screen::Plan => match key.code {
                KeyCode::Enter => self.confirm(self.queue.clone()),
                KeyCode::Char('x') | KeyCode::Delete => {
                    if let Some(selected) = self.main_state.selected()
                        && selected < self.queue.len()
                    {
                        self.queue.remove(selected);
                        self.clamp_main();
                    }
                }
                KeyCode::Char('c') => {
                    self.queue.clear();
                    self.clamp_main();
                }
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::History => match key.code {
                KeyCode::Enter => self.show_log(),
                KeyCode::Char('r') => self.reload_history(),
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Settings => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('l') | KeyCode::Right => {
                    self.change_setting()
                }
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
        }
    }

    fn handle_search_key(&mut self, key: &KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                self.focus = Focus::Main;
                self.main_state.select(Some(0));
            }
            KeyCode::Esc => {
                self.search.clear();
                self.cursor = 0;
                self.focus = Focus::Main;
                self.update_rows();
                self.main_state.select(Some(0));
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search.clear();
                self.cursor = 0;
                self.focus = Focus::Main;
                self.update_rows();
            }
            KeyCode::Tab => {
                if let Some(completion) = self.completion.take() {
                    self.search.extend(completion.chars());
                    self.cursor = self.search.len();
                    self.update_rows();
                }
            }
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.search.remove(self.cursor);
                    self.update_rows();
                }
            }
            KeyCode::Delete => {
                if self.cursor < self.search.len() {
                    self.search.remove(self.cursor);
                    self.update_rows();
                }
            }
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.search.len()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.search.len(),
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search.insert(self.cursor, character);
                self.cursor += 1;
                self.update_rows();
            }
            _ => {}
        }
    }

    fn move_selection(&mut self, step: isize) {
        let count = self.main_len();
        if count == 0 {
            return;
        }
        let current = self.main_state.selected().unwrap_or(0) as isize;
        self.main_state
            .select(Some((current + step).clamp(0, count as isize - 1) as usize));
    }

    fn enter(&mut self) {
        match self.selected_row().cloned() {
            Some(Row::Up) => self.back(),
            Some(Row::Group { index, .. }) => {
                let row = self.main_state.selected().unwrap_or(0);
                if let Some(area) = self.area_mut() {
                    area.path.push((index, row));
                }
                self.main_state.select(Some(0));
                self.update_rows();
            }
            Some(Row::Entry(entry)) => self.confirm(vec![*entry]),
            None => {}
        }
    }

    fn back(&mut self) {
        if self.searching() {
            self.search.clear();
            self.cursor = 0;
            self.update_rows();
            self.main_state.select(Some(0));
            return;
        }
        let popped = self.area_mut().and_then(|area| area.path.pop());
        match popped {
            Some((_, row)) => {
                self.update_rows();
                self.main_state.select(Some(row));
            }
            None => self.focus = Focus::Sidebar,
        }
    }

    fn toggle_queue(&mut self) {
        let Some(entry) = self.selected_entry().cloned() else {
            return;
        };
        if !entry.multi_select {
            self.popup = Some(Popup::Text(TextPopup::new(
                entry.name.clone(),
                "This entry runs on its own and cannot be queued with others. Press Enter to run it.",
                true,
            )));
            return;
        }
        if let Some(position) = self.queue.iter().position(|other| other.id == entry.id) {
            self.queue.remove(position);
        } else {
            self.queue.push(entry);
        }
    }

    fn preview(&mut self) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        let text = match &entry.action {
            Action::Script { file, .. } => std::fs::read_to_string(file)
                .unwrap_or_else(|error| format!("The script cannot be read: {error}")),
            Action::Command { line } => line.clone(),
            _ => match plan([entry], &self.platform, &|id| self.catalog.find(id)) {
                Ok(planned) => {
                    let mut lines = planned.notes.clone();
                    lines.extend(planned.steps.iter().map(|step| step.display()));
                    lines.join("\n")
                }
                Err(error) => error.to_string(),
            },
        };
        self.popup = Some(Popup::Text(TextPopup::new(
            format!("Preview: {}", entry.name),
            text,
            false,
        )));
    }

    fn describe(&mut self) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        let text = if entry.description.is_empty() {
            "This entry has no description.".to_string()
        } else {
            entry.description.clone()
        };
        self.popup = Some(Popup::Text(TextPopup::new(entry.name.clone(), text, true)));
    }

    fn show_log(&mut self) {
        let Some(record) = self
            .main_state
            .selected()
            .and_then(|index| self.records.get(index))
        else {
            return;
        };
        let title = record.names.join(", ");
        let text = match &record.log {
            Some(path) => std::fs::read(path)
                .map(|bytes| strip_escapes(&String::from_utf8_lossy(&bytes)))
                .unwrap_or_else(|error| format!("The log cannot be read: {error}")),
            None => format!(
                "{}\n\nNo log was saved for this run. Press l in the running window after a run finishes to save one.",
                record.result
            ),
        };
        self.popup = Some(Popup::Text(TextPopup::new(title, text, false)));
    }

    fn change_setting(&mut self) {
        match self.main_state.selected().unwrap_or(0) {
            0 => self.theme = self.theme.next(),
            1 => self.skip_confirmation = !self.skip_confirmation,
            2 => {
                self.show_hidden = !self.show_hidden;
                match Catalog::builtin(!self.show_hidden) {
                    Ok(catalog) => {
                        self.catalog = catalog;
                        self.menu = build_menu(&self.catalog);
                        self.areas = vec![AreaState::default(); self.catalog.tabs.len()];
                        self.queue.clear();
                        self.update_rows();
                    }
                    Err(error) => {
                        self.popup = Some(Popup::Text(TextPopup::new(
                            "Cannot reload the catalog",
                            error.to_string(),
                            true,
                        )));
                    }
                }
            }
            _ => {}
        }
    }

    /// Plan entries; ask before running what can run.
    fn confirm(&mut self, entries: Vec<Entry>) {
        if entries.is_empty() {
            return;
        }
        let planned = plan(entries.iter(), &self.platform, &|id| self.catalog.find(id));
        self.confirm_plan(planned);
    }

    /// Show a plan's confirmation, or why there is nothing to run.
    fn confirm_plan(&mut self, planned: Result<Plan, EngineError>) {
        let planned = match planned {
            Ok(planned) => planned,
            Err(error) => {
                self.popup = Some(Popup::Text(TextPopup::new(
                    "Cannot plan",
                    error.to_string(),
                    true,
                )));
                return;
            }
        };
        if planned.is_empty() {
            let mut lines = planned.notes.clone();
            lines.extend(
                planned
                    .blocked
                    .iter()
                    .map(|blocked| format!("Not run: {blocked}")),
            );
            let text = if lines.is_empty() {
                "Nothing to run.".to_string()
            } else {
                lines.join("\n")
            };
            self.popup = Some(Popup::Text(TextPopup::new("Nothing to run", text, true)));
            return;
        }
        let header = vec![format!(
            "Risk: {}{}",
            planned.risk.label(),
            if planned.privileged {
                " · asks for your password"
            } else {
                ""
            }
        )];
        let mut lines: Vec<(String, String)> = planned
            .steps
            .iter()
            .map(|step| (step.name().to_string(), step.display()))
            .collect();
        for note in &planned.notes {
            lines.push((note.clone(), String::new()));
        }
        for blocked in &planned.blocked {
            lines.push((format!("Not run: {blocked}"), String::new()));
        }
        self.pending = Some(planned);
        if self.skip_confirmation {
            self.run_pending();
            return;
        }
        self.popup = Some(Popup::Confirm(ConfirmPopup::new(&header, &lines)));
    }

    fn run_pending(&mut self) {
        let Some(planned) = self.pending.take() else {
            return;
        };
        let this =
            std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("helixforge"));
        self.popup = Some(
            match RunningView::start(&planned, &self.catalog.lib_dir(), &this) {
                Ok(running) => {
                    // What ran from the plan screen leaves the queue.
                    if self.screen == Screen::Plan {
                        self.queue.clear();
                        self.clamp_main();
                    }
                    Popup::Running(Box::new(running))
                }
                Err(error) => Popup::Text(TextPopup::new("Cannot run", error, true)),
            },
        );
    }
}

fn removable(entry: &Entry) -> bool {
    matches!(
        entry.action,
        Action::Package {
            removable: true,
            ..
        }
    )
}

fn build_menu(catalog: &Catalog) -> Vec<MenuItem> {
    let area = |id: &str| catalog.tabs.iter().position(|tab| tab.id == id);
    let mut menu = vec![MenuItem {
        label: "Dashboard".into(),
        screen: Screen::Dashboard,
        section: "MAIN MENU",
    }];
    if let Some(index) = area("applications") {
        menu.push(MenuItem {
            label: catalog.tabs[index].name.clone(),
            screen: Screen::Area(index),
            section: "MAIN MENU",
        });
    }
    menu.push(MenuItem {
        label: "Install Plan".into(),
        screen: Screen::Plan,
        section: "MAIN MENU",
    });
    for (index, tab) in catalog.tabs.iter().enumerate() {
        if tab.id != "applications" {
            menu.push(MenuItem {
                label: tab.name.clone(),
                screen: Screen::Area(index),
                section: "MORE TOOLS",
            });
        }
    }
    menu.push(MenuItem {
        label: "History".into(),
        screen: Screen::History,
        section: "MORE TOOLS",
    });
    menu.push(MenuItem {
        label: "Settings".into(),
        screen: Screen::Settings,
        section: "MORE TOOLS",
    });
    menu
}

/// A local date and time without pulling in a time library: UTC is fine for a list.
fn timestamp(unix: u64) -> String {
    let days = unix / 86_400;
    let seconds = unix % 86_400;
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        seconds / 3600,
        (seconds % 3600) / 60
    )
}

/// Terminal output without its escape sequences, for reading a saved log.
fn strip_escapes(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '\u{1b}' {
            match chars.peek() {
                Some('[') => {
                    chars.next();
                    for next in chars.by_ref() {
                        if next.is_ascii_alphabetic() || next == '~' {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next();
                    for next in chars.by_ref() {
                        if next == '\u{7}' {
                            break;
                        }
                    }
                }
                _ => {
                    chars.next();
                }
            }
        } else if character != '\r' {
            result.push(character);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn app() -> App {
        let args = TuiArgs {
            override_validation: true,
            bypass_root: true,
            ..TuiArgs::default()
        };
        let platform =
            LinuxInfo::from_os_release("ID=garuda\nID_LIKE=arch\nPRETTY_NAME=\"Garuda Linux\"\n")
                .unwrap();
        App::new(Catalog::builtin(false).unwrap(), platform, &args)
    }

    fn screen(app: &mut App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    + "\n"
            })
            .collect()
    }

    fn press(app: &mut App, code: KeyCode) -> bool {
        app.handle_key(&KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn type_text(app: &mut App, text: &str) {
        for character in text.chars() {
            press(app, KeyCode::Char(character));
        }
    }

    #[test]
    fn the_dashboard_is_the_first_screen() {
        let mut app = app();
        let text = screen(&mut app, 120, 40);
        assert!(text.contains("HELIXFORGE"));
        assert!(text.contains("SYSTEM READY"));
        assert!(text.contains("MAIN MENU"));
        assert!(text.contains("MORE TOOLS"));
        assert!(text.contains("Dashboard"));
        assert!(text.contains("System Overview"));
        assert!(text.contains("Garuda Linux"));
        assert!(text.contains("Quick Actions"));
        assert!(text.contains("Browse Applications"));
        assert!(text.contains("Navigate"));
    }

    #[test]
    fn a_small_terminal_gets_a_warning_unless_bypassed() {
        let mut app = app();
        assert!(screen(&mut app, 60, 20).contains("needs 100 by 25"));
        app.size_bypass = true;
        assert!(screen(&mut app, 60, 20).contains("HELIXFORGE"));
    }

    #[test]
    fn the_sidebar_opens_areas_and_groups_remember_their_row() {
        let mut app = app();
        press(&mut app, KeyCode::Char('j'));
        assert_eq!(app.screen, Screen::Area(0));
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.focus, Focus::Main);
        let text = screen(&mut app, 120, 40);
        assert!(text.contains("[D]  Developer Tools"));
        assert!(text.contains("FLAGS"));
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.areas[0].path.len(), 1);
        assert!(screen(&mut app, 120, 40).contains("[D]  .."));
        press(&mut app, KeyCode::Char('h'));
        assert!(app.at_root());
        assert_eq!(app.main_state.selected(), Some(1));
        press(&mut app, KeyCode::Char('h'));
        assert_eq!(app.focus, Focus::Sidebar);
    }

    #[test]
    fn quick_actions_open_screens() {
        let mut app = app();
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Enter);
        assert!(
            matches!(app.screen, Screen::Area(index) if app.catalog.tabs[index].id == "doctor")
        );
        assert!(screen(&mut app, 120, 40).contains("Machine Summary"));
    }

    #[test]
    fn search_spans_every_area_and_escape_clears_it() {
        let mut app = app();
        press(&mut app, KeyCode::Char('/'));
        assert!(matches!(app.screen, Screen::Area(_)));
        type_text(&mut app, "git");
        assert!(!app.rows.is_empty());
        assert!(app.rows.iter().all(
            |row| matches!(row, Row::Entry(entry) if entry.name.to_lowercase().contains("git"))
        ));
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.focus, Focus::Main);
        press(&mut app, KeyCode::Char('/'));
        press(&mut app, KeyCode::Esc);
        assert!(app.search.is_empty());
        assert!(app.rows.iter().any(|row| matches!(row, Row::Group { .. })));
    }

    #[test]
    fn entries_queue_into_the_install_plan_and_run_from_it() {
        let mut app = app();
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "System Report (script)");
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char(' '));
        assert!(
            matches!(app.popup, Some(Popup::Text(_))),
            "a solo entry explains it cannot be queued"
        );
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('h'));
        type_text(&mut app, "/Machine Summary");
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char(' '));
        assert_eq!(app.queue.len(), 1);
        assert!(screen(&mut app, 120, 40).contains("Machine Summary *"));
        app.open_screen(Screen::Plan);
        app.focus = Focus::Main;
        let text = screen(&mut app, 120, 40);
        assert!(text.contains("Install Plan"));
        assert!(text.contains("1 step, risk low"));
        assert!(text.contains("helixforge native system-report"));
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        assert!(screen(&mut app, 120, 40).contains("Risk: low"));
        press(&mut app, KeyCode::Char('n'));
        press(&mut app, KeyCode::Char('x'));
        assert!(app.queue.is_empty());
        assert!(screen(&mut app, 120, 40).contains("Nothing is queued"));
    }

    #[test]
    fn running_an_entry_asks_first_and_quitting_works() {
        let mut app = app();
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "System Report (script)");
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('d'));
        assert!(screen(&mut app, 120, 40).contains("Print the distribution"));
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('p'));
        assert!(screen(&mut app, 120, 40).contains("HELIXFORGE_LIB"));
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        let text = screen(&mut app, 120, 40);
        assert!(text.contains("1. System Report (script)"), "{text}");
        assert!(text.contains("system-report.sh"), "{text}");
        press(&mut app, KeyCode::Char('n'));
        assert!(app.popup.is_none());
        assert!(!press(&mut app, KeyCode::Char('q')));
    }

    #[test]
    fn removable_entries_offer_removal_and_others_do_not() {
        let mut app = app();
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "Tenacity");
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 120, 40).contains("Remove"));
        press(&mut app, KeyCode::Char('r'));
        let text = screen(&mut app, 120, 40);
        assert!(
            text.contains("not installed") || text.contains("Remove Tenacity"),
            "{text}"
        );
        press(&mut app, KeyCode::Esc);
        app.popup = None;
        press(&mut app, KeyCode::Char('h'));
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "Machine Summary");
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('r'));
        assert!(
            app.popup.is_none(),
            "a non-package entry has nothing to remove"
        );
    }

    #[test]
    fn the_history_screen_lists_records() {
        let state = tempfile::tempdir().unwrap();
        let mut app = app();
        history::record_in(
            state.path(),
            &[("git".into(), "Git".into())],
            false,
            "exit 1",
            None,
        )
        .unwrap();
        app.records = history::read_in(state.path(), 10).unwrap();
        app.screen = Screen::History;
        app.focus = Focus::Main;
        app.clamp_main();
        let text = screen(&mut app, 120, 40);
        assert!(text.contains("failed"));
        assert!(text.contains("Git"));
        assert!(text.contains("exit 1"));
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 120, 40).contains("No log was saved"));
    }

    #[test]
    fn settings_change_the_theme_and_the_confirmation() {
        let mut app = app();
        app.open_screen(Screen::Settings);
        app.focus = Focus::Main;
        assert!(screen(&mut app, 120, 40).contains("Ask before running"));
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.theme, Theme::Compatible);
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Enter);
        assert!(app.skip_confirmation);
    }

    #[test]
    fn help_and_flags_guide_open_as_popups() {
        let mut app = app();
        press(&mut app, KeyCode::Char('?'));
        assert!(screen(&mut app, 120, 40).contains("Install Plan"));
        press(&mut app, KeyCode::Esc);
        app.open_screen(Screen::Area(0));
        press(&mut app, KeyCode::Char('g'));
        assert!(screen(&mut app, 120, 40).contains("privileged"));
    }

    #[test]
    fn timestamps_and_escape_stripping_are_sane() {
        assert_eq!(timestamp(0), "1970-01-01 00:00 UTC");
        assert_eq!(timestamp(1_700_000_000), "2023-11-14 22:13 UTC");
        assert_eq!(strip_escapes("a\u{1b}[31mb\u{1b}[0m\r\n"), "ab\n");
    }
}
