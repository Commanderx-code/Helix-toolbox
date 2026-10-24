//! The Drift screen.

use super::super::*;

impl App {
    pub(in crate::tui::app) fn drift_dir(&self) -> Option<std::path::PathBuf> {
        self.baselines_dir.clone().or_else(drift::dir)
    }

    /// Read the baselines and profiles again, keep the chosen one, compare.
    pub(in crate::tui::app) fn reload_drift(&mut self) {
        let chosen = self
            .drift_targets
            .get(self.drift_target)
            .map(DriftTarget::key);
        let mut targets: Vec<DriftTarget> = self
            .drift_dir()
            .map(|directory| drift::load_all(&directory).0)
            .unwrap_or_default()
            .into_iter()
            .map(|(slug, baseline)| DriftTarget::Baseline { slug, baseline })
            .collect();
        if let Some(tab) = self.catalog.tabs.iter().find(|tab| tab.id == "profiles") {
            for entry in tab.entries() {
                if let Action::Profile { entries } = &entry.action {
                    targets.push(DriftTarget::Profile {
                        id: entry.id.clone(),
                        name: entry.name.clone(),
                        entries: entries.clone(),
                    });
                }
            }
        }
        self.drift_target = chosen
            .and_then(|key| targets.iter().position(|target| target.key() == key))
            .unwrap_or(0);
        self.drift_targets = targets;
        self.check_drift();
    }

    /// Compare with the chosen target, from the last look at what is
    /// installed; nothing is compared before that look ends.
    pub(in crate::tui::app) fn check_drift(&mut self) {
        let Some(target) = self.drift_targets.get(self.drift_target).cloned() else {
            self.drift_report = None;
            return;
        };
        let Some(inventory) = &self.inventory else {
            self.drift_report = None;
            return;
        };
        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_default();
        let (records, settings) = if self.examine_here {
            (
                history::read(1000).unwrap_or_default(),
                drift::settings_here(&doctor::Here),
            )
        } else {
            (Vec::new(), Default::default())
        };
        let settings = move || settings.clone();
        let scripts = self.catalog.root().join("scripts");
        let examine_here = self.examine_here;
        let render = move |arguments: &[String]| {
            if examine_here {
                helix_toolbox_engine::preview::render_with(&scripts, arguments)
            } else {
                Err("not this machine".to_string())
            }
        };
        let sources = drift::Sources {
            catalog: &self.catalog,
            platform: &self.platform,
            inventory,
            on_path: if self.examine_here { &which } else { &|_| None },
            system: &doctor::Here,
            home: &home,
            history: &records,
            settings: &settings,
            render: &render,
        };
        let report = match &target {
            DriftTarget::Baseline { baseline, .. } => drift::compare(baseline, &sources),
            DriftTarget::Profile { name, entries, .. } => {
                drift::compare_profile(name, entries, &sources)
            }
        };
        self.drift_report = Some((target.key(), report));
        if self.screen == Screen::Drift {
            self.clamp_main();
        }
    }

    pub(in crate::tui::app) fn next_drift_target(&mut self, step: isize) {
        if self.drift_targets.len() < 2 {
            return;
        }
        let count = self.drift_targets.len() as isize;
        self.drift_target = (self.drift_target as isize + step).rem_euclid(count) as usize;
        self.main_state.select(Some(0));
        self.check_drift();
    }

    pub(in crate::tui::app) fn selected_difference(&self) -> Option<&drift::Difference> {
        let (_, report) = self.drift_report.as_ref()?;
        report.differences.get(self.main_state.selected()?)
    }

