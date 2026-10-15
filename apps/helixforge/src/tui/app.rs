//! The screen and its keys.

use super::{
    hints::{self, Hint},
    popup::{ConfirmPopup, Decision, TextPopup, centered},
    running::RunningView,
    sysinfo,
    theme::Theme,
};
use crate::TuiArgs;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use helixforge_catalog::Catalog;
use helixforge_core::{Action, Entry, Item};
use helixforge_engine::{Plan, plan};
use helixforge_platform::LinuxInfo;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Flex, Layout, Rect},
    style::Style,
    symbols::border,
    text::{Line, Span, Text},
    widgets::{Block, List, ListState, Padding, Paragraph, Wrap},
};

const MIN_WIDTH: u16 = 100;
const MIN_HEIGHT: u16 = 25;
const TITLE: &str = " HELIXFORGE ";
const HIGHLIGHT: &str = "> ";
const GROUP_ICON: &str = "[D]";
const ENTRY_ICON: &str = "[*]";
const PICKED_ICON: &str = "*";
const LOGO: [&str; 5] = ["⬡ HELIXFORGE", "", "BUILD", "CONFIGURE", "OPTIMIZE"];
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
";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    TabList,
    List,
    Search,
}

enum Popup {
    Text(TextPopup),
    Confirm(ConfirmPopup),
    Running(RunningView),
}

/// One visible row of the list.
#[derive(Clone)]
enum Row {
    Up,
    Group { index: usize, name: String },
    Entry(Entry),
}

pub struct App {
    catalog: Catalog,
    platform: LinuxInfo,
    theme: Theme,
    focus: Focus,
    current_tab: usize,
    tab_state: ListState,
    tab_width: u16,
    /// The groups entered in the current tab, each with the row that was selected there.
    path: Vec<(usize, usize)>,
    list_state: ListState,
    rows: Vec<Row>,
    search: Vec<char>,
    cursor: usize,
    completion: Option<String>,
    multi_select: bool,
    picked: Vec<Entry>,
    pending: Option<Plan>,
    popup: Option<Popup>,
    size_bypass: bool,
    skip_confirmation: bool,
    info: Vec<(&'static str, String)>,
}

impl App {
    pub fn new(catalog: Catalog, platform: LinuxInfo, args: &TuiArgs) -> Self {
        let tab_width = catalog
            .tabs
            .iter()
            .map(|tab| tab.name.chars().count())
            .max()
            .unwrap_or(0) as u16;
        let info = sysinfo::gather(&platform);
        let mut app = Self {
            catalog,
            platform,
            theme: args.theme,
            focus: Focus::List,
            current_tab: 0,
            tab_state: ListState::default().with_selected(Some(0)),
            tab_width,
            path: Vec::new(),
            list_state: ListState::default().with_selected(Some(0)),
            rows: Vec::new(),
            search: Vec::new(),
            cursor: 0,
            completion: None,
            multi_select: false,
            picked: Vec::new(),
            pending: None,
            popup: None,
            size_bypass: args.size_bypass,
            skip_confirmation: args.skip_confirmation,
            info,
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

    fn group_path(&self) -> Vec<usize> {
        self.path.iter().map(|(index, _)| *index).collect()
    }

    fn at_root(&self) -> bool {
        self.path.is_empty()
    }

    fn searching(&self) -> bool {
        !self.search.is_empty()
    }

    /// The visible rows: the current group's items, or every entry matching
    /// the search across all tabs.
    fn update_rows(&mut self) {
        let query = self.search.iter().collect::<String>().to_lowercase();
        self.rows = if query.is_empty() {
            let mut rows = Vec::new();
            if !self.at_root() {
                rows.push(Row::Up);
            }
            let items = self.catalog.tabs[self.current_tab]
                .items_at(&self.group_path())
                .unwrap_or(&[]);
            rows.extend(items.iter().enumerate().map(|(index, item)| match item {
                Item::Group(group) => Row::Group {
                    index,
                    name: group.name.clone(),
                },
                Item::Entry(entry) => Row::Entry(entry.clone()),
            }));
            rows
        } else {
            let mut found: Vec<Entry> = self
                .catalog
                .entries()
                .into_iter()
                .filter(|entry| entry.name.to_lowercase().contains(&query))
                .cloned()
                .collect();
            found.sort_by(|a, b| a.name.cmp(&b.name));
            found.into_iter().map(Row::Entry).collect()
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
        let count = self.rows.len();
        let selected = self.list_state.selected().unwrap_or(0);
        self.list_state.select(Some(if count == 0 {
            0
        } else {
            selected.min(count - 1)
        }));
    }

    fn selected_row(&self) -> Option<&Row> {
        self.rows.get(self.list_state.selected()?)
    }

    fn selected_entry(&self) -> Option<&Entry> {
        match self.selected_row() {
            Some(Row::Entry(entry)) => Some(entry),
            _ => None,
        }
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

        let (scope, hint_list) = self.hints();
        let hints_block = Block::bordered()
            .title(format!(" {scope} "))
            .border_set(border::PLAIN)
            .border_style(Style::default().fg(self.theme.unfocused()))
            .title_style(Style::default().fg(self.theme.title()).bold())
            .padding(Padding::horizontal(1));
        let hint_lines = hints::lines(&hint_list, hints_block.inner(area).width);
        let hint_height = hint_lines.len() as u16 + 2;
        let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(hint_height)])
            .flex(Flex::Legacy)
            .split(area);
        let columns = Layout::horizontal([
            Constraint::Length(self.tab_width.max(20) + 11),
            Constraint::Fill(1),
        ])
        .split(rows[0]);

        self.draw_sidebar(frame, columns[0]);
        let right = Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).split(columns[1]);
        self.draw_search(frame, right[0]);
        self.draw_list(frame, right[1]);

        match &mut self.popup {
            Some(Popup::Text(popup)) => popup.draw(frame, centered(right[1], 95, 95), self.theme),
            Some(Popup::Confirm(popup)) => {
                popup.draw(frame, centered(right[1], 70, 60), self.theme)
            }
            Some(Popup::Running(popup)) => {
                popup.draw(frame, centered(right[1], 95, 95), self.theme)
            }
            None => {}
        }
        frame.render_widget(
            Paragraph::new(Text::from(hint_lines)).block(hints_block),
            rows[1],
        );
    }

