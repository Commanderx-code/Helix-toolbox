//! The screens: a header, a sidebar menu, the current screen and a hint line,
//! after the HelixForge dashboard design.

use super::{
    banner,
    hints::Hint,
    icons::Icon,
    popup::{ConfirmPopup, Decision, TextPopup, centered},
    running::RunningView,
    settings::{self, Look},
    sysinfo::Overview,
    theme::Theme,
};
use crate::TuiArgs;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use helixforge_catalog::Catalog;
use helixforge_core::{Action, Entry, Item};
use helixforge_engine::{
    EngineError, Plan, history,
    inventory::{Inventory, Status, which},
    plan, plan_removal, preflight,
};
use helixforge_platform::LinuxInfo;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Style},
    symbols::border,
    text::{Line, Span, Text},
    widgets::{Block, List, ListState, Padding, Paragraph, Wrap},
};
use ratatui_image::{Image, protocol::Protocol};
use std::{collections::HashMap, sync::mpsc};

const MIN_WIDTH: u16 = 100;
const MIN_HEIGHT: u16 = 25;
const SIDEBAR_WIDTH: u16 = 30;
const HEADER_HEIGHT: u16 = 4;
/// Room kept beside the banner for the status on the right.
const STATUS_WIDTH: u16 = 18;
const HIGHLIGHT: &str = "> ";
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
  Tab / Shift-Tab   next / previous screen       t   next theme (remembered)
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
  u                 update the entry             U           update every entry with an update
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

const QUICK_ICONS: [Icon; 4] = [Icon::Applications, Icon::Plan, Icon::Doctor, Icon::History];

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
    /// Nerd Font icons, or plain text.
    icons: bool,
    /// Where theme and icon choices are saved; none in tests.
    settings_path: Option<std::path::PathBuf>,
    /// The banner picture, when the terminal can show one.
    banner: Option<Protocol>,
    /// The Install Plan screen's plan, kept briefly: planning asks the
    /// package manager about every queued package.
    planned: Option<PlanCache>,
    /// How each package entry stands, by entry id, from the last check.
    statuses: HashMap<String, Status>,
    /// The snapshot that check read, for planning updates.
    inventory: Option<Inventory>,
    /// The check running in the background, if any; off in tests.
    checking: Option<mpsc::Receiver<(Inventory, HashMap<String, Status>)>>,
    check_statuses: bool,
}

/// A plan of the queue as it was, from when it was made, with what the
/// checks before a run found.
struct PlanCache {
    ids: Vec<String>,
    made: std::time::Instant,
    result: Result<Plan, String>,
    findings: Vec<preflight::Finding>,
}

/// How long a cached plan is trusted, for changes made outside HelixForge.
const PLAN_KEPT: std::time::Duration = std::time::Duration::from_secs(3);

impl App {
    pub fn new(catalog: Catalog, platform: LinuxInfo, args: &TuiArgs) -> Self {
        let menu = build_menu(&catalog);
        let areas = vec![AreaState::default(); catalog.tabs.len()];
        let overview = Overview::gather(&platform);
        let mut app = Self {
            catalog,
            platform,
            theme: args.theme.unwrap_or_default(),
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
            icons: !args.plain,
            settings_path: None,
            banner: None,
            planned: None,
            statuses: HashMap::new(),
            inventory: None,
            checking: None,
            check_statuses: false,
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

    /// Use and keep the saved look; `--theme` and `--plain` given on the
    /// command line win for this run.
    pub fn remembering(
        mut self,
        look: Look,
        path: Option<std::path::PathBuf>,
        args: &TuiArgs,
    ) -> Self {
        self.theme = args.theme.unwrap_or(look.theme);
        self.icons = look.icons && !args.plain;
        self.settings_path = path;
        self
    }

    /// Check in the background how package entries stand, now and after
    /// every run.
    pub fn with_status_checks(mut self) -> Self {
        self.check_statuses = true;
        self.refresh_statuses();
        self
    }

    /// Start a fresh status check; the old answers stay shown until it ends.
    fn refresh_statuses(&mut self) {
        if !self.check_statuses {
            return;
        }
        let entries: Vec<Entry> = self
            .catalog
            .entries()
            .into_iter()
            .filter(|entry| entry.identity().is_some())
            .cloned()
            .collect();
        let platform = self.platform.clone();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let inventory = Inventory::gather(&platform);
            let statuses = entries
                .iter()
                .filter_map(|entry| {
                    inventory
                        .status(entry, &platform, &which)
                        .map(|status| (entry.id.clone(), status))
                })
                .collect();
            let _ = sender.send((inventory, statuses));
        });
        self.checking = Some(receiver);
    }

    /// Draw this picture as the header where it fits.
    pub fn with_banner(mut self, banner: Option<Protocol>) -> Self {
        self.banner = banner;
        self
    }

    /// The banner, unless icons are off, the sixteen-colour theme is in use,
    /// or the terminal is too narrow for it and the status beside it.
    fn banner(&self, width: u16) -> Option<&Protocol> {
        self.banner.as_ref().filter(|banner| {
            self.icons
                && self.theme != Theme::Compatible
                && banner.size().width + 2 + STATUS_WIDTH <= width
        })
    }

