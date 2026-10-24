//! The Doctor screen.

use super::super::*;
use super::{Ask, Cx, Read, ToolScreen, View};

/// The health checks, with the Helix Doctor area's reports below them.
#[derive(Default)]
pub(in crate::tui::app) struct Doctor {
    /// What the last health check found.
    pub(in crate::tui::app) diagnoses: Vec<doctor::Diagnosis>,
    /// A health check running in the background.
    examining: Read<Vec<doctor::Diagnosis>>,
    /// A check to show once the checks finish, from the palette.
    pub(in crate::tui::app) wanted: Option<&'static str>,
    /// A run while another screen was shown left what it found out of
    /// date.
    stale: bool,
}

/// The Helix Doctor area's entries, shown below the checks.
fn reports(catalog: &Catalog) -> Vec<&Entry> {
    catalog
        .tabs
        .iter()
        .find(|tab| tab.id == "doctor")
        .map(|tab| tab.entries())
        .unwrap_or_default()
}

/// A check in full: what was found, what to do, and what f runs; on the
/// Recovery screen, what c does with the snapshots.
pub(in crate::tui::app) fn diagnosis_popup(
    diagnosis: &doctor::Diagnosis,
    catalog: &Catalog,
    recovery: bool,
) -> Popup {
    let mut text = format!("{}: {}\n", diagnosis.health.label(), diagnosis.summary);
    if !diagnosis.evidence.is_empty() {
        text.push_str("\nWhat was found:\n");
        for line in &diagnosis.evidence {
            text.push_str(&format!("  {line}\n"));
        }
    }
    if let Some(advice) = &diagnosis.advice {
        text.push_str(&format!("\nWhat to do: {advice}\n"));
    }
    if let Some(repair) = &diagnosis.repair {
        text.push_str(&format!(
            "\nRepair: {}. f runs it after its own confirmation:\n",
            repair.label
        ));
        for command in &repair.commands {
            text.push_str(&format!("  {}\n", command.join(" ")));
        }
    } else if let Some(entry) = diagnosis.entry.as_deref().and_then(|id| catalog.find(id)) {
        text.push_str(&format!(
            "\nThe {} entry addresses it; f runs it.\n",
            entry.name
        ));
    }
    if diagnosis.id == "snapshots" && recovery {
        text.push_str(
            "\nc lists the snapshots as root and looks inside the newest, reading only.\n",
        );
    }
    Popup::Text(TextPopup::new(diagnosis.title.clone(), text, true))
}

impl Doctor {
    /// Run the health checks in the background.
    fn examine(&mut self, cx: &Cx) {
        let manager = cx.platform.manager;
        self.examining
            .start(cx.host, move |host| doctor::examine_on(&host, manager));
    }

    fn selected(&self, at: Option<usize>) -> Option<&doctor::Diagnosis> {
        at.and_then(|index| self.diagnoses.get(index))
    }

    fn describe(&self, cx: &mut Cx) {
        if let Some(diagnosis) = self.selected(cx.at()) {
            *cx.popup = Some(diagnosis_popup(
                diagnosis,
                cx.catalog,
                cx.screen == Screen::Recovery,
            ));
        }
    }

    /// The rows above the reports: the checks, or the line that says they
    /// are running or how to run them.
    fn checks(&self) -> usize {
        self.diagnoses.len().max(1)
    }

