//! The Services screen.

use super::super::*;
use super::{Ask, Cx, ToolScreen, View};

/// systemd's units and the applications started at login.
#[derive(Default)]
pub(in crate::tui::app) struct Services {
    /// The units and autostart entries, as last read.
    found: Option<services::Services>,
    pub(in crate::tui::app) rows: Vec<ServiceRow>,
    view: ServiceView,
    /// What the names shown must have.
    pub(in crate::tui::app) filter: String,
}

/// Which units the Services screen lists.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ServiceView {
    #[default]
    System,
    Timers,
    User,
    Login,
    Failed,
}

impl ServiceView {
    fn next(self) -> Self {
        match self {
            ServiceView::System => ServiceView::Timers,
            ServiceView::Timers => ServiceView::User,
            ServiceView::User => ServiceView::Login,
            ServiceView::Login => ServiceView::Failed,
            ServiceView::Failed => ServiceView::System,
        }
    }

    fn label(self) -> &'static str {
        match self {
            ServiceView::System => "the machine's services",
            ServiceView::Timers => "timers and sockets",
            ServiceView::User => "your session's",
            ServiceView::Login => "started at login",
            ServiceView::Failed => "failed",
        }
    }
}

/// A line of the Services screen.
#[derive(Clone, Debug)]
pub(in crate::tui::app) enum ServiceRow {
    Heading(String),
    Unit(usize),
    Autostart(usize),
}

impl Services {
    /// Read the units and the autostart entries again; quick, so not in the
    /// background.
    fn reload(&mut self, cx: &mut Cx) {
        self.found = Some(services::gather_on(cx.host));
        self.build_rows(cx);
    }

    pub(in crate::tui::app) fn unit(&self, index: usize) -> Option<&services::Unit> {
        self.found.as_ref()?.units.get(index)
    }

    fn selected(&self, at: Option<usize>) -> Option<ServiceRow> {
        self.rows.get(at?).cloned()
    }

    /// Show only the machine's services named `name`, as opening the
    /// screen from another does.
    pub(in crate::tui::app) fn show(&mut self, name: &str) {
        self.filter = name.to_string();
        self.view = ServiceView::System;
    }

    /// What the names must have, as typed; the list starts again at the top.
    pub(in crate::tui::app) fn set_filter(&mut self, cx: &mut Cx, text: &str) {
        self.filter = text.trim().to_string();
        cx.selected.select(Some(0));
        self.build_rows(cx);
    }

    /// The rows of the chosen view, those the filter lets through.
    fn build_rows(&mut self, cx: &mut Cx) {
        let mut rows = Vec::new();
        if let Some(found) = &self.found {
            let filter = self.filter.to_lowercase();
            let wanted = |text: &str, more: &str| {
                filter.is_empty()
                    || text.to_lowercase().contains(&filter)
                    || more.to_lowercase().contains(&filter)
            };
            if self.view == ServiceView::Login {
                rows.push(ServiceRow::Heading(format!(
                    "Started at login ({})",
                    found.autostart.len()
                )));
                rows.extend(
                    found
                        .autostart
                        .iter()
                        .enumerate()
                        .filter(|(_, item)| wanted(&item.name, &item.file))
                        .map(|(index, _)| ServiceRow::Autostart(index)),
                );
            } else {
                let units: Vec<usize> = found
                    .units
                    .iter()
                    .enumerate()
                    .filter(|(_, unit)| match self.view {
                        ServiceView::System => !unit.user && unit.kind() == "service",
                        ServiceView::Timers => unit.kind() != "service",
                        ServiceView::User => unit.user && unit.kind() == "service",
                        ServiceView::Failed => unit.failed(),
                        ServiceView::Login => false,
                    })
                    .filter(|(_, unit)| wanted(&unit.name, &unit.description))
                    .map(|(index, _)| index)
                    .collect();
                rows.push(ServiceRow::Heading(format!(
                    "{} ({})",
                    match self.view {
                        ServiceView::System => "The machine's services",
                        ServiceView::Timers => "Timers and sockets",
                        ServiceView::User => "Your session's services",
                        ServiceView::Failed => "Failed units",
                        ServiceView::Login => "",
                    },
                    units.len()
                )));
                rows.extend(units.into_iter().map(ServiceRow::Unit));
            }
        }
        self.rows = rows;
        cx.clamp();
    }