    /// Save the theme and icon choice, when there is somewhere to save it.
    fn save_look(&mut self) {
        let Some(path) = &self.settings_path else {
            return;
        };
        let look = Look {
            theme: self.theme,
            icons: self.icons,
        };
        if let Err(error) = settings::save(path, &look) {
            tracing::warn!(%error, "the settings were not saved");
        }
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
            Screen::Settings => 5,
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
        if let Some(receiver) = &self.checking {
            match receiver.try_recv() {
                Ok((inventory, statuses)) => {
                    self.inventory = Some(inventory);
                    self.statuses = statuses;
                    self.checking = None;
                    return true;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.checking = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        match &mut self.popup {
            Some(Popup::Running(running)) => running.needs_redraw(),
            _ => false,
        }
    }

    // ----- drawing -----

    fn glyph(&self, icon: Icon) -> &'static str {
        icon.glyph(self.icons)
    }

    /// An icon and the space after it, or nothing when icons are off.
    fn icon_gap(&self, icon: Icon) -> String {
        match self.glyph(icon) {
            "" => String::new(),
            glyph => format!("{glyph}  "),
        }
    }

    fn menu_icon(&self, screen: Screen) -> Icon {
        match screen {
            Screen::Dashboard => Icon::Dashboard,
            Screen::Plan => Icon::Plan,
            Screen::History => Icon::History,
            Screen::Settings => Icon::Settings,
            Screen::Area(index) => Icon::for_area(&self.catalog.tabs[index].id),
        }
    }

    /// The style of the selected row of a list that has the focus.
    fn highlight(&self, focused: bool) -> Style {
        if focused {
            self.theme.selection()
        } else {
            Style::default()
        }
    }

    /// What marks the selected row of a list.
    fn marker(&self, focused: bool) -> &'static str {
        match (focused, self.icons) {
            (false, _) => "  ",
            (true, true) => "▌ ",
            (true, false) => HIGHLIGHT,
        }
    }

    pub fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        frame.render_widget(Block::default().style(self.theme.base()), area);
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
        let header = if self.banner(area.width).is_some() {
            banner::ROWS + 1
        } else {
            HEADER_HEIGHT
        };
        let rows = Layout::vertical([
            Constraint::Length(header),
            Constraint::Min(1),
            Constraint::Length(2),
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
            Some(Popup::Confirm(popup)) => {
                let area = popup.fitted(centered(main, 80, 70));
                popup.draw(frame, area, self.theme)
            }
            Some(Popup::Running(popup)) => popup.draw(frame, centered(main, 96, 96), self.theme),
            None => {}
        }
        self.draw_footer(frame, rows[2]);
    }

    fn draw_header(&self, frame: &mut Frame, area: Rect) {
        let (status, colour) = match &self.popup {
            Some(Popup::Running(running)) if !running.is_finished() => {
                ("● RUNNING", self.theme.title())
            }
            _ => ("● SYSTEM READY", self.theme.success()),
        };
        if let Some(banner) = self.banner(area.width) {
            let size = banner.size();
            frame.render_widget(
                Image::new(banner),
                Rect::new(area.x + 2, area.y, size.width, size.height).intersection(area),
            );
            let middle = Rect::new(
                area.x,
                area.y + size.height.saturating_sub(1) / 2,
                area.width.saturating_sub(2),
                1,
            )
            .intersection(area);
            frame.render_widget(
                Paragraph::new(Line::styled(status, Style::default().fg(colour).bold()))
                    .alignment(Alignment::Right),
                middle,
            );
            self.rule(frame, area.x, area.bottom().saturating_sub(1), area.width);
            return;
        }
        // The logo tile, then the wordmark and the tagline beside it.
        let tile = Rect::new(area.x + 2, area.y, 5, 3).intersection(area);
        let inside = self.framed(
            frame,
            tile,
            self.theme.selected_line(),
            Style::default().bg(self.theme.selected()),
        );
        frame.render_widget(
            Paragraph::new(Line::styled(
                self.glyph(Icon::Logo),
                Style::default().fg(self.theme.focused()).bold(),
            ))
            .alignment(Alignment::Center),
            inside,
        );
        let text = Rect::new(
            area.x + 9,
            area.y,
            area.width.saturating_sub(11),
            area.height.min(3),
        )
        .intersection(area);
        let brand = Text::from(vec![
            Line::from(vec![
                Span::styled("HELIX", Style::default().fg(self.theme.focused()).bold()),
                Span::styled("FORGE", Style::default().fg(self.theme.title()).bold()),
            ]),
            Line::styled(
                "BUILD • CONFIGURE • OPTIMIZE",
                Style::default().fg(self.theme.faint()),
            ),
        ]);
        frame.render_widget(Paragraph::new(brand), text);
        frame.render_widget(
            Paragraph::new(Line::styled(status, Style::default().fg(colour).bold()))
                .alignment(Alignment::Right),
            text,
        );
        self.rule(frame, area.x, area.bottom().saturating_sub(1), area.width);
    }

    /// A rounded card whose fill stays inside its border, as in the design;
    /// returns the space inside.
    fn framed(&self, frame: &mut Frame, area: Rect, border: Color, fill: Style) -> Rect {
        let block = Block::bordered()
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(border));
        let inside = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(Block::default().style(fill), inside);
        inside
    }

