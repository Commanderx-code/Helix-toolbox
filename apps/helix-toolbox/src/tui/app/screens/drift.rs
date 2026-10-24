//! The Drift screen.

use super::super::*;
use super::{Ask, Cx, ToolScreen, View};

/// How the machine differs from a saved baseline or a profile.
#[derive(Default)]
pub(in crate::tui::app) struct Drift {
    /// What it can compare with: saved baselines, then profiles.
    pub(in crate::tui::app) targets: Vec<DriftTarget>,
    /// The one chosen.
    pub(in crate::tui::app) target: usize,
    /// The last comparison, with the target's key.
    pub(in crate::tui::app) report: Option<(String, drift::Report)>,
}

/// What Drift compares the machine with.
#[derive(Clone, Debug)]
pub(in crate::tui::app) enum DriftTarget {
    Baseline {
        slug: String,
        baseline: drift::Baseline,
    },
    Profile {
        id: String,
        name: String,
        entries: Vec<String>,
    },
}

impl DriftTarget {
    fn key(&self) -> String {
        match self {
            DriftTarget::Baseline { slug, .. } => format!("baseline:{slug}"),
            DriftTarget::Profile { id, .. } => format!("profile:{id}"),
        }
    }
}

/// Whether queueing a difference's entry would put back what it names.
fn restorable(kind: drift::Kind) -> bool {
    !matches!(kind, drift::Kind::Added | drift::Kind::Version)
}

