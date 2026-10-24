//! The Performance screen.

use super::super::*;
use super::{Ask, Cx, Read, ToolScreen, View};

/// The boot's times, the machine now, and this boot beside another.
#[derive(Default)]
pub(in crate::tui::app) struct Performance {
    /// The performance as last read, with the boots recorded so far.
    found: Option<PerformanceReading>,
    /// Which earlier boot this one is set beside (0 is the one before).
    against: usize,
    pub(in crate::tui::app) rows: Vec<PerformanceRow>,
    reading: Read<PerformanceReading>,
}

/// A reading of the performance, with the boots recorded so far.
type PerformanceReading = (performance::Performance, Vec<performance::Boot>);

/// A line of the Performance screen.
#[derive(Clone, Debug)]
pub(in crate::tui::app) enum PerformanceRow {
    Heading(String),
    Text(String),
    /// A change from the earlier boot, and whether it is slower.
    Change(String, bool),
    /// A unit and how long it took.
    Unit(String, String),
    Hint(usize),
}

impl Performance {
    /// Read the boot and the machine in the background, recording this boot.
    fn read(&mut self, cx: &Cx) {
        self.reading.start(cx.host, |host| {
            (performance::gather_on(&host), performance::boots_on(&host))
        });
    }

    /// The recorded boots before this one, newest first.
    fn earlier_boots(&self) -> Vec<&performance::Boot> {
        let Some((found, boots)) = &self.found else {
            return Vec::new();
        };
        let Some(boot) = &found.boot else {
            return Vec::new();
        };
        boots
            .iter()
            .filter(|other| other.id != boot.id && other.booted <= boot.booted)
            .collect()
    }

    /// This boot beside the chosen earlier one.
    fn comparison(&self, host: &Host) -> Option<performance::Comparison> {
        let (found, _) = self.found.as_ref()?;
        if self.against == 0 {
            return found.previous.clone();
        }
        let before = self.earlier_boots().get(self.against).copied()?;
        Some(performance::compare(
            before,
            found.boot.as_ref()?,
            &host.history(1000),
        ))
    }

    fn build_rows(&mut self, cx: &mut Cx) {
        let mut list = Vec::new();
        let comparison = self.comparison(cx.host);
        if let Some((found, _)) = &self.found {
            list.push(PerformanceRow::Heading("This boot".into()));
            let shown = found.boot.as_ref().map(rows::boot);
            match (&found.boot, &shown) {
                (Some(boot), Some(shown)) => {
                    list.push(PerformanceRow::Text(format!(
                        "{}, kernel {}: {}",
                        timestamp(boot.booted),
                        boot.kernel,
                        shown.total
                    )));
                    list.push(PerformanceRow::Text(
                        shown
                            .parts
                            .iter()
                            .map(|part| format!("{} {}", part.name, part.shown))
                            .collect::<Vec<_>>()
                            .join(" · "),
                    ));
                }
                _ => list.push(PerformanceRow::Text(
                    found.no_boot.clone().unwrap_or_default(),
                )),
            }
            match &comparison {
                Some(compared) => {
                    let said = rows::comparison(compared);
                    list.push(PerformanceRow::Heading(format!(
                        "Against the boot of {}: {}",
                        timestamp(compared.before.booted),
                        said.total
                    )));
                    for change in &said.changes {
                        list.push(PerformanceRow::Change(
                            format!("{:<40} {:>10}  ({})", change.name, change.now, change.how),
                            change.slower,
                        ));
                    }
                    if let Some((before, after)) = &said.kernel {
                        list.push(PerformanceRow::Text(format!(
                            "The kernel changed: {before} to {after}"
                        )));
                    }
                    if said.runs.is_empty() {
                        list.push(PerformanceRow::Text(
                            "No Helix Toolbox runs in between.".into(),
                        ));
                    }
                    for (at, ran) in &said.runs {
                        list.push(PerformanceRow::Text(format!(
                            "Ran in between, {}: {ran}",
                            timestamp(*at)
                        )));
                    }
                }
                None if found.boot.is_some() => list.push(PerformanceRow::Text(
                    "No earlier boot is recorded yet; after the next boot, this one is compared with it."
                        .into(),
                )),
                None => {}
            }
            if let Some(shown) = shown {
                if !shown.waited.is_empty() {
                    list.push(PerformanceRow::Heading(
                        "The login screen waited for".into(),
                    ));
                    list.extend(
                        shown
                            .waited
                            .into_iter()
                            .map(|(name, took)| PerformanceRow::Unit(name, took)),
                    );
                }
                if !shown.slowest.is_empty() {
                    list.push(PerformanceRow::Heading(
                        "Slowest services since the boot".into(),
                    ));
                    list.extend(
                        shown
                            .slowest
                            .into_iter()
                            .map(|(name, took)| PerformanceRow::Unit(name, took)),
                    );
                }
            }
            list.push(PerformanceRow::Heading("Now".into()));
            for fact in rows::now(&found.now) {
                list.push(PerformanceRow::Text(if fact.note.is_empty() {
                    format!("{}: {}", fact.term, fact.text)
                } else {
                    format!("{}: {} {}", fact.term, fact.text, fact.note)
                }));
            }
            if !found.hints.is_empty() {
                list.push(PerformanceRow::Heading("Worth knowing".into()));
                list.extend((0..found.hints.len()).map(PerformanceRow::Hint));
            }
        }
        self.rows = list;
        cx.clamp();
    }