    /// The report the selected row is, below the checks.
    fn selected_report<'a>(&self, catalog: &'a Catalog, at: Option<usize>) -> Option<&'a Entry> {
        let index = at?.checked_sub(self.checks())?;
        reports(catalog).into_iter().nth(index)
    }

    /// Run the selected check's repair, or the entry that addresses it.
    fn repair_selected(&self, cx: &mut Cx) {
        let Some(diagnosis) = self.selected(cx.at()).cloned() else {
            return;
        };
        if let Some(repair) = &diagnosis.repair {
            cx.confirm(repair.plan(&diagnosis.id));
        } else if let Some(entry) = diagnosis
            .entry
            .as_deref()
            .and_then(|id| cx.catalog.find(id))
            .cloned()
        {
            cx.ask(Ask::Run(vec![entry]));
        }
    }

    /// Opened from the palette: show the check `wanted` once there are
    /// checks to show, or check again.
    pub(in crate::tui::app) fn want(&mut self, cx: &mut Cx, wanted: Option<&'static str>) {
        if let Some(wanted) = wanted {
            if self.diagnoses.is_empty() || self.examining.busy() {
                self.wanted = Some(wanted);
            } else {
                self.show_check(cx, wanted);
            }
        } else if !self.diagnoses.is_empty() {
            self.examine(cx);
        }
    }

    /// Select a health check by id (or the first whose id starts with it)
    /// and show its details.
    pub(in crate::tui::app) fn show_check(&self, cx: &mut Cx, wanted: &str) {
        let found = self
            .diagnoses
            .iter()
            .position(|diagnosis| diagnosis.id == wanted)
            .or_else(|| {
                self.diagnoses
                    .iter()
                    .position(|diagnosis| diagnosis.id.starts_with(wanted))
            });
        if let Some(index) = found {
            cx.selected.select(Some(index));
            self.describe(cx);
        }
    }
}

impl ToolScreen for Doctor {
    fn open(&mut self, cx: &mut Cx) {
        if std::mem::take(&mut self.stale) || self.diagnoses.is_empty() {
            self.examine(cx);
        } else if let Some(wanted) = self.wanted.take() {
            // Asked for from the palette, but the checks answered after the
            // screen was left.
            self.show_check(cx, wanted);
        }
    }

    fn len(&self, view: &View) -> usize {
        self.checks() + reports(view.catalog).len()
    }

    fn key(&mut self, cx: &mut Cx, code: KeyCode) {
        match code {
            KeyCode::Enter => match self.selected(cx.at()) {
                Some(_) => self.describe(cx),
                None => {
                    if let Some(entry) = self.selected_report(cx.catalog, cx.at()).cloned() {
                        cx.ask(Ask::Run(vec![entry]));
                    }
                }
            },
            KeyCode::Char('f') => self.repair_selected(cx),
            KeyCode::Char('r') => self.examine(cx),
            KeyCode::Char('d') | KeyCode::Char('D') => {
                if self.selected(cx.at()).is_some() {
                    self.describe(cx);
                } else if self.selected_report(cx.catalog, cx.at()).is_some() {
                    cx.ask(Ask::Describe);
                }
            }
            // A report row is an entry like any other.
            KeyCode::Char('p') | KeyCode::Char('P') => cx.ask(Ask::Preview),
            KeyCode::Char('v') => {
                if let Some(entry) = self.selected_report(cx.catalog, cx.at()) {
                    cx.ask(Ask::Inspect(entry.id.clone()));
                }
            }
            KeyCode::Char(' ') => {
                if let Some(entry) = self.selected_report(cx.catalog, cx.at()).cloned() {
                    cx.toggle_queued(entry);
                }
            }
            KeyCode::Char('*') => cx.ask(Ask::Pin),
            KeyCode::Char('h') | KeyCode::Left => cx.ask(Ask::Sidebar),
            _ => {}
        }
    }

    fn hints(&self, view: &View, at: Option<usize>, list: &mut Vec<Hint>) {
        list.push(Hint::new(&["↑↓"], "Navigate"));
        if let Some(diagnosis) = self.selected(at) {
            list.push(Hint::new(&["Enter"], "Details"));
            if diagnosis.repair.is_some() {
                list.push(Hint::new(&["f"], "Repair"));
            } else if diagnosis.entry.is_some() {
                list.push(Hint::new(&["f"], "Run the entry"));
            }
        } else if self.selected_report(view.catalog, at).is_some() {
            list.push(Hint::new(&["Enter"], "Run"));
            list.push(Hint::new(&["Space"], "Plan"));
            list.push(Hint::new(&["d"], "Details"));
        }
        list.push(Hint::new(&["r"], "Check again"));
    }

    fn poll(&mut self, cx: &mut Cx) -> bool {
        let Some(answer) = self.examining.take() else {
            return false;
        };
        if let Ok(diagnoses) = answer {
            self.diagnoses = diagnoses;
        }
        // The selection and the popup are the screen shown: a check asked
        // for waits until this one is shown again.
        if cx.screen == Screen::Doctor {
            cx.clamp();
            if let Some(wanted) = self.wanted.take() {
                cx.ask(Ask::ShowCheck(wanted));
            }
        }
        true
    }