impl Drift {
    /// Read the baselines and profiles again, keep the chosen one, compare.
    fn reload(&mut self, cx: &mut Cx) {
        let chosen = self.targets.get(self.target).map(DriftTarget::key);
        let mut targets: Vec<DriftTarget> = cx
            .host
            .baselines()
            .map(|directory| drift::load_all(&directory).0)
            .unwrap_or_default()
            .into_iter()
            .map(|(slug, baseline)| DriftTarget::Baseline { slug, baseline })
            .collect();
        if let Some(tab) = cx.catalog.tabs.iter().find(|tab| tab.id == "profiles") {
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
        self.target = chosen
            .and_then(|key| targets.iter().position(|target| target.key() == key))
            .unwrap_or(0);
        self.targets = targets;
        self.check(cx);
    }

    /// Compare with the chosen target, from the last look at what is
    /// installed; nothing is compared before that look ends.
    pub(in crate::tui::app) fn check(&mut self, cx: &mut Cx) {
        let Some(target) = self.targets.get(self.target).cloned() else {
            self.report = None;
            return;
        };
        let Some(inventory) = cx.inventory else {
            self.report = None;
            return;
        };
        let sources = drift::Sources::of(cx.host, cx.catalog, cx.platform, inventory);
        let report = match &target {
            DriftTarget::Baseline { baseline, .. } => drift::compare(baseline, &sources),
            DriftTarget::Profile { name, entries, .. } => {
                drift::compare_profile(name, entries, &sources)
            }
        };
        drop(sources);
        self.report = Some((target.key(), report));
        if cx.screen == Screen::Drift {
            cx.clamp();
        }
    }

    fn next_target(&mut self, cx: &mut Cx, step: isize) {
        if self.targets.len() < 2 {
            return;
        }
        let count = self.targets.len() as isize;
        self.target = (self.target as isize + step).rem_euclid(count) as usize;
        cx.selected.select(Some(0));
        self.check(cx);
    }

    fn selected(&self, at: Option<usize>) -> Option<&drift::Difference> {
        let (_, report) = self.report.as_ref()?;
        report.differences.get(at?)
    }

    fn describe(&self, cx: &mut Cx) {
        let Some(difference) = self.selected(cx.at()) else {
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
            .and_then(|id| cx.catalog.find(id))
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
        *cx.popup = Some(Popup::Text(TextPopup::new("Drift", text, true)));
    }

    /// Queue every entry that puts back something missing, and show the
    /// Install Plan to review them.
    fn queue_missing(&self, cx: &mut Cx) {
        let Some((_, report)) = &self.report else {
            return;
        };
        let (restore, unavailable) =
            crate::drift::available(&report.restore(), cx.catalog, cx.platform);
        let mut added = Vec::new();
        for id in &restore {
            if let Some(entry) = cx.catalog.find(id).cloned()
                && entry.multi_select
                && !cx.queue.iter().any(|other| other.id == entry.id)
            {
                added.push(entry.name.clone());
                cx.queue.push(entry);
            }
        }
        let alone: Vec<String> = restore
            .iter()
            .filter_map(|id| cx.catalog.find(id))
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
                cx.platform.pretty_name,
                unavailable.join(", ")
            ));
        }
        if !added.is_empty() {
            cx.ask(Ask::Open(Screen::Plan));
        }
        *cx.popup = Some(Popup::Text(TextPopup::new("Install Plan", text, true)));
    }

    /// Review every entry that puts back something missing as one run, as
    /// the window's Run now does; nothing runs before the confirmation.
    fn run_missing(&self, cx: &mut Cx) {
        let Some((_, report)) = &self.report else {
            return;
        };
        let (restore, _) = crate::drift::available(&report.restore(), cx.catalog, cx.platform);
        let entries: Vec<Entry> = restore
            .iter()
            .filter_map(|id| cx.catalog.find(id))
            .cloned()
            .collect();
        if entries.is_empty() {
            *cx.popup = Some(Popup::Text(TextPopup::new(
                "Nothing to put back",
                "No entry here puts back what differs.",
                true,
            )));
            return;
        }
        cx.ask(Ask::Run(entries));
    }

    /// Record this machine as a baseline and compare with it.
    pub(in crate::tui::app) fn save_baseline(&mut self, cx: &mut Cx, name: &str) {
        let Some(inventory) = cx.inventory else {
            *cx.popup = Some(Popup::Text(TextPopup::new(
                "Still checking",
                "Helix Toolbox is still checking what is installed; try again in a moment.",
                true,
            )));
            return;
        };
        let sources = drift::Sources::of(cx.host, cx.catalog, cx.platform, inventory);
        let baseline = drift::capture(name, cx.host.now(), &sources, &[]);
        drop(sources);
        let saved = cx
            .host
            .baselines()
            .ok_or_else(|| "cannot find the configuration folder".to_string())
            .and_then(|directory| {
                drift::save(&directory, &baseline).map_err(|error| error.to_string())
            });
        let said = match saved {
            Ok(path) => {
                let slug = path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.reload(cx);
                if let Some(position) = self.targets.iter().position(|target| {
                    matches!(target, DriftTarget::Baseline { slug: known, .. } if *known == slug)
                }) {
                    self.target = position;
                    self.check(cx);
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
        };
        *cx.popup = Some(Popup::Text(said));
    }
}

impl ToolScreen for Drift {
    fn open(&mut self, cx: &mut Cx) {
        self.reload(cx);
    }

    fn len(&self, _view: &View) -> usize {
        self.report
            .as_ref()
            .map_or(0, |(_, report)| report.differences.len())
    }

    fn key(&mut self, cx: &mut Cx, code: KeyCode) {
        match code {
            KeyCode::Enter | KeyCode::Char('d') => self.describe(cx),
            KeyCode::Char(' ') => {
                if let Some(difference) = self.selected(cx.at())
                    && restorable(difference.kind)
                    && let Some(entry) = difference
                        .entry
                        .as_deref()
                        .and_then(|id| cx.catalog.find(id))
                        .cloned()
                {
                    cx.toggle_queued(entry);
                }
            }
            KeyCode::Char('a') => self.queue_missing(cx),
            KeyCode::Char('R') => self.run_missing(cx),
            KeyCode::Char('b') => self.next_target(cx, 1),
            KeyCode::Char('B') => self.next_target(cx, -1),
            KeyCode::Char('s') => {
                *cx.popup = Some(Popup::BaselineName(InputPopup::new(
                    "Save this machine as a baseline",
                    "A name for the baseline (it is saved in ~/.config/helix-toolbox/baselines):",
                )));
            }
            KeyCode::Char('r') => {
                self.reload(cx);
                cx.ask(Ask::CheckStatuses);
            }
            KeyCode::Char('h') | KeyCode::Left => cx.ask(Ask::Sidebar),
            _ => {}
        }
    }

    fn hints(&self, _view: &View, at: Option<usize>, list: &mut Vec<Hint>) {
        list.push(Hint::new(&["↑↓"], "Navigate"));
        if let Some(difference) = self.selected(at) {
            list.push(Hint::new(&["Enter"], "Details"));
            if difference.entry.is_some() && restorable(difference.kind) {
                list.push(Hint::new(&["Space"], "Plan"));
            }
        }
        if self
            .report
            .as_ref()
            .is_some_and(|(_, report)| !report.restore().is_empty())
        {
            list.push(Hint::new(&["a"], "Plan all missing"));
            list.push(Hint::new(&["R"], "Run them now"));
        }
        if self.targets.len() > 1 {
            list.push(Hint::new(&["b"], "Compare with"));
        }
        list.push(Hint::new(&["s"], "Save baseline"));
        list.push(Hint::new(&["r"], "Check again"));
    }

    fn draw(&self, view: &View, frame: &mut Frame, area: Rect, state: &mut ListState) {
        let inner = view.page(
            frame,
            area,
            "Drift",
            "How this machine differs from a saved baseline or a profile. It only reads.",
        );
        let target = self.targets.get(self.target);
        let mut lines: Vec<Line> = Vec::new();
        match target {
            None => lines.push(Line::from(
                "No baseline saved yet, and no profile to compare with.",
            )),
            Some(DriftTarget::Baseline { baseline, .. }) => lines.push(Line::from(vec![
                Span::styled("Baseline  ", Style::default().fg(view.theme.muted())),
                Span::styled(
                    helix_toolbox_core::printable(&baseline.name),
                    Style::default().fg(view.theme.entry()).bold(),
                ),
                Span::styled(
                    format!(
                        " · saved {} on {}",
                        timestamp(baseline.taken),
                        helix_toolbox_core::printable(&baseline.system)
                    ),
                    Style::default().fg(view.theme.muted()),
                ),
            ])),
            Some(DriftTarget::Profile { name, .. }) => lines.push(Line::from(vec![
                Span::styled("Profile  ", Style::default().fg(view.theme.muted())),
                Span::styled(name.clone(), Style::default().fg(view.theme.entry()).bold()),
                Span::styled(
                    " · is each of its entries here?",
                    Style::default().fg(view.theme.muted()),
                ),
            ])),
        }
        let summary = match &self.report {
            _ if target.is_none() => None,
            None => Some(Line::styled(
                "Checking what is installed…",
                Style::default().fg(view.theme.muted()),
            )),
            Some((_, report)) if report.differences.is_empty() => Some(Line::styled(
                format!(
                    "No differences: {} things are as they were.",
                    report.matched
                ),
                Style::default().fg(view.theme.success()),
            )),
            Some((_, report)) => {
                let counts: Vec<String> = report
                    .counts()
                    .into_iter()
                    .map(|(label, count)| format!("{count} {}", label.to_lowercase()))
                    .collect();
                Some(Line::styled(
                    format!("{} · {} the same", counts.join(" · "), report.matched),
                    Style::default().fg(view.theme.warning()),
                ))
            }
        };
        lines.extend(summary);
        if let Some((_, report)) = &self.report
            && target.is_some()
        {
            if !report.unknown.is_empty() {
                lines.push(Line::styled(
                    format!("Not compared: {}", report.unknown.join(", ")),
                    Style::default().fg(view.theme.muted()),
                ));
            }
            let (_, unavailable) =
                crate::drift::available(&report.restore(), view.catalog, view.platform);
            if !unavailable.is_empty() {
                let names: Vec<&str> = unavailable
                    .iter()
                    .map(|id| {
                        view.catalog
                            .find(id)
                            .map_or(id.as_str(), |entry| &entry.name)
                    })
                    .collect();
                lines.push(Line::styled(
                    format!("Not on this machine: {}", names.join(", ")),
                    Style::default().fg(view.theme.muted()),
                ));
            }
        }
        lines.push(Line::styled(
            match self.targets.len() {
                0 | 1 => "s saves this machine as a baseline".to_string(),
                count => format!(
                    "b: another baseline or profile ({} of {count}) · s: save this machine",
                    self.target + 1
                ),
            },
            Style::default().fg(view.theme.faint()),
        ));
        let parts = Layout::vertical([
            Constraint::Length(lines.len() as u16 + 2),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);
        view.fill(frame, parts[0]);
        frame.render_widget(
            Paragraph::new(Text::from(lines))
                .block(view.card("Compared with"))
                .wrap(Wrap { trim: true }),
            parts[0],
        );
        view.fill(frame, parts[2]);
        let Some((_, report)) = &self.report else {
            return;
        };
        if report.differences.is_empty() {
            frame.render_widget(
                Paragraph::new("Everything recorded is as it was.")
                    .block(view.card("Differences"))
                    .style(Style::default().fg(view.theme.muted()))
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
        let focused = view.focused;
        let items: Vec<Line> = report
            .differences
            .iter()
            .map(|difference| {
                let color = match difference.kind {
                    kind if kind.missing() => view.theme.failure(),
                    drift::Kind::Added | drift::Kind::Version => view.theme.muted(),
                    _ => view.theme.warning(),
                };
                let queued = difference
                    .entry
                    .as_deref()
                    .is_some_and(|id| view.queue.iter().any(|entry| entry.id == id))
                    && restorable(difference.kind);
                let subject: String = difference.subject.chars().take(36).collect();
                Line::from(vec![
                    Span::styled(
                        format!("{:<10}", difference.kind.label()),
                        Style::default().fg(color).bold(),
                    ),
                    Span::styled(
                        format!("{subject:<subject_width$}"),
                        Style::default().fg(view.theme.entry()),
                    ),
                    Span::styled(
                        difference.note.clone(),
                        Style::default().fg(view.theme.muted()),
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
                        Style::default().fg(view.theme.faint()),
                    ),
                    Span::styled(
                        if queued { "  · planned" } else { "" },
                        Style::default().fg(view.theme.success()),
                    ),
                ])
            })
            .collect();
        let list = List::new(view.measure(items))
            .block(view.card("Differences"))
            .highlight_style(view.highlight(focused))
            .highlight_symbol(view.marker(focused));
        bordered_list(frame, list, parts[2], state, view.hits);
    }
}