    fn describe_unit(&self, cx: &mut Cx, index: usize) {
        let Some(unit) = self.unit(index).cloned() else {
            return;
        };
        let needed = services::needed_by(cx.host, &unit.name, unit.user);
        let shown = rows::unit(&unit);
        let mut text = format!(
            "{}\n\nEnabled: {}\nRunning: {}\n",
            if unit.description.is_empty() {
                "No description."
            } else {
                unit.description.as_str()
            },
            shown.enablement,
            shown.now
        );
        if !needed.is_empty() {
            text.push_str(&format!("Needed by: {}\n", needed.join(", ")));
        }
        if let Some(cost) = services::consequence(&unit.name, unit.user, cx.host.ssh()) {
            text.push_str(&format!("\nBefore stopping or disabling it: {cost}\n"));
        }
        if let Some(note) = shown.note {
            text.push_str(&format!("\n{note}\n"));
        }
        text.push_str(
            "\ns starts or stops it, e enables or disables it, R restarts it; each asks first.",
        );
        *cx.popup = Some(Popup::Text(TextPopup::new(
            format!("{}{}", if unit.user { "Your " } else { "" }, unit.name),
            text,
            true,
        )));
    }

    /// Plan starting, stopping, restarting, enabling or disabling a unit.
    fn change_unit(&self, cx: &mut Cx, index: usize, verb: Verb) {
        let Some(unit) = self.unit(index).cloned() else {
            return;
        };
        cx.confirm_or_say(services::plan_on(cx.host, &unit.name, unit.user, verb));
    }

    /// An application started at login, as the window's side pane shows it.
    fn describe_autostart(&self, cx: &mut Cx, index: usize) {
        let Some(item) = self
            .found
            .as_ref()
            .and_then(|found| found.autostart.get(index))
        else {
            return;
        };
        let shown = rows::autostart(item);
        let mut text = String::new();
        if !item.comment.is_empty() {
            text.push_str(&format!("{}\n\n", item.comment));
        }
        text.push_str(&format!(
            "Runs: {}\nFrom: {}\nAt login: {}\n\n",
            shown.runs, shown.from, shown.at_login
        ));
        text.push_str(&format!(
            "Enter plans {}, reviewed before it changes anything. For you only: the system's file stays as it is.",
            if item.enabled {
                "not starting it at login"
            } else {
                "starting it at login"
            }
        ));
        *cx.popup = Some(Popup::Text(TextPopup::new(item.name.clone(), text, true)));
    }

    /// Plan turning an autostart entry off, or on again.
    fn switch_autostart(&self, cx: &mut Cx, index: usize) {
        let Some(item) = self
            .found
            .as_ref()
            .and_then(|found| found.autostart.get(index))
            .cloned()
        else {
            return;
        };
        let dirs = services::AutostartDirs::of(cx.host);
        cx.confirm_or_say(services::plan_autostart(
            &item,
            !item.enabled,
            &dirs,
            cx.host,
        ));
    }
}

impl ToolScreen for Services {
    fn open(&mut self, cx: &mut Cx) {
        self.reload(cx);
    }

    fn len(&self, _view: &View) -> usize {
        self.rows.len()
    }

    fn key(&mut self, cx: &mut Cx, code: KeyCode) {
        match code {
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('l') | KeyCode::Right => {
                match self.selected(cx.at()) {
                    Some(ServiceRow::Unit(index)) => self.describe_unit(cx, index),
                    Some(ServiceRow::Autostart(index)) => self.switch_autostart(cx, index),
                    _ => {}
                }
            }
            KeyCode::Char('d') | KeyCode::Char('D') => match self.selected(cx.at()) {
                Some(ServiceRow::Unit(index)) => self.describe_unit(cx, index),
                Some(ServiceRow::Autostart(index)) => self.describe_autostart(cx, index),
                _ => {}
            },
            KeyCode::Char('s') => {
                if let Some(ServiceRow::Unit(index)) = self.selected(cx.at())
                    && let Some(unit) = self.unit(index)
                {
                    let verb = if unit.running() {
                        Verb::Stop
                    } else {
                        Verb::Start
                    };
                    self.change_unit(cx, index, verb);
                }
            }
            KeyCode::Char('e') => {
                if let Some(ServiceRow::Unit(index)) = self.selected(cx.at())
                    && let Some(unit) = self.unit(index)
                    && unit.switchable()
                {
                    let verb = if unit.enabled() {
                        Verb::Disable
                    } else {
                        Verb::Enable
                    };
                    self.change_unit(cx, index, verb);
                }
            }
            KeyCode::Char('R') => {
                if let Some(ServiceRow::Unit(index)) = self.selected(cx.at()) {
                    self.change_unit(cx, index, Verb::Restart);
                }
            }
            KeyCode::Char('i') => {
                self.view = self.view.next();
                cx.selected.select(Some(0));
                self.build_rows(cx);
            }
            KeyCode::Char('f') => {
                *cx.popup = Some(Popup::Ask(
                    InputPopup::new(
                        "Filter",
                        "Only units and applications whose name or description has (blank for all):",
                    )
                    .with_value(self.filter.clone())
                    .answering("Filter"),
                    Asked::ServiceFilter,
                ));
            }
            KeyCode::Char('r') => self.reload(cx),
            KeyCode::Char('h') | KeyCode::Left => cx.ask(Ask::Sidebar),
            _ => {}
        }
    }