    pub(in crate::tui::app) fn describe_difference(&mut self) {
        let Some(difference) = self.selected_difference() else {
            return;
        };
        let mut text = format!(
            "{}: {}\n{}\n\n{}\n",
            difference.kind.label(),
            difference.subject,
            difference.note,
            difference.kind.explain()
        );
        if let Some(was) = &difference.was {
            text.push_str(&format!("\nIn the baseline: {was}"));
        }
        if let Some(now) = &difference.now {
            text.push_str(&format!("\nHere: {now}"));
        }
        if let Some(entry) = difference
            .entry
            .as_deref()
            .and_then(|id| self.catalog.find(id))
            && restorable(difference.kind)
        {
            text.push_str(&format!(
                "\n\nThe {} entry puts it back: Space adds it to the Install Plan, where it is planned and confirmed before anything runs.",
                entry.name
            ));
            if matches!(
                difference.kind,
                drift::Kind::FileChanged | drift::Kind::FileReplaced
            ) {
                text.push_str(" Running it replaces what is there now, keeping a backup.");
            }
        }
        self.popup = Some(Popup::Text(TextPopup::new("Drift", text, true)));
    }

    /// Queue every entry that puts back something missing, and show the
    /// Install Plan to review them.
    pub(in crate::tui::app) fn queue_missing(&mut self) {
        let Some((_, report)) = &self.drift_report else {
            return;
        };
        let (restore, unavailable) =
            crate::drift::available(&report.restore(), &self.catalog, &self.platform);
        let mut added = Vec::new();
        for id in &restore {
            if let Some(entry) = self.catalog.find(id).cloned()
                && entry.multi_select
                && !self.is_queued(&entry)
            {
                added.push(entry.name.clone());
                self.queue.push(entry);
            }
        }
        let alone: Vec<String> = restore
            .iter()
            .filter_map(|id| self.catalog.find(id))
            .filter(|entry| !entry.multi_select)
            .map(|entry| entry.name.clone())
            .collect();
        let mut text = if added.is_empty() {
            "Nothing new was added to the Install Plan.".to_string()
        } else {
            format!(
                "Added to the Install Plan: {}.\n\nNothing has run: review the plan there and press Enter to run it.",
                added.join(", ")
            )
        };
        if !alone.is_empty() {
            text.push_str(&format!(
                "\n\nThese run on their own, so run each from its area: {}.",
                alone.join(", ")
            ));
        }
        if !unavailable.is_empty() {
            text.push_str(&format!(
                "\n\nNot available on {}: {}.",
                self.platform.pretty_name,
                unavailable.join(", ")
            ));
        }
        if !added.is_empty() {
            self.open_screen(Screen::Plan);
            self.focus = Focus::Main;
        }
        self.popup = Some(Popup::Text(TextPopup::new("Install Plan", text, true)));
    }

    /// Review every entry that puts back something missing as one run, as
    /// the window's Run now does; nothing runs before the confirmation.
    pub(in crate::tui::app) fn run_missing(&mut self) {
        let Some((_, report)) = &self.drift_report else {
            return;
        };
        let (restore, _) =
            crate::drift::available(&report.restore(), &self.catalog, &self.platform);
        let entries: Vec<Entry> = restore
            .iter()
            .filter_map(|id| self.catalog.find(id))
            .cloned()
            .collect();
        if entries.is_empty() {
            self.popup = Some(Popup::Text(TextPopup::new(
                "Nothing to put back",
                "No entry here puts back what differs.",
                true,
            )));
            return;
        }
        self.confirm(entries);
    }