    fn selected(&self, at: Option<usize>) -> Option<PerformanceRow> {
        self.rows.get(at?).cloned()
    }

    fn selected_hint(&self, at: Option<usize>) -> Option<performance::Hint> {
        let Some(PerformanceRow::Hint(index)) = self.selected(at) else {
            return None;
        };
        let (found, _) = self.found.as_ref()?;
        found.hints.get(index).cloned()
    }

    fn describe(&self, cx: &mut Cx) {
        let (title, text) = match self.selected(cx.at()) {
            Some(PerformanceRow::Hint(_)) => {
                let Some(hint) = self.selected_hint(cx.at()) else {
                    return;
                };
                let mut text = hint.detail.clone();
                if let Some(command) = &hint.command {
                    text.push_str(&format!("\n\n{command}\nEnter plans it here, then asks."));
                    if command.contains(" show ") {
                        text = text.replace(
                            "Enter plans it here, then asks.",
                            "Enter shows it in Services.",
                        );
                    }
                }
                if let Some(entry) = hint.entry.as_deref().and_then(|id| cx.catalog.find(id)) {
                    text.push_str(&format!(
                        "\n\nThe {} entry sets it up; Enter runs it after confirmation.",
                        entry.name
                    ));
                }
                (hint.title, text)
            }
            Some(PerformanceRow::Unit(name, took)) => (
                name.clone(),
                format!(
                    "{name} took {took}.\n\nEnter shows it in Services, with what needs it and what stopping it costs."
                ),
            ),
            _ => return,
        };
        *cx.popup = Some(Popup::Text(TextPopup::new(title, text, true)));
    }

    /// Enter on a hint plans what changes it; on a unit, shows it in
    /// Services.
    fn act(&self, cx: &mut Cx) {
        match self.selected(cx.at()) {
            Some(PerformanceRow::Unit(name, _)) => cx.ask(Ask::ShowUnit(name)),
            Some(PerformanceRow::Hint(_)) => {
                let Some(hint) = self.selected_hint(cx.at()) else {
                    return;
                };
                match (&hint.unit, &hint.command, &hint.entry) {
                    (Some(unit), Some(command), _) if command.contains(" disable ") => {
                        cx.confirm_or_say(services::plan_on(cx.host, unit, false, Verb::Disable));
                    }
                    (Some(unit), _, _) => cx.ask(Ask::ShowUnit(unit.clone())),
                    (None, _, Some(entry)) => match cx.catalog.find(entry).cloned() {
                        Some(entry) => cx.ask(Ask::Run(vec![entry])),
                        None => self.describe(cx),
                    },
                    _ => self.describe(cx),
                }
            }
            _ => {}
        }
    }
}

impl ToolScreen for Performance {
    fn open(&mut self, cx: &mut Cx) {
        if self.found.is_none() {
            self.read(cx);
        } else {
            self.build_rows(cx);
        }
    }