    fn hints(&self, _view: &View, at: Option<usize>, list: &mut Vec<Hint>) {
        list.push(Hint::new(&["↑↓"], "Navigate"));
        match self.selected(at) {
            Some(ServiceRow::Unit(index)) => {
                list.push(Hint::new(&["Enter"], "Details"));
                if let Some(unit) = self.unit(index) {
                    list.push(Hint::new(
                        &["s"],
                        if unit.running() { "Stop" } else { "Start" },
                    ));
                    if unit.switchable() {
                        list.push(Hint::new(
                            &["e"],
                            if unit.enabled() { "Disable" } else { "Enable" },
                        ));
                    }
                    if unit.running() {
                        list.push(Hint::new(&["R"], "Restart"));
                    }
                }
            }
            Some(ServiceRow::Autostart(_)) => {
                list.push(Hint::new(&["Enter"], "On or off at login"));
                list.push(Hint::new(&["d"], "Details"));
            }
            _ => {}
        }
        list.push(Hint::new(&["i"], "Show"));
        list.push(Hint::new(&["f"], "Filter"));
        list.push(Hint::new(&["r"], "Read again"));
    }

    fn after_run(&mut self, cx: &mut Cx, reading: Reading) {
        if reading == Reading::Tools {
            self.reload(cx);
        }
    }

    fn draw(&self, view: &View, frame: &mut Frame, area: Rect, state: &mut ListState) {
        let inner = view.page(
            frame,
            area,
            "Services",
            "systemd's units and what starts at login; each change asks first and says what it costs.",
        );
        view.fill(frame, inner);
        let focused = view.focused;
        let mut items: Vec<Line> = Vec::new();
        let mut title = format!("Showing {}", self.view.label());
        if !self.filter.is_empty() {
            title.push_str(&format!(", with \"{}\"", self.filter));
        }
        match &self.found {
            None => items.push(Line::styled(
                "Press r to read this machine's services.",
                Style::default().fg(view.theme.muted()),
            )),
            Some(found) if !found.systemd && self.view != ServiceView::Login => {
                items.push(Line::styled(
                    "systemd does not run this machine, so there are no units; i shows what starts at login.",
                    Style::default().fg(view.theme.muted()),
                ))
            }
            Some(found) => {
                let width = inner.width.saturating_sub(8) as usize;
                for row in &self.rows {
                    items.push(match row {
                        ServiceRow::Heading(text) => Line::styled(
                            text.clone(),
                            Style::default().fg(view.theme.group()).bold(),
                        ),
                        ServiceRow::Unit(index) => {
                            let unit = &found.units[*index];
                            let state = format!(
                                "{:<9} {:<9}",
                                unit.enablement.as_deref().unwrap_or("-"),
                                unit.active.as_deref().unwrap_or("-")
                            );
                            let color = if unit.failed() {
                                view.theme.failure()
                            } else if unit.running() {
                                view.theme.success()
                            } else {
                                view.theme.muted()
                            };
                            let name: String = unit.name.chars().take(40).collect();
                            let room = width.saturating_sub(42 + state.len());
                            Line::from(vec![
                                Span::styled(format!("  {name:<40} "), Style::default().fg(view.theme.entry())),
                                Span::styled(state, Style::default().fg(color)),
                                Span::styled(
                                    format!("  {}", unit.description.chars().take(room).collect::<String>()),
                                    Style::default().fg(view.theme.muted()),
                                ),
                            ])
                        }
                        ServiceRow::Autostart(index) => {
                            let item = &found.autostart[*index];
                            let word = rows::autostart(item).state;
                            let color = match (item.here, item.enabled) {
                                (false, _) => view.theme.muted(),
                                (true, true) => view.theme.success(),
                                (true, false) => view.theme.warning(),
                            };
                            Line::from(vec![
                                Span::styled(format!("  {:<36} ", item.name), Style::default().fg(view.theme.entry())),
                                Span::styled(format!("{word:<9}"), Style::default().fg(color)),
                                Span::styled(
                                    format!("  {}", item.exec.chars().take(width.saturating_sub(48)).collect::<String>()),
                                    Style::default().fg(view.theme.muted()),
                                ),
                            ])
                        }
                    });
                }
            }
        }
        let list = List::new(view.measure(items))
            .block(view.card(&title))
            .highlight_style(view.highlight(focused))
            .highlight_symbol(view.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, inner, state, view.hits);
    }
}