    fn draw_sidebar(&mut self, frame: &mut Frame, area: Rect) {
        let logo_height = (LOGO.len() as u16 + 2).min(area.height.saturating_sub(4));
        let info_height = if self.info.is_empty() {
            0
        } else {
            self.info.len() as u16 + 2
        };
        let show_info = info_height > 0 && area.height > logo_height + info_height + 3;
        let parts = Layout::vertical([
            Constraint::Length(logo_height),
            Constraint::Min(1),
            Constraint::Length(if show_info { info_height } else { 0 }),
        ])
        .split(area);

        let mut logo: Vec<Line> =
            vec![Line::from(LOGO[0]).style(Style::default().fg(self.theme.focused()).bold())];
        logo.extend(
            LOGO[1..]
                .iter()
                .map(|line| Line::from(*line).style(Style::default().fg(self.theme.title()))),
        );
        logo.push(Line::from(""));
        logo.push(
            Line::from(format!("v{}", env!("CARGO_PKG_VERSION")))
                .style(Style::default().fg(self.theme.muted())),
        );
        frame.render_widget(
            Paragraph::new(Text::from(logo)).alignment(Alignment::Center),
            parts[0],
        );

        let focused = self.focus == Focus::TabList;
        let border = Style::default().fg(if focused {
            self.theme.focused()
        } else {
            self.theme.unfocused()
        });
        let highlight = if focused {
            Style::default()
                .bg(self.theme.focused())
                .fg(self.theme.on_highlight())
                .bold()
        } else {
            Style::default().fg(self.theme.unfocused())
        };
        let names: Vec<&str> = self
            .catalog
            .tabs
            .iter()
            .map(|tab| tab.name.as_str())
            .collect();
        let tabs = List::new(names)
            .block(
                Block::bordered()
                    .border_set(border::PLAIN)
                    .border_style(border)
                    .padding(Padding::horizontal(1)),
            )
            .style(Style::default().fg(self.theme.title()))
            .highlight_style(highlight)
            .highlight_symbol(HIGHLIGHT);
        frame.render_stateful_widget(tabs, parts[1], &mut self.tab_state);

        if show_info {
            let block = Block::bordered()
                .border_set(border::PLAIN)
                .border_style(Style::default().fg(self.theme.unfocused()))
                .title(" SYSTEM ")
                .title_style(Style::default().fg(self.theme.title()).bold())
                .padding(Padding::horizontal(1));
            let width = block.inner(parts[2]).width as usize;
            let lines: Vec<Line> = self
                .info
                .iter()
                .map(|(label, value)| {
                    let prefix = format!("{label:>4}: ");
                    let room = width.saturating_sub(prefix.len());
                    let value = if value.chars().count() > room {
                        let mut short: String =
                            value.chars().take(room.saturating_sub(1)).collect();
                        short.push('…');
                        short
                    } else {
                        value.clone()
                    };
                    Line::from(vec![
                        Span::styled(prefix, Style::default().fg(self.theme.title()).bold()),
                        Span::styled(value, Style::default().fg(self.theme.entry())),
                    ])
                })
                .collect();
            frame.render_widget(Paragraph::new(Text::from(lines)).block(block), parts[2]);
        }
    }