    /// Record this machine as a baseline and compare with it.
    pub(in crate::tui::app) fn save_baseline(&mut self, name: &str) {
        let Some(inventory) = &self.inventory else {
            self.popup = Some(Popup::Text(TextPopup::new(
                "Still checking",
                "Helix Toolbox is still checking what is installed; try again in a moment.",
                true,
            )));
            return;
        };
        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_default();
        let (records, settings) = if self.examine_here {
            (
                history::read(1000).unwrap_or_default(),
                drift::settings_here(&doctor::Here),
            )
        } else {
            (Vec::new(), Default::default())
        };
        let settings = move || settings.clone();
        let scripts = self.catalog.root().join("scripts");
        let examine_here = self.examine_here;
        let render = move |arguments: &[String]| {
            if examine_here {
                helix_toolbox_engine::preview::render_with(&scripts, arguments)
            } else {
                Err("not this machine".to_string())
            }
        };
        let sources = drift::Sources {
            catalog: &self.catalog,
            platform: &self.platform,
            inventory,
            on_path: if self.examine_here { &which } else { &|_| None },
            system: &doctor::Here,
            home: &home,
            history: &records,
            settings: &settings,
            render: &render,
        };
        let baseline = drift::capture(name, history::now(), &sources, &[]);
        let saved = self
            .drift_dir()
            .ok_or_else(|| "cannot find the configuration folder".to_string())
            .and_then(|directory| {
                drift::save(&directory, &baseline).map_err(|error| error.to_string())
            });
        self.popup = Some(Popup::Text(match saved {
            Ok(path) => {
                let slug = path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.reload_drift();
                if let Some(position) = self.drift_targets.iter().position(|target| {
                    matches!(target, DriftTarget::Baseline { slug: known, .. } if *known == slug)
                }) {
                    self.drift_target = position;
                    self.check_drift();
                }
                TextPopup::new(
                    "Baseline saved",
                    format!(
                        "{} is saved as {}: {} entries, {} configuration paths, {} settings and {} runs.\n\nDrift compares with it from now on; b chooses another. To compare another machine with it: helix-toolbox drift export \"{}\", then helix-toolbox drift check <file> there.",
                        baseline.name,
                        path.display(),
                        baseline.entries.len(),
                        baseline.files.len(),
                        baseline.settings.len(),
                        baseline.applied.len(),
                        baseline.name
                    ),
                    true,
                )
            }
            Err(error) => TextPopup::new("Baseline not saved", error, true),
        }));
    }

