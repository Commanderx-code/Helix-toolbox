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
        let mut rows = Vec::new();
        let comparison = self.performance_comparison();
        if let Some((found, _)) = &self.performance_found {
            rows.push(PerformanceRow::Heading("This boot".into()));
            match &found.boot {
                Some(boot) => {
                    rows.push(PerformanceRow::Text(format!(
                        "{}, kernel {}: {}",
                        timestamp(boot.booted),
                        boot.kernel,
                        performance::shown(boot.phases.total)
                    )));
                    rows.push(PerformanceRow::Text(
                        boot.phases
                            .parts()
                            .iter()
                            .map(|(name, took)| format!("{name} {}", performance::shown(*took)))
                            .collect::<Vec<_>>()
                            .join(" · "),
                    ));
                }
                None => rows.push(PerformanceRow::Text(
                    found.no_boot.clone().unwrap_or_default(),
                )),
            }
            match &comparison {
                Some(compared) => {
                    rows.push(PerformanceRow::Heading(format!(
                        "Against the boot of {}: {}",
                        timestamp(compared.before.booted),
                        performance::shown_change(compared.total())
                    )));
                    for change in compared.phases.iter().skip(1).chain(&compared.units) {
                        let now = change
                            .after
                            .map(performance::shown)
                            .unwrap_or_else(|| "not now".into());
                        let (how, slower) = match change.difference() {
                            Some(difference) if difference.unsigned_abs() < 100 => continue,
                            Some(difference) => {
                                (performance::shown_change(difference), difference > 0)
                            }
                            None if change.before.is_none() => ("new".to_string(), true),
                            None => ("gone".to_string(), false),
                        };
                        rows.push(PerformanceRow::Change(
                            format!("{:<40} {now:>10}  ({how})", change.name),
                            slower,
                        ));
                    }
                    if compared.before.kernel != compared.after.kernel
                        && !compared.before.kernel.is_empty()
                    {
                        rows.push(PerformanceRow::Text(format!(
                            "The kernel changed: {} to {}",
                            compared.before.kernel, compared.after.kernel
                        )));
                    }
                    if compared.runs.is_empty() {
                        rows.push(PerformanceRow::Text(
                            "No Helix Toolbox runs in between.".into(),
                        ));
                    }
                    for run in &compared.runs {
                        rows.push(PerformanceRow::Text(format!(
                            "Ran in between, {}: {}{}",
                            timestamp(run.at),
                            run.names.join(", "),
                            if run.success { "" } else { " (failed)" }
                        )));
                    }
                }
                None if found.boot.is_some() => rows.push(PerformanceRow::Text(
                    "No earlier boot is recorded yet; after the next boot, this one is compared with it."
                        .into(),
                )),
                None => {}
            }
            if let Some(boot) = &found.boot {
                let waited: Vec<&performance::Link> = boot
                    .chain
                    .iter()
                    .filter(|link| link.took.is_some())
                    .collect();
                if !waited.is_empty() {
                    rows.push(PerformanceRow::Heading(
                        "The login screen waited for".into(),
                    ));
                    for link in waited {
                        rows.push(PerformanceRow::Unit(
                            link.name.clone(),
                            performance::shown(link.took.unwrap_or(0)),
                        ));
                    }
                }
                if !boot.slowest.is_empty() {
                    rows.push(PerformanceRow::Heading(
                        "Slowest services since the boot".into(),
                    ));
                    for timed in &boot.slowest {
                        rows.push(PerformanceRow::Unit(
                            timed.name.clone(),
                            performance::shown(timed.took),
                        ));
                    }
                }
            }
            let now = &found.now;
            rows.push(PerformanceRow::Heading("Now".into()));
            if let Some([one, five, fifteen]) = now.load {
                rows.push(PerformanceRow::Text(format!(
                    "Load {one:.2} {five:.2} {fifteen:.2} on {} processors",
                    now.cpus
                )));
            }
            if now.memory_total > 0 {
                rows.push(PerformanceRow::Text(format!(
                    "Memory: {} free of {}",
                    storage::shown(now.memory_available),
                    storage::shown(now.memory_total)
                )));
            }
            rows.push(PerformanceRow::Text(if now.swaps.is_empty() {
                "Swap: none".to_string()
            } else {
                format!(
                    "Swap: {} used of {}{}",
                    storage::shown(now.swap_used()),
                    storage::shown(now.swap_total()),
                    if now.swaps.iter().any(|swap| swap.zram) {
                        " (zram)"
                    } else {
                        ""
                    }
                )
            }));
            let pressure: Vec<String> = [
                ("processor", now.pressure.cpu),
                ("memory", now.pressure.memory),
                ("disk", now.pressure.io),
            ]
            .into_iter()
            .filter_map(|(what, waited)| Some(format!("{what} {:.0}%", waited?)))
            .collect();
            if !pressure.is_empty() {
                rows.push(PerformanceRow::Text(format!(
                    "Waiting in the last minute: {}",
                    pressure.join(", ")
                )));
            }
            if let Some(governor) = &now.governor {
                rows.push(PerformanceRow::Text(format!(
                    "Governor {governor}{}",
                    now.power_profile
                        .as_ref()
                        .map(|profile| format!(", power profile {profile}"))
                        .unwrap_or_default()
                )));
            }
            if !now.heaviest.is_empty() {
                rows.push(PerformanceRow::Text(format!(
                    "Most memory: {}",
                    now.heaviest
                        .iter()
                        .take(5)
                        .map(|process| format!(
                            "{} {}",
                            helix_toolbox_core::printable(&process.name),
                            storage::shown(process.memory)
                        ))
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
            if !found.hints.is_empty() {
                rows.push(PerformanceRow::Heading("Worth knowing".into()));
                rows.extend((0..found.hints.len()).map(PerformanceRow::Hint));
            }
        }
        self.performance_rows = rows;
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