    fn draw_search(&self, frame: &mut Frame, area: Rect) {
        let searching = self.focus == Focus::Search;
        let text = if !searching && self.search.is_empty() {
            Line::from(Span::styled(
                "Type to search (/)",
                Style::default().fg(self.theme.unfocused()).dim(),
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
            self.theme.unfocused()
        };
        let block = Block::bordered()
            .border_set(border::PLAIN)
            .border_style(Style::default().fg(colour))
            .title(" SEARCH ")
            .title_style(Style::default().fg(self.theme.title()).bold())
            .padding(Padding::horizontal(1));
        if searching {
            let inner = block.inner(area);
            let x = inner.x + self.search[..self.cursor].len() as u16;
            frame.set_cursor_position((x.min(inner.right().saturating_sub(1)), inner.y));
        }
        frame.render_widget(Paragraph::new(text).block(block), area);
    }

    fn draw_list(&mut self, frame: &mut Frame, area: Rect) {
        let focused = self.focus == Focus::List;
        let dim = if focused {
            Style::default()
        } else {
            Style::default().dim()
        };
        let border = Style::default().fg(if focused {
            self.theme.focused()
        } else {
            self.theme.unfocused()
        });
        let title = if self.multi_select {
            format!("{TITLE}[Multi-select] ")
        } else {
            TITLE.to_string()
        };
        let block = Block::bordered()
            .border_set(border::PLAIN)
            .border_style(border)
            .title(title)
            .title(Line::from(" FLAGS ").right_aligned())
            .padding(Padding::horizontal(1));
        let content_width = (block.inner(area).width as usize).saturating_sub(HIGHLIGHT.len());

        let lines: Vec<Line> = self
            .rows
            .iter()
            .map(|row| match row {
                Row::Up => Line::from(format!("{GROUP_ICON}  .."))
                    .style(Style::default().fg(self.theme.group()))
                    .patch_style(dim),
                Row::Group { name, .. } => Line::from(format!("{GROUP_ICON}  {name}"))
                    .style(Style::default().fg(self.theme.group()))
                    .patch_style(dim),
                Row::Entry(entry) => {
                    let picked = self.picked.iter().any(|other| other.id == entry.id);
                    let style = if picked {
                        Style::default().bold()
                    } else if self.multi_select && !entry.multi_select {
                        Style::default().fg(self.theme.disabled())
                    } else {
                        Style::default()
                    };
                    let left = format!(
                        "{ENTRY_ICON}  {} {}",
                        entry.name,
                        if picked { PICKED_ICON } else { "" }
                    );
                    let right = format!("{} ", entry.flags);
                    let gap = " "
                        .repeat(content_width.saturating_sub(left.chars().count() + right.len()));
                    Line::from(format!("{left}{gap}{right}"))
                        .style(Style::default().fg(self.theme.entry()))
                        .patch_style(style)
                        .patch_style(dim)
                }
            })
            .collect();

        let highlight = if focused {
            Style::default()
                .bg(self.theme.focused())
                .fg(self.theme.on_highlight())
                .bold()
        } else {
            dim
        };
        let list = List::new(lines)
            .block(block)
            .highlight_style(highlight)
            .highlight_symbol(if focused { HIGHLIGHT } else { "  " })
            .scroll_padding(1);
        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn hints(&self) -> (&'static str, Vec<Hint>) {
        match &self.popup {
            Some(Popup::Text(popup)) => return popup.hints(),
            Some(Popup::Confirm(popup)) => return popup.hints(),
            Some(Popup::Running(popup)) => return popup.hints(),
            None => {}
        }
        match self.focus {
            Focus::Search => (
                "Search",
                vec![
                    Hint::new(&["Enter"], "Keep the results"),
                    Hint::new(&["Esc"], "Clear the search"),
                ],
            ),
            Focus::TabList => (
                "Tabs",
                vec![
                    Hint::new(&["q", "Ctrl-c"], "Quit"),
                    Hint::new(&["l", "Right", "Enter"], "Focus the list"),
                    Hint::new(&["k", "Up"], "Tab above"),
                    Hint::new(&["j", "Down"], "Tab below"),
                    Hint::new(&["/"], "Search"),
                    Hint::new(&["t"], "Switch theme"),
                    Hint::new(&["g"], "Flags guide"),
                ],
            ),
            Focus::List => {
                let mut list = vec![Hint::new(&["q", "Ctrl-c"], "Quit")];
                match self.selected_row() {
                    Some(Row::Up) => list.push(Hint::new(
                        &["l", "Right", "Enter", "h", "Left"],
                        "Parent group",
                    )),
                    Some(Row::Group { .. }) => {
                        list.push(Hint::new(
                            &["h", "Left"],
                            if self.at_root() {
                                "Focus the tabs"
                            } else {
                                "Parent group"
                            },
                        ));
                        list.push(Hint::new(&["l", "Right", "Enter"], "Open"));
                    }
                    Some(Row::Entry(_)) => {
                        list.push(Hint::new(
                            &["h", "Left"],
                            if self.at_root() {
                                "Focus the tabs"
                            } else {
                                "Parent group"
                            },
                        ));
                        list.push(Hint::new(&["l", "Right", "Enter"], "Run"));
                        list.push(Hint::new(&["p"], "Preview"));
                        list.push(Hint::new(&["d"], "Description"));
                    }
                    None => {}
                }
                list.push(Hint::new(&["k", "Up"], "Row above"));
                list.push(Hint::new(&["j", "Down"], "Row below"));
                list.push(Hint::new(&["/"], "Search"));
                list.push(Hint::new(&["Tab", "Shift-Tab"], "Next / previous tab"));
                list.push(Hint::new(&["v"], "Multi-select mode"));
                if self.multi_select {
                    list.push(Hint::new(&["Space"], "Pick for the queue"));
                }
                list.push(Hint::new(&["t"], "Switch theme"));
                list.push(Hint::new(&["g"], "Flags guide"));
                ("Entries", list)
            }
        }
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
                    self.popup = None;
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
            KeyCode::Tab => self.next_tab(1),
            KeyCode::BackTab => self.next_tab(-1),
            KeyCode::Char('/') => {
                self.focus = Focus::Search;
                self.cursor = self.search.len();
            }
            KeyCode::Char('t') | KeyCode::Char('T') => self.theme = self.theme.next(),
            KeyCode::Char('g') | KeyCode::Char('G') => {
                self.popup = Some(Popup::Text(TextPopup::new("Flags", FLAGS_GUIDE, true)))
            }
            KeyCode::Char('v') | KeyCode::Char('V') => {
                self.multi_select = !self.multi_select;
                if !self.multi_select {
                    self.picked.clear();
                }
            }
            _ => match self.focus {
                Focus::TabList => match key.code {
                    KeyCode::Char('j') | KeyCode::Down => self.next_tab(1),
                    KeyCode::Char('k') | KeyCode::Up => self.next_tab(-1),
                    KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                        self.focus = Focus::List
                    }
                    _ => {}
                },
                Focus::List => match key.code {
                    KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
                    KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
                    KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => self.enter(),
                    KeyCode::Char('h') | KeyCode::Left => self.back(),
                    KeyCode::Char('p') | KeyCode::Char('P') => self.preview(),
                    KeyCode::Char('d') | KeyCode::Char('D') => self.describe(),
                    KeyCode::Char(' ') if self.multi_select => self.toggle_pick(),
                    _ => {}
                },
                Focus::Search => {}
            },
        }
        true
    }

    fn handle_search_key(&mut self, key: &KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                self.focus = Focus::List;
                self.list_state.select(Some(0));
            }
            KeyCode::Esc | KeyCode::Char('c')
                if key.code == KeyCode::Esc || key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.search.clear();
                self.cursor = 0;
                self.focus = Focus::List;
                self.update_rows();
                self.list_state.select(Some(0));
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

    fn next_tab(&mut self, step: isize) {
        let count = self.catalog.tabs.len() as isize;
        self.current_tab = ((self.current_tab as isize + step).rem_euclid(count)) as usize;
        self.tab_state.select(Some(self.current_tab));
        self.path.clear();
        self.search.clear();
        self.cursor = 0;
        self.list_state.select(Some(0));
        self.update_rows();
    }

    fn move_selection(&mut self, step: isize) {
        let count = self.rows.len();
        if count == 0 {
            return;
        }
        let current = self.list_state.selected().unwrap_or(0) as isize;
        self.list_state
            .select(Some((current + step).clamp(0, count as isize - 1) as usize));
    }

    fn enter(&mut self) {
        match self.selected_row().cloned() {
            Some(Row::Up) => self.back(),
            Some(Row::Group { index, .. }) => {
                let row = self.list_state.selected().unwrap_or(0);
                self.path.push((index, row));
                self.list_state.select(Some(0));
                self.update_rows();
            }
            Some(Row::Entry(entry)) => {
                if self.multi_select {
                    if !self.picked.is_empty() {
                        self.confirm_picked();
                    }
                } else {
                    self.picked = vec![entry];
                    self.confirm_picked();
                }
            }
            None => {}
        }
    }

    fn back(&mut self) {
        if self.searching() {
            self.search.clear();
            self.cursor = 0;
            self.update_rows();
            self.list_state.select(Some(0));
            return;
        }
        match self.path.pop() {
            Some((_, row)) => {
                self.update_rows();
                self.list_state.select(Some(row));
            }
            None => self.focus = Focus::TabList,
        }
    }

    fn toggle_pick(&mut self) {
        let Some(entry) = self.selected_entry().cloned() else {
            return;
        };
        if !entry.multi_select {
            return;
        }
        if let Some(position) = self.picked.iter().position(|other| other.id == entry.id) {
            self.picked.remove(position);
        } else {
            self.picked.push(entry);
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

    /// Plan the picked entries; ask before running what can run.
    fn confirm_picked(&mut self) {
        let picked = std::mem::take(&mut self.picked);
        let planned = match plan(picked.iter(), &self.platform, &|id| self.catalog.find(id)) {
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
            let text = if planned.notes.is_empty() {
                "Nothing to run.".to_string()
            } else {
                planned.notes.join("\n")
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
                Ok(running) => Popup::Running(running),
                Err(error) => Popup::Text(TextPopup::new("Cannot run", error, true)),
            },
        );
    }
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

    #[test]
    fn the_first_screen_shows_tabs_groups_and_hints() {
        let mut app = app();
        let text = screen(&mut app, 120, 40);
        assert!(text.contains("HELIXFORGE"));
        assert!(text.contains("Applications"));
        assert!(text.contains("Helix Doctor"));
        assert!(text.contains("FLAGS"));
        assert!(text.contains("[D]  Developer Tools"));
        assert!(text.contains("Quit"));
    }

    #[test]
    fn a_small_terminal_gets_a_warning_unless_bypassed() {
        let mut app = app();
        assert!(screen(&mut app, 60, 20).contains("needs 100 by 25"));
        app.size_bypass = true;
        assert!(screen(&mut app, 60, 20).contains("HELIXFORGE"));
    }

    #[test]
    fn groups_open_and_close_and_remember_the_row() {
        let mut app = app();
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.path.len(), 1);
        assert!(screen(&mut app, 120, 40).contains("[D]  .."));
        press(&mut app, KeyCode::Char('h'));
        assert!(app.at_root());
        assert_eq!(app.list_state.selected(), Some(1));
    }

    #[test]
    fn search_spans_every_tab_and_escape_clears_it() {
        let mut app = app();
        press(&mut app, KeyCode::Char('/'));
        for character in "git".chars() {
            press(&mut app, KeyCode::Char(character));
        }
        assert!(!app.rows.is_empty());
        assert!(app.rows.iter().all(
            |row| matches!(row, Row::Entry(entry) if entry.name.to_lowercase().contains("git"))
        ));
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.focus, Focus::List);
        press(&mut app, KeyCode::Char('/'));
        press(&mut app, KeyCode::Esc);
        assert!(app.search.is_empty());
        assert!(app.rows.iter().any(|row| matches!(row, Row::Group { .. })));
    }

    #[test]
    fn running_an_entry_asks_first_and_quitting_works() {
        let mut app = app();
        press(&mut app, KeyCode::Char('/'));
        for character in "System Report (script)".chars() {
            press(&mut app, KeyCode::Char(character));
        }
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('d'));
        assert!(matches!(app.popup, Some(Popup::Text(_))));
        assert!(screen(&mut app, 120, 40).contains("Print the distribution"));
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('p'));
        assert!(screen(&mut app, 120, 40).contains("HELIXFORGE_LIB"));
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        let text = screen(&mut app, 120, 40);
        assert!(text.contains("Risk: low"), "{text}");
        assert!(text.contains("1. System Report"), "{text}");
        assert!(text.contains("system-report.sh"), "{text}");
        press(&mut app, KeyCode::Char('n'));
        assert!(app.popup.is_none());
        assert!(!press(&mut app, KeyCode::Char('q')));
    }

    #[test]
    fn the_flags_guide_and_theme_toggle_work_from_the_tab_list() {
        let mut app = app();
        press(&mut app, KeyCode::Char('h'));
        assert_eq!(app.focus, Focus::TabList);
        press(&mut app, KeyCode::Char('t'));
        assert_eq!(app.theme, Theme::Compatible);
        press(&mut app, KeyCode::Char('g'));
        assert!(screen(&mut app, 120, 40).contains("privileged"));
    }
}
