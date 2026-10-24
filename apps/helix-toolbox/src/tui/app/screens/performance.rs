//! The Performance screen.

use super::super::*;

impl App {
    /// Read the boot and the machine in the background, recording this boot.
    pub(in crate::tui::app) fn read_performance(&mut self) {
        if !self.examine_here {
            self.build_performance_rows();
            return;
        }
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let state = history::state_dir();
            let history = history::read(1000).unwrap_or_default();
            let found = performance::gather(&doctor::Here, state.as_deref(), &history);
            let boots = state
                .map(|state| performance::load(&performance::boots_path(&state)))
                .unwrap_or_default();
            let _ = sender.send((found, boots));
        });
        self.reading_performance = Some(receiver);
    }

    /// The recorded boots before this one, newest first.
    pub(in crate::tui::app) fn earlier_boots(&self) -> Vec<&performance::Boot> {
        let Some((found, boots)) = &self.performance_found else {
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
    fn performance_comparison(&self) -> Option<performance::Comparison> {
        let (found, _) = self.performance_found.as_ref()?;
        if self.performance_against == 0 {
            return found.previous.clone();
        }
        let before = self
            .earlier_boots()
            .get(self.performance_against)
            .copied()?;
        let history = if self.examine_here {
            history::read(1000).unwrap_or_default()
        } else {
            Vec::new()
        };
        Some(performance::compare(before, found.boot.as_ref()?, &history))
    }

    pub(in crate::tui::app) fn build_performance_rows(&mut self) {
        let mut list = Vec::new();
        let comparison = self.performance_comparison();
        if let Some((found, _)) = &self.performance_found {
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
        self.performance_rows = list;
        self.clamp_main();
    }

    pub(in crate::tui::app) fn selected_performance_row(&self) -> Option<PerformanceRow> {
        self.performance_rows
            .get(self.main_state.selected()?)
            .cloned()
    }

    fn selected_hint(&self) -> Option<performance::Hint> {
        let Some(PerformanceRow::Hint(index)) = self.selected_performance_row() else {
            return None;
        };
        let (found, _) = self.performance_found.as_ref()?;
        found.hints.get(index).cloned()
    }

    pub(in crate::tui::app) fn describe_performance(&mut self) {
        let (title, text) = match self.selected_performance_row() {
            Some(PerformanceRow::Hint(_)) => {
                let Some(hint) = self.selected_hint() else {
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
                if let Some(entry) = hint.entry.as_deref().and_then(|id| self.catalog.find(id)) {
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
        self.popup = Some(Popup::Text(TextPopup::new(title, text, true)));
    }

    /// Show a unit in the Services screen, filtered to it.
    fn show_unit(&mut self, name: &str) {
        self.service_filter = name.to_string();
        self.service_view = ServiceView::System;
        self.open_screen(Screen::Services);
    }

    /// Enter on a hint plans what changes it; on a unit, shows it in
    /// Services.
    pub(in crate::tui::app) fn act_on_performance(&mut self) {
        match self.selected_performance_row() {
            Some(PerformanceRow::Unit(name, _)) => self.show_unit(&name),
            Some(PerformanceRow::Hint(_)) => {
                let Some(hint) = self.selected_hint() else {
                    return;
                };
                match (&hint.unit, &hint.command, &hint.entry) {
                    (Some(unit), Some(command), _) if command.contains(" disable ") => {
                        let needed = if self.examine_here {
                            services::needed_by(&doctor::Here, unit, false)
                        } else {
                            Vec::new()
                        };
                        match services::plan(
                            unit,
                            false,
                            Verb::Disable,
                            &needed,
                            std::env::var_os("SSH_CONNECTION").is_some(),
                        ) {
                            Ok(planned) => self.confirm_plan(Ok(planned)),
                            Err(why) => {
                                self.popup =
                                    Some(Popup::Text(TextPopup::new("Cannot plan", why, true)));
                            }
                        }
                    }
                    (Some(unit), _, _) => {
                        let unit = unit.clone();
                        self.show_unit(&unit);
                    }
                    (None, _, Some(entry)) => match self.catalog.find(entry).cloned() {
                        Some(entry) => self.confirm(vec![entry]),
                        None => self.describe_performance(),
                    },
                    _ => self.describe_performance(),
                }
            }
            _ => {}
        }
    }

    pub(in crate::tui::app) fn draw_performance(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Performance",
            "How long the boot took and where the time went, and the machine now. It only reads.",
        );
        self.fill(frame, inner);
        let focused = self.focus == Focus::Main;
        let mut items: Vec<Line> = Vec::new();
        if self.performance_rows.is_empty() {
            items.push(Line::styled(
                if self.reading_performance.is_some() {
                    "Reading the boot…"
                } else {
                    "Press r to read this boot and the machine now."
                },
                Style::default().fg(self.theme.muted()),
            ));
        }
        let hints = self
            .performance_found
            .as_ref()
            .map(|(found, _)| found.hints.clone())
            .unwrap_or_default();
        for row in &self.performance_rows {
            items.push(match row {
                PerformanceRow::Heading(text) => {
                    Line::styled(text.clone(), Style::default().fg(self.theme.group()).bold())
                }
                PerformanceRow::Text(text) => {
                    Line::styled(format!("  {text}"), Style::default().fg(self.theme.entry()))
                }
                PerformanceRow::Change(text, slower) => Line::styled(
                    format!("  {text}"),
                    Style::default().fg(if *slower {
                        self.theme.warning()
                    } else {
                        self.theme.success()
                    }),
                ),
                PerformanceRow::Unit(name, took) => Line::from(vec![
                    Span::styled(
                        format!("  {took:>10}  "),
                        Style::default().fg(self.theme.muted()),
                    ),
                    Span::styled(name.clone(), Style::default().fg(self.theme.entry())),
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
                            Style::default().fg(self.theme.warning()),
                        ),
                        Span::styled(offer, Style::default().fg(self.theme.focused())),
                    ])
                }
            });
        }
        let title = if self.reading_performance.is_some() && !self.performance_rows.is_empty() {
            "Boot and machine (reading again…)"
        } else {
            "Boot and machine"
        };
        let list = List::new(self.measure(items))
            .block(self.card(title))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, inner, &mut self.main_state, &self.hits);
    }
}