    fn len(&self, _view: &View) -> usize {
        self.rows.len()
    }

    fn key(&mut self, cx: &mut Cx, code: KeyCode) {
        match code {
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('l') | KeyCode::Right => {
                self.act(cx)
            }
            KeyCode::Char('d') => self.describe(cx),
            KeyCode::Char('b') => {
                let earlier = self.earlier_boots().len();
                if earlier > 1 {
                    self.against = (self.against + 1) % earlier;
                    self.build_rows(cx);
                }
            }
            KeyCode::Char('r') => self.read(cx),
            KeyCode::Char('h') | KeyCode::Left => cx.ask(Ask::Sidebar),
            _ => {}
        }
    }

    fn hints(&self, _view: &View, at: Option<usize>, list: &mut Vec<Hint>) {
        list.push(Hint::new(&["↑↓"], "Navigate"));
        match self.selected(at) {
            Some(PerformanceRow::Hint(_)) => {
                list.push(Hint::new(&["Enter"], "Act on it"));
                list.push(Hint::new(&["d"], "Details"));
            }
            Some(PerformanceRow::Unit(..)) => list.push(Hint::new(&["Enter"], "In Services")),
            _ => {}
        }
        if self.earlier_boots().len() > 1 {
            list.push(Hint::new(&["b"], "An earlier boot"));
        }
        list.push(Hint::new(&["r"], "Read again"));
    }

    fn poll(&mut self, cx: &mut Cx) -> bool {
        let Some(found) = self.reading.take() else {
            return false;
        };
        self.found = Some(found);
        self.build_rows(cx);
        true
    }

    fn draw(&self, view: &View, frame: &mut Frame, area: Rect, state: &mut ListState) {
        let inner = view.page(
            frame,
            area,
            "Performance",
            "How long the boot took and where the time went, and the machine now. It only reads.",
        );
        view.fill(frame, inner);
        let focused = view.focused;
        let mut items: Vec<Line> = Vec::new();
        if self.rows.is_empty() {
            items.push(Line::styled(
                if self.reading.busy() {
                    "Reading the boot…"
                } else {
                    "Press r to read this boot and the machine now."
                },
                Style::default().fg(view.theme.muted()),
            ));
        }
        let hints = self
            .found
            .as_ref()
            .map(|(found, _)| found.hints.clone())
            .unwrap_or_default();
        for row in &self.rows {
            items.push(match row {
                PerformanceRow::Heading(text) => {
                    Line::styled(text.clone(), Style::default().fg(view.theme.group()).bold())
                }
                PerformanceRow::Text(text) => {
                    Line::styled(format!("  {text}"), Style::default().fg(view.theme.entry()))
                }
                PerformanceRow::Change(text, slower) => Line::styled(
                    format!("  {text}"),
                    Style::default().fg(if *slower {
                        view.theme.warning()
                    } else {
                        view.theme.success()
                    }),
                ),
                PerformanceRow::Unit(name, took) => Line::from(vec![
                    Span::styled(
                        format!("  {took:>10}  "),
                        Style::default().fg(view.theme.muted()),
                    ),
                    Span::styled(name.clone(), Style::default().fg(view.theme.entry())),
                ]),
                PerformanceRow::Hint(index) => {
                    let hint = &hints[*index];
                    let offer = match (&hint.command, &hint.entry) {
                        (Some(command), _) if command.contains(" disable ") => {
                            "  · Enter turns it off"
                        }
                        (Some(_), _) => "  · Enter shows it",
                        (None, Some(_)) => "  · Enter sets it up",
                        _ => "  · d details",
                    };
                    Line::from(vec![
                        Span::styled(
                            format!("  {}", hint.title),
                            Style::default().fg(view.theme.warning()),
                        ),
                        Span::styled(offer, Style::default().fg(view.theme.focused())),
                    ])
                }
            });
        }
        let title = if self.reading.busy() && !self.rows.is_empty() {
            "Boot and machine (reading again…)"
        } else {
            "Boot and machine"
        };
        let list = List::new(view.measure(items))
            .block(view.card(title))
            .highlight_style(view.highlight(focused))
            .highlight_symbol(view.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, inner, state, view.hits);
    }
}