    pub(in crate::tui::app) fn draw_drift(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Drift",
            "How this machine differs from a saved baseline or a profile. It only reads.",
        );
        let target = self.drift_targets.get(self.drift_target);
        let mut lines: Vec<Line> = Vec::new();
        match target {
            None => lines.push(Line::from(
                "No baseline saved yet, and no profile to compare with.",
            )),
            Some(DriftTarget::Baseline { baseline, .. }) => lines.push(Line::from(vec![
                Span::styled("Baseline  ", Style::default().fg(self.theme.muted())),
                Span::styled(
                    helix_toolbox_core::printable(&baseline.name),
                    Style::default().fg(self.theme.entry()).bold(),
                ),
                Span::styled(
                    format!(
                        " · saved {} on {}",
                        timestamp(baseline.taken),
                        helix_toolbox_core::printable(&baseline.system)
                    ),
                    Style::default().fg(self.theme.muted()),
                ),
            ])),
            Some(DriftTarget::Profile { name, .. }) => lines.push(Line::from(vec![
                Span::styled("Profile  ", Style::default().fg(self.theme.muted())),
                Span::styled(name.clone(), Style::default().fg(self.theme.entry()).bold()),
                Span::styled(
                    " · is each of its entries here?",
                    Style::default().fg(self.theme.muted()),
                ),
            ])),
        }
        let summary = match &self.drift_report {
            _ if target.is_none() => None,
            None => Some(Line::styled(
                "Checking what is installed…",
                Style::default().fg(self.theme.muted()),
            )),
            Some((_, report)) if report.differences.is_empty() => Some(Line::styled(
                format!(
                    "No differences: {} things are as they were.",
                    report.matched
                ),
                Style::default().fg(self.theme.success()),
            )),
            Some((_, report)) => {
                let counts: Vec<String> = report
                    .counts()
                    .into_iter()
                    .map(|(label, count)| format!("{count} {}", label.to_lowercase()))
                    .collect();
                Some(Line::styled(
                    format!("{} · {} the same", counts.join(" · "), report.matched),
                    Style::default().fg(self.theme.warning()),
                ))
            }
        };
        lines.extend(summary);
        if let Some((_, report)) = &self.drift_report
            && target.is_some()
        {
            if !report.unknown.is_empty() {
                lines.push(Line::styled(
                    format!("Not compared: {}", report.unknown.join(", ")),
                    Style::default().fg(self.theme.muted()),
                ));
            }
            let (_, unavailable) =
                crate::drift::available(&report.restore(), &self.catalog, &self.platform);
            if !unavailable.is_empty() {
                let names: Vec<&str> = unavailable
                    .iter()
                    .map(|id| {
                        self.catalog
                            .find(id)
                            .map_or(id.as_str(), |entry| &entry.name)
                    })
                    .collect();
                lines.push(Line::styled(
                    format!("Not on this machine: {}", names.join(", ")),
                    Style::default().fg(self.theme.muted()),
                ));
            }
        }
        lines.push(Line::styled(
            match self.drift_targets.len() {
                0 | 1 => "s saves this machine as a baseline".to_string(),
                count => format!(
                    "b: another baseline or profile ({} of {count}) · s: save this machine",
                    self.drift_target + 1
                ),
            },
            Style::default().fg(self.theme.faint()),
        ));
        let parts = Layout::vertical([
            Constraint::Length(lines.len() as u16 + 2),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);
        self.fill(frame, parts[0]);
        frame.render_widget(
            Paragraph::new(Text::from(lines))
                .block(self.card("Compared with"))
                .wrap(Wrap { trim: true }),
            parts[0],
        );
        self.fill(frame, parts[2]);
        let Some((_, report)) = &self.drift_report else {
            return;
        };
        if report.differences.is_empty() {
            frame.render_widget(
                Paragraph::new("Everything recorded is as it was.")
                    .block(self.card("Differences"))
                    .style(Style::default().fg(self.theme.muted()))
                    .wrap(Wrap { trim: true }),
                parts[2],
            );
            return;
        }
        let subject_width = report
            .differences
            .iter()
            .map(|difference| difference.subject.chars().count())
            .max()
            .unwrap_or(0)
            .min(36)
            + 2;
        let focused = self.focus == Focus::Main;
        let items: Vec<Line> = report
            .differences
            .iter()
            .map(|difference| {
                let color = match difference.kind {
                    kind if kind.missing() => self.theme.failure(),
                    drift::Kind::Added | drift::Kind::Version => self.theme.muted(),
                    _ => self.theme.warning(),
                };
                let queued = difference
                    .entry
                    .as_deref()
                    .is_some_and(|id| self.queue.iter().any(|entry| entry.id == id))
                    && restorable(difference.kind);
                let subject: String = difference.subject.chars().take(36).collect();
                Line::from(vec![
                    Span::styled(
                        format!("{:<10}", difference.kind.label()),
                        Style::default().fg(color).bold(),
                    ),
                    Span::styled(
                        format!("{subject:<subject_width$}"),
                        Style::default().fg(self.theme.entry()),
                    ),
                    Span::styled(
                        difference.note.clone(),
                        Style::default().fg(self.theme.muted()),
                    ),
                    // Was and now, as the window's columns show them.
                    Span::styled(
                        if difference.was.is_some() || difference.now.is_some() {
                            format!(
                                "  {} → {}",
                                difference.was.as_deref().unwrap_or("–"),
                                difference.now.as_deref().unwrap_or("–")
                            )
                        } else {
                            String::new()
                        },
                        Style::default().fg(self.theme.faint()),
                    ),
                    Span::styled(
                        if queued { "  · planned" } else { "" },
                        Style::default().fg(self.theme.success()),
                    ),
                ])
            })
            .collect();
        let list = List::new(self.measure(items))
            .block(self.card("Differences"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused));
        bordered_list(frame, list, parts[2], &mut self.main_state, &self.hits);
    }
}