    /// A horizontal divider.
    fn rule(&self, frame: &mut Frame, x: u16, y: u16, width: u16) {
        frame.render_widget(
            Paragraph::new("─".repeat(width as usize))
                .style(Style::default().fg(self.theme.line())),
            Rect::new(x, y, width, 1).intersection(frame.area()),
        );
    }

    fn draw_sidebar(&mut self, frame: &mut Frame, area: Rect) {
        let focused = self.focus == Focus::Sidebar;
        let edge = Block::default()
            .borders(ratatui::widgets::Borders::RIGHT)
            .border_set(border::PLAIN)
            .border_style(Style::default().fg(self.theme.line()));
        let inner = edge.inner(area);
        frame.render_widget(edge, area);
        let inner = Rect::new(
            inner.x + 1,
            inner.y + 1,
            inner.width.saturating_sub(2),
            inner.height.saturating_sub(1),
        );
        // Menu items by section, in order.
        let mut sections: Vec<(&'static str, Vec<usize>)> = Vec::new();
        for (index, item) in self.menu.iter().enumerate() {
            match sections.last_mut() {
                Some((name, items)) if *name == item.section => items.push(index),
                _ => sections.push((item.section, vec![index])),
            }
        }
        // The first section's items are cards when there is room for them.
        let compact = sections.len() * 3 + self.menu.len();
        let cards = inner.height as usize >= compact + 2 * sections[0].1.len();
        let bottom = inner.bottom();
        let mut y = inner.y;
        for (number, (name, items)) in sections.iter().enumerate() {
            if number > 0 {
                self.rule(frame, inner.x, y, inner.width);
                y += 2;
            }
            if y >= bottom {
                break;
            }
            frame.render_widget(
                Paragraph::new(Line::styled(
                    *name,
                    Style::default().fg(self.theme.faint()).bold(),
                )),
                Rect::new(inner.x, y, inner.width, 1),
            );
            y += 1;
            for &index in items {
                let item = &self.menu[index];
                let current = item.screen == self.screen;
                let icon = self.icon_gap(self.menu_icon(item.screen));
                let text_style = if current {
                    Style::default().fg(self.theme.on_selected()).bold()
                } else {
                    Style::default().fg(self.theme.entry())
                };
                let label = Line::from(vec![
                    Span::styled(
                        icon,
                        Style::default().fg(if current {
                            self.theme.focused()
                        } else {
                            self.theme.muted()
                        }),
                    ),
                    Span::styled(item.label.clone(), text_style),
                ]);
                if cards && number == 0 {
                    if y + 3 > bottom {
                        break;
                    }
                    let border = match (current, focused) {
                        (true, true) => self.theme.focused(),
                        (true, false) => self.theme.selected_line(),
                        _ => self.theme.line(),
                    };
                    let fill = if current {
                        Style::default().bg(self.theme.selected())
                    } else {
                        self.theme.card()
                    };
                    let inside =
                        self.framed(frame, Rect::new(inner.x, y, inner.width, 3), border, fill);
                    frame.render_widget(
                        Paragraph::new(label)
                            .block(Block::default().padding(Padding::horizontal(1))),
                        inside,
                    );
                    y += 3;
                } else {
                    if y >= bottom {
                        break;
                    }
                    let row = Rect::new(inner.x, y, inner.width, 1);
                    let edge = if current && focused {
                        Span::styled("▌", Style::default().fg(self.theme.focused()))
                    } else {
                        Span::raw(" ")
                    };
                    let mut spans = vec![edge, Span::raw(" ")];
                    spans.extend(label.spans);
                    let style = if current {
                        Style::default().bg(self.theme.selected())
                    } else {
                        Style::default()
                    };
                    frame.render_widget(Paragraph::new(Line::from(spans)).style(style), row);
                    y += 1;
                }
            }
            y += 1;
        }
    }

    /// A screen's title and subtitle; returns the space below them.
    fn page(&self, frame: &mut Frame, area: Rect, title: &str, subtitle: &str) -> Rect {
        let inner = Rect::new(
            area.x + 2,
            area.y + 1,
            area.width.saturating_sub(4),
            area.height.saturating_sub(1),
        );
        frame.render_widget(
            Paragraph::new(Text::from(vec![
                Line::styled(
                    title.to_string(),
                    Style::default().fg(self.theme.entry()).bold(),
                ),
                Line::styled(
                    subtitle.to_string(),
                    Style::default().fg(self.theme.muted()),
                ),
            ])),
            inner,
        );
        Rect::new(
            inner.x,
            inner.y + 3,
            inner.width,
            inner.height.saturating_sub(3),
        )
    }

    fn card(&self, title: &str) -> Block<'static> {
        Block::bordered()
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(self.theme.line()))
            .title(format!(" {title} "))
            .title_style(Style::default().fg(self.theme.entry()).bold())
            .padding(Padding::horizontal(1))
    }

    /// Fill a card's inside, so the fill never reaches past its rounded
    /// border; draw the card's widget over it afterwards.
    fn fill(&self, frame: &mut Frame, area: Rect) {
        let inside = Block::bordered().inner(area);
        frame.render_widget(Block::default().style(self.theme.card()), inside);
    }

    fn draw_dashboard(&mut self, frame: &mut Frame, area: Rect) {
        let content = self.page(
            frame,
            area,
            "Dashboard",
            "Welcome to your Linux command center.",
        );
        let rows = self.overview.rows.len() as u16;
        let actions = QUICK_ACTIONS.len() as u16;
        let overview_height = |spaced: bool| if spaced { rows * 2 + 1 } else { rows + 2 };
        // Roomy rows and two-line cards when they fit, tighter otherwise.
        let (spaced, card) = [(true, 4), (false, 4), (false, 3), (false, 1)]
            .into_iter()
            .find(|&(spaced, card)| overview_height(spaced) + 3 + actions * card <= content.height)
            .unwrap_or((false, 1));
        let width = content.width.saturating_sub(4) as usize;
        let mut lines: Vec<Line> = Vec::new();
        for (index, (label, value)) in self.overview.rows.iter().enumerate() {
            if spaced && index > 0 {
                lines.push(Line::from(""));
            }
            let gap = width.saturating_sub(label.chars().count() + value.chars().count());
            lines.push(Line::from(vec![
                Span::styled(*label, Style::default().fg(self.theme.muted())),
                Span::raw(" ".repeat(gap)),
                Span::styled(value.clone(), Style::default().fg(self.theme.entry())),
            ]));
        }
        let overview = Rect::new(
            content.x,
            content.y,
            content.width,
            overview_height(spaced).min(content.height),
        );
        let mut block = self.card("System Overview");
        let corner = self.glyph(Icon::Overview);
        if !corner.is_empty() {
            block = block.title(
                Line::styled(
                    format!(" {corner} "),
                    Style::default().fg(self.theme.focused()),
                )
                .right_aligned(),
            );
        }
        self.fill(frame, overview);
        frame.render_widget(Paragraph::new(Text::from(lines)).block(block), overview);
        // A blank line before the heading, unless every action would not fit.
        let gap = u16::from(overview_height(spaced) + 3 + actions * card <= content.height);
        let heading = overview.bottom() + gap;
        if heading >= content.bottom() {
            return;
        }
        frame.render_widget(
            Paragraph::new(Line::styled(
                "Quick Actions",
                Style::default().fg(self.theme.entry()).bold(),
            )),
            Rect::new(content.x, heading, content.width, 1),
        );
        let list = Rect::new(
            content.x,
            heading + 1,
            content.width,
            content.bottom().saturating_sub(heading + 1),
        );
        let focused = self.focus == Focus::Main;
        if card == 1 {
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
            let widget = List::new(items)
                .highlight_style(self.highlight(focused))
                .highlight_symbol(self.marker(focused));
            frame.render_stateful_widget(widget, list, &mut self.main_state);
            return;
        }
        let selected = self.main_state.selected().unwrap_or(0);
        for (index, (title, detail)) in QUICK_ACTIONS.iter().enumerate() {
            let rect = Rect::new(list.x, list.y + index as u16 * card, list.width, card);
            if rect.bottom() > list.bottom() {
                break;
            }
            self.action_card(
                frame,
                rect,
                QUICK_ICONS[index],
                title,
                detail,
                index == selected,
                focused,
            );
        }
    }

    /// A quick action as a card: icon, title, detail and a chevron.
    #[allow(clippy::too_many_arguments)]
    fn action_card(
        &self,
        frame: &mut Frame,
        area: Rect,
        icon: Icon,
        title: &str,
        detail: &str,
        selected: bool,
        focused: bool,
    ) {
        let border = match (selected, focused) {
            (true, true) => self.theme.focused(),
            (true, false) => self.theme.selected_line(),
            _ => self.theme.line(),
        };
        let fill = if selected {
            Style::default().bg(self.theme.selected())
        } else {
            self.theme.card()
        };
        let inside = self.framed(frame, area, border, fill);
        let inner = Rect::new(
            inside.x + 1,
            inside.y,
            inside.width.saturating_sub(2),
            inside.height,
        );
        let icon = self.icon_gap(icon);
        let indent = " ".repeat(icon.chars().count());
        let title_style = Style::default()
            .fg(if selected {
                self.theme.on_selected()
            } else {
                self.theme.entry()
            })
            .bold();
        let icon = Span::styled(icon, Style::default().fg(self.theme.focused()));
        let lines = if inner.height >= 2 {
            vec![
                Line::from(vec![icon, Span::styled(title.to_string(), title_style)]),
                Line::from(vec![
                    Span::raw(indent),
                    Span::styled(detail.to_string(), Style::default().fg(self.theme.muted())),
                ]),
            ]
        } else {
            vec![Line::from(vec![
                icon,
                Span::styled(title.to_string(), title_style),
                Span::styled(
                    format!("   {detail}"),
                    Style::default().fg(self.theme.muted()),
                ),
            ])]
        };
        let text = Rect::new(
            inner.x,
            inner.y,
            inner.width.saturating_sub(2),
            inner.height,
        );
        frame.render_widget(Paragraph::new(Text::from(lines)), text);
        frame.render_widget(
            Paragraph::new(Line::styled(
                self.glyph(Icon::Chevron),
                Style::default().fg(if selected {
                    self.theme.focused()
                } else {
                    self.theme.muted()
                }),
            ))
            .alignment(Alignment::Right),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
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
        let content = self.page(frame, area, &name, &subtitle);
        let parts = Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).split(content);
        self.draw_search(frame, parts[0]);
        self.draw_list(frame, parts[1]);
    }

    fn draw_search(&self, frame: &mut Frame, area: Rect) {
        let searching = self.focus == Focus::Search;
        let prefix = self.icon_gap(Icon::Search);
        let mut spans = vec![Span::styled(
            prefix.clone(),
            Style::default().fg(if searching {
                self.theme.focused()
            } else {
                self.theme.muted()
            }),
        )];
        if !searching && self.search.is_empty() {
            spans.push(Span::styled(
                "Type to search every area (/)",
                Style::default().fg(self.theme.muted()),
            ));
        } else {
            spans.push(Span::styled(
                self.search.iter().collect::<String>(),
                Style::default().fg(self.theme.focused()).bold(),
            ));
            if let Some(completion) = &self.completion {
                spans.push(Span::styled(
                    completion.clone(),
                    Style::default().fg(self.theme.muted()),
                ));
            }
        }
        let colour = if searching {
            self.theme.focused()
        } else {
            self.theme.line()
        };
        let block = Block::bordered()
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(colour))
            .padding(Padding::horizontal(1));
        self.fill(frame, area);
        if searching {
            let inner = block.inner(area);
            let x =
                inner.x + prefix.chars().count() as u16 + self.search[..self.cursor].len() as u16;
            frame.set_cursor_position((x.min(inner.right().saturating_sub(1)), inner.y));
        }
        frame.render_widget(Paragraph::new(Line::from(spans)).block(block), area);
    }

    fn draw_list(&mut self, frame: &mut Frame, area: Rect) {
        let focused = self.focus == Focus::Main;
        let block = self
            .card(if self.searching() {
                "Results"
            } else {
                "Entries"
            })
            .title(
                Line::styled(" STATUS · FLAGS ", Style::default().fg(self.theme.muted()))
                    .right_aligned(),
            );
        let marker = self.marker(focused);
        let content_width =
            (block.inner(area).width as usize).saturating_sub(marker.chars().count());
        let group = self.glyph(Icon::Group);
        let lines: Vec<Line> = self
            .rows
            .iter()
            .map(|row| match row {
                Row::Up => Line::from(format!("{group}  .."))
                    .style(Style::default().fg(self.theme.group())),
                Row::Group { name, .. } => Line::from(format!("{group}  {name}"))
                    .style(Style::default().fg(self.theme.group())),
                Row::Entry(entry) => {
                    let queued = self.is_queued(entry);
                    let (style, icon_colour) = if queued {
                        (
                            Style::default().fg(self.theme.focused()).bold(),
                            self.theme.success(),
                        )
                    } else {
                        (Style::default().fg(self.theme.entry()), self.theme.muted())
                    };
                    let icon = if queued {
                        self.glyph(Icon::Queued)
                    } else {
                        self.glyph(Icon::Entry)
                    };
                    // Without icons, a queued entry is marked after its name.
                    let suffix = if queued && !self.icons { " *" } else { "" };
                    let left = format!("{icon}  {}{suffix}", entry.name);
                    let flags = format!("  {} ", entry.flags);
                    // The status word, then where from, dropped first when short of room.
                    let (word, detail, colour) = match self.statuses.get(&entry.id) {
                        Some(status) => (
                            status.label().to_string(),
                            status.detail(),
                            self.status_colour(status),
                        ),
                        None => (String::new(), String::new(), self.theme.muted()),
                    };
                    let room = content_width.saturating_sub(left.chars().count() + 2);
                    let mut detail = if detail.is_empty() {
                        detail
                    } else {
                        format!("  {detail}")
                    };
                    let width = |detail: &str| {
                        word.chars().count() + detail.chars().count() + flags.chars().count()
                    };
                    if width(&detail) > room {
                        detail.clear();
                    }
                    let flags = if width(&detail) > room {
                        " ".to_string()
                    } else {
                        flags
                    };
                    let right =
                        word.chars().count() + detail.chars().count() + flags.chars().count();
                    let gap =
                        " ".repeat(content_width.saturating_sub(left.chars().count() + right));
                    Line::from(vec![
                        Span::styled(icon.to_string(), Style::default().fg(icon_colour)),
                        Span::styled(format!("  {}{suffix}", entry.name), style),
                        Span::raw(gap),
                        Span::styled(word, Style::default().fg(colour)),
                        Span::styled(detail, Style::default().fg(self.theme.faint())),
                        Span::styled(flags, Style::default().fg(self.theme.muted())),
                    ])
                }
            })
            .collect();
        self.fill(frame, area);
        let list = List::new(lines)
            .block(block)
            .highlight_style(self.highlight(focused))
            .highlight_symbol(marker)
            .scroll_padding(1);
        frame.render_stateful_widget(list, area, &mut self.main_state);
    }

    fn draw_plan(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Install Plan",
            "Review planned changes before they run.",
        );
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
        let ids: Vec<String> = self.queue.iter().map(|entry| entry.id.clone()).collect();
        let fresh = self
            .planned
            .as_ref()
            .is_some_and(|cache| cache.ids == ids && cache.made.elapsed() < PLAN_KEPT);
        if !fresh {
            let result = plan(self.queue.iter(), &self.platform, &|id| {
                self.catalog.find(id)
            })
            .map_err(|error| error.to_string());
            let findings = match &result {
                Ok(planned) => self.findings(planned),
                Err(_) => Vec::new(),
            };
            self.planned = Some(PlanCache {
                ids,
                made: std::time::Instant::now(),
                result,
                findings,
            });
        }
        let planned = self
            .planned
            .as_ref()
            .map(|cache| cache.result.clone())
            .unwrap_or_else(|| Err("not planned".to_string()));
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
                lines.extend(
                    self.planned
                        .iter()
                        .flat_map(|cache| cache.findings.iter().map(preflight::Finding::line)),
                );
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
        let parts = Layout::vertical([
            Constraint::Length(summary_height),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);
        self.fill(frame, parts[0]);
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
                        self.icon_gap(Icon::Queued),
                        Style::default().fg(self.theme.success()),
                    ),
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
        self.fill(frame, parts[2]);
        let list = List::new(items)
            .block(self.card("Queued entries"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused));
        frame.render_stateful_widget(list, parts[2], &mut self.main_state);
    }

    fn draw_history(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(frame, area, "History", "Past runs, newest first.");
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
                    Span::styled("● ", Style::default().fg(colour)),
                    Span::styled(format!("{status:<8}"), Style::default().fg(colour).bold()),
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
        self.fill(frame, inner);
        let list = List::new(items)
            .block(self.card("Runs"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused));
        frame.render_stateful_widget(list, inner, &mut self.main_state);
    }

    fn settings(&self) -> Vec<(String, String)> {
        vec![
            ("Theme".to_string(), self.theme.label().to_string()),
            (
                "Icons".to_string(),
                if self.icons {
                    "Nerd Font".to_string()
                } else {
                    "plain".to_string()
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
        let inner = self.page(
            frame,
            area,
            "Settings",
            "Enter changes the selected setting; theme and icons are remembered.",
        );
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
        self.fill(frame, inner);
        let list = List::new(items)
            .block(self.card("Settings"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused));
        frame.render_stateful_widget(list, inner, &mut self.main_state);
    }

    fn draw_footer(&self, frame: &mut Frame, area: Rect) {
        self.rule(frame, area.x, area.y, area.width);
        let (_, hints) = self.hints();
        // Help and quit sit on the right, as in the design.
        let (right, left): (Vec<&Hint>, Vec<&Hint>) = hints
            .iter()
            .partition(|hint| matches!(hint.keys(), ["?"] | ["q"]));
        let spans = |hints: &[&Hint]| {
            let mut spans: Vec<Span> = Vec::new();
            for (index, hint) in hints.iter().enumerate() {
                if index > 0 {
                    spans.push(Span::styled(" · ", Style::default().fg(self.theme.faint())));
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
            spans
        };
        let row =
            Rect::new(area.x + 1, area.y + 1, area.width.saturating_sub(2), 1).intersection(area);
        frame.render_widget(Paragraph::new(Line::from(spans(&left))), row);
        frame.render_widget(
            Paragraph::new(Line::from(spans(&right))).alignment(Alignment::Right),
            row,
        );
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
                            if let Some(Status::Update { .. }) = self.statuses.get(&entry.id) {
                                list.push(Hint::new(&["u"], "Update"));
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
                    // What is installed may have changed.
                    self.planned = None;
                    self.popup = popup
                        .take_problem()
                        .map(|problem| Popup::Text(TextPopup::new("Not recorded", problem, true)));
                    self.refresh_statuses();
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
            KeyCode::Char('t') | KeyCode::Char('T') => {
                self.theme = self.theme.next();
                self.save_look();
            }
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
                KeyCode::Char('u') => {
                    if let Some(entry) = self.selected_entry().cloned() {
                        self.update(vec![entry]);
                    }
                }
                KeyCode::Char('U') => {
                    let waiting: Vec<Entry> = self
                        .catalog
                        .entries()
                        .into_iter()
                        .filter(|entry| {
                            matches!(self.statuses.get(&entry.id), Some(Status::Update { .. }))
                        })
                        .cloned()
                        .collect();
                    if waiting.is_empty() {
                        self.popup = Some(Popup::Text(TextPopup::new(
                            "Nothing to update",
                            "No package entry has an update in the repository metadata on this machine.",
                            true,
                        )));
                    } else {
                        self.update(waiting);
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
                    for step in &planned.steps {
                        lines.push(step.display());
                        lines.extend(step.fallbacks());
                    }
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

    /// The colour of a status word.
    fn status_colour(&self, status: &Status) -> Color {
        match status {
            Status::Installed { .. } | Status::Elsewhere { .. } => self.theme.success(),
            Status::Update { .. } => self.theme.warning(),
            Status::Managed { .. } => self.theme.title(),
            Status::Absent { .. } => self.theme.muted(),
        }
    }

    fn describe(&mut self) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        let mut text = if entry.description.is_empty() {
            "This entry has no description.".to_string()
        } else {
            entry.description.clone()
        };
        if let Some(status) = self.statuses.get(&entry.id) {
            let detail = status.detail();
            text.push_str(&format!("\n\nStatus: {}", status.label()));
            if !detail.is_empty() {
                text.push_str(&format!(" ({detail})"));
            }
            if let Status::Update { .. } = status {
                text.push_str(
                    "\nThe repository metadata on this machine has a newer version: u updates it (on Arch, with the whole system).",
                );
            }
        }
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
            0 => {
                self.theme = self.theme.next();
                self.save_look();
            }
            1 => {
                self.icons = !self.icons;
                self.save_look();
            }
            2 => self.skip_confirmation = !self.skip_confirmation,
            3 => {
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
    /// What the checks before a run find for a plan on this machine.
    fn findings(&self, planned: &Plan) -> Vec<preflight::Finding> {
        if !self.check_statuses {
            // Tests: no machine questions beyond what planning asked.
            return preflight::check(
                planned,
                &self.platform,
                &|id| self.catalog.find(id),
                &preflight::Facts {
                    present: &|_| true,
                    free_bytes: &|_| None,
                    gpus: &Vec::new,
                    repositories: &|| None,
                    sizes: &|_, _| None,
                    ..preflight::Facts::real()
                },
            );
        }
        preflight::check(
            planned,
            &self.platform,
            &|id| self.catalog.find(id),
            &preflight::Facts::real(),
        )
    }

    /// Plan updating entries from the last status check, and ask.
    fn update(&mut self, entries: Vec<Entry>) {
        let Some(inventory) = &self.inventory else {
            self.popup = Some(Popup::Text(TextPopup::new(
                "Still checking",
                "HelixForge is still checking what is installed; try again in a moment.",
                true,
            )));
            return;
        };
        let planned = inventory.plan_update(entries.iter(), &self.platform, &which);
        self.confirm_plan(Ok(planned));
    }

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
        let findings = self.findings(&planned);
        if preflight::blocked(&findings) {
            let lines: Vec<String> = findings.iter().map(preflight::Finding::line).collect();
            self.popup = Some(Popup::Text(TextPopup::new(
                "Cannot run yet",
                format!("{}\n\nNothing was changed.", lines.join("\n")),
                true,
            )));
            return;
        }
        let mut header = vec![format!(
            "Risk: {}{}",
            planned.risk.label(),
            if planned.privileged {
                " · asks for your password"
            } else {
                ""
            }
        )];
        header.extend(findings.iter().map(preflight::Finding::line));
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
        let mut app = App::new(Catalog::builtin(false).unwrap(), platform, &args);
        // Plain text keeps these screen tests readable; one test draws icons.
        app.icons = false;
        app
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

    fn with_banner(mut app: App) -> App {
        #[allow(deprecated)]
        let mut picker = ratatui_image::picker::Picker::from_fontsize((9, 19).into());
        picker.set_protocol_type(ratatui_image::picker::ProtocolType::Kitty);
        app.icons = true;
        app.with_banner(banner::encode(&picker))
    }

    #[test]
    fn the_banner_replaces_the_text_header_where_it_can_be_drawn() {
        let mut app = with_banner(app());
        let text = screen(&mut app, 120, 40);
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines[0].contains('\u{10EEEE}'), "{}", lines[0]);
        assert!(lines[3].contains('\u{10EEEE}'));
        assert!(lines[1].contains("SYSTEM READY"));
        assert!(!text.contains("BUILD • CONFIGURE"));
        // Four rows of picture, then the rule and the sidebar below it.
        assert!(lines[4].contains('─'));
        assert!(lines[6].contains("MAIN MENU"));
    }

    #[test]
    fn the_text_header_returns_without_icons_in_sixteen_colours_or_when_narrow() {
        let mut app = with_banner(app());
        app.icons = false;
        assert!(screen(&mut app, 120, 40).contains("BUILD • CONFIGURE"));
        app.icons = true;
        app.theme = Theme::Compatible;
        assert!(screen(&mut app, 120, 40).contains("BUILD • CONFIGURE"));
        app.theme = Theme::Solid;
        app.size_bypass = true;
        let narrow = screen(&mut app, 40, 30);
        assert!(!narrow.contains('\u{10EEEE}'));
        assert!(narrow.contains("HELIXFORGE"));
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
    fn entries_show_how_they_stand() {
        let mut app = app();
        app.statuses.insert(
            "ripgrep".into(),
            Status::Update {
                source: "pacman".into(),
                version: Some("15.2.0-1".into()),
                latest: Some("15.3.0-1".into()),
                shadowed: None,
            },
        );
        app.statuses.insert(
            "fzf".into(),
            Status::Managed {
                by: "Home Manager".into(),
                version: Some("0.74.4".into()),
            },
        );
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "ripgrep");
        let wide = screen(&mut app, 140, 40);
        let row = wide.lines().find(|line| line.contains("Ripgrep")).unwrap();
        assert!(
            row.contains("Update  pacman 15.2.0-1 → 15.3.0-1  I"),
            "{row}"
        );
        assert!(wide.contains("STATUS · FLAGS"));
        // Short of room, the detail goes first and the word stays.
        app.size_bypass = true;
        let narrow = screen(&mut app, 70, 30);
        let row = narrow
            .lines()
            .find(|line| line.contains("Ripgrep"))
            .unwrap();
        assert!(row.contains("Update") && !row.contains("15.3.0-1"), "{row}");
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "fzf");
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('d'));
        assert!(
            screen(&mut app, 140, 40).contains("Status: Managed (Home Manager 0.74.4)"),
            "the description says it too"
        );
    }

    #[test]
    fn a_blocked_plan_says_why_instead_of_asking() {
        let fixtures =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/packages");
        let args = TuiArgs {
            override_validation: true,
            bypass_root: true,
            ..TuiArgs::default()
        };
        let platform = LinuxInfo::from_os_release("ID=arch\nPRETTY_NAME=\"Arch Linux\"\n").unwrap();
        let mut app = App::new(
            Catalog::from_directory(&fixtures, false).unwrap(),
            platform,
            &args,
        );
        app.icons = false;
        for name in ["Conflict A", "Conflict B"] {
            // A fresh search for each.
            press(&mut app, KeyCode::Char('/'));
            press(&mut app, KeyCode::Esc);
            press(&mut app, KeyCode::Char('/'));
            type_text(&mut app, name);
            press(&mut app, KeyCode::Enter);
            press(&mut app, KeyCode::Char(' '));
        }
        assert_eq!(app.queue.len(), 2);
        app.open_screen(Screen::Plan);
        app.focus = Focus::Main;
        let plan = screen(&mut app, 140, 40);
        assert!(
            plan.contains("Blocked: Conflict A and Conflict B replace each other"),
            "{plan}"
        );
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.popup, Some(Popup::Text(_))), "no confirmation");
        let shown = screen(&mut app, 140, 40);
        assert!(
            shown.contains("Cannot run yet") && shown.contains("Nothing was changed"),
            "{shown}"
        );
        assert!(app.pending.is_none());
    }

    #[test]
    fn u_plans_an_update_and_waits_for_the_status_check() {
        let mut app = app();
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "ripgrep");
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('u'));
        assert!(screen(&mut app, 140, 40).contains("still checking what is installed"));
        press(&mut app, KeyCode::Esc);
        let inventory = Inventory::from_parts(
            Some(helixforge_core::PackageManager::Arch),
            Some([("ripgrep".to_string(), "15.2.0-1".to_string())].into()),
            [("ripgrep".to_string(), "15.3.0-1".to_string())].into(),
            HashMap::new(),
            Default::default(),
        );
        let ripgrep = app.catalog.find("ripgrep").unwrap().clone();
        let status = inventory
            .status(&ripgrep, &app.platform, &|_| None)
            .unwrap();
        assert_eq!(status.label(), "Update");
        app.statuses.insert("ripgrep".into(), status);
        app.inventory = Some(inventory);
        assert!(
            screen(&mut app, 140, 40).contains("u Update"),
            "the hint offers it"
        );
        press(&mut app, KeyCode::Char('u'));
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        let shown = screen(&mut app, 140, 40);
        assert!(shown.contains("sudo pacman -Syu"), "{shown}");
        assert!(shown.contains("partial upgrade"));
        press(&mut app, KeyCode::Esc);
        app.popup = None;
        press(&mut app, KeyCode::Char('U'));
        assert!(
            matches!(app.popup, Some(Popup::Confirm(_))),
            "U finds it too"
        );
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
        assert_eq!(app.theme, Theme::Solid, "solid is the default");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.theme, Theme::Transparent);
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Enter);
        assert!(app.icons);
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
    fn the_dashboard_draws_cards_and_icons_in_every_theme() {
        let mut app = app();
        app.icons = true;
        for theme in [Theme::Solid, Theme::Transparent, Theme::Compatible] {
            app.theme = theme;
            let text = screen(&mut app, 120, 40);
            assert!(text.contains("\u{f0e4}  Dashboard"), "{text}");
            assert!(text.contains("\u{f0ad}  Maintenance"), "{text}");
            assert!(text.contains("╭"), "{text}");
            assert!(text.contains("See what is queued and run it"), "{text}");
            assert!(text.contains("? Help"), "{text}");
        }
        // The smallest supported size still shows every quick action.
        let text = screen(&mut app, 100, 25);
        assert!(text.contains("View History"), "{text}");
        assert!(text.contains("Install Plan"), "{text}");
        app.icons = false;
        assert!(!screen(&mut app, 120, 40).contains('\u{f0e4}'));
    }

    #[test]
    fn timestamps_and_escape_stripping_are_sane() {
        assert_eq!(timestamp(0), "1970-01-01 00:00 UTC");
        assert_eq!(timestamp(1_700_000_000), "2023-11-14 22:13 UTC");
        assert_eq!(strip_escapes("a\u{1b}[31mb\u{1b}[0m\r\n"), "ab\n");
    }
}