    fn after_run(&mut self, cx: &mut Cx, reading: Reading) {
        if reading == Reading::Tools {
            self.examine(cx);
        }
    }

    fn stale(&mut self, reading: Reading) {
        if reading == Reading::Tools {
            self.stale = true;
        }
    }

    fn entry<'a>(&self, view: &View<'a>, at: Option<usize>) -> Option<&'a Entry> {
        self.selected_report(view.catalog, at)
    }

    fn draw(&self, view: &View, frame: &mut Frame, area: Rect, state: &mut ListState) {
        let inner = view.page(
            frame,
            area,
            "Helix Doctor",
            "The checks only read the system; a repair runs after its own confirmation.",
        );
        let focused = view.focused;
        let mut items: Vec<Line> = Vec::new();
        if self.diagnoses.is_empty() {
            items.push(Line::styled(
                if self.examining.busy() {
                    "Checking…"
                } else if self.examining.failed() {
                    "The checks stopped without an answer. Press r to check again."
                } else {
                    "Press r to check this machine."
                },
                Style::default().fg(view.theme.muted()),
            ));
        }
        // The title column fits the longest title.
        let title_width = self
            .diagnoses
            .iter()
            .map(|diagnosis| diagnosis.title.chars().count())
            .max()
            .unwrap_or(0)
            + 2;
        for diagnosis in &self.diagnoses {
            let color = match diagnosis.health {
                doctor::Health::Good => view.theme.success(),
                doctor::Health::Notice => view.theme.warning(),
                doctor::Health::Problem => view.theme.failure(),
            };
            let offer = if diagnosis.repair.is_some() {
                "  · f repairs"
            } else if diagnosis.entry.is_some() {
                "  · f runs the entry"
            } else {
                ""
            };
            items.push(Line::from(vec![
                Span::styled(
                    format!("{:<8}", diagnosis.health.label()),
                    Style::default().fg(color).bold(),
                ),
                Span::styled(
                    format!("{:<title_width$}", diagnosis.title),
                    Style::default().fg(view.theme.entry()),
                ),
                Span::styled(
                    diagnosis.summary.clone(),
                    Style::default().fg(view.theme.muted()),
                ),
                Span::styled(offer, Style::default().fg(view.theme.focused())),
            ]));
        }
        for entry in reports(view.catalog) {
            // Queued and pinned, marked as in the entry lists.
            let queued = view.queue.iter().any(|other| other.id == entry.id);
            let marks = format!(
                "{}{}",
                if queued { " *" } else { "" },
                if view.favorites.contains(&entry.id) {
                    " ★"
                } else {
                    ""
                }
            );
            let status = view
                .statuses
                .get(&entry.id)
                .map(|status| format!("  {}", status.label()))
                .unwrap_or_default();
            items.push(Line::from(vec![
                Span::styled("Report  ", Style::default().fg(view.theme.group())),
                Span::styled(
                    format!("{}{marks}", entry.name),
                    if queued {
                        Style::default().fg(view.theme.focused()).bold()
                    } else {
                        Style::default().fg(view.theme.entry())
                    },
                ),
                Span::styled(status, Style::default().fg(view.theme.muted())),
            ]));
        }
        view.fill(frame, inner);
        let title = if self.diagnoses.is_empty() {
            "Checks".to_string()
        } else {
            let count = |health: doctor::Health| {
                self.diagnoses
                    .iter()
                    .filter(|diagnosis| diagnosis.health == health)
                    .count()
            };
            let (problems, notices) = (
                count(doctor::Health::Problem),
                count(doctor::Health::Notice),
            );
            format!(
                "Checks · {problems} {} · {notices} {} · {} OK{}",
                if problems == 1 { "problem" } else { "problems" },
                if notices == 1 { "notice" } else { "notices" },
                count(doctor::Health::Good),
                if self.examining.busy() {
                    " (checking again…)"
                } else {
                    ""
                }
            )
        };
        let list = List::new(view.measure(items))
            .block(view.card(&title))
            .highlight_style(view.highlight(focused))
            .highlight_symbol(view.marker(focused));
        bordered_list(frame, list, inner, state, view.hits);
    }
}
