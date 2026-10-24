//! The Doctor screen.

use super::super::*;

impl App {
    /// Run the health checks in the background.
    pub(in crate::tui::app) fn examine(&mut self) {
        if !self.examine_here {
            return;
        }
        let manager = self.platform.manager;
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(doctor::examine(&doctor::Here, manager));
        });
        self.examining = Some(receiver);
    }

    /// The Helix Doctor area's entries, shown below the checks.
    pub(in crate::tui::app) fn doctor_entries(&self) -> Vec<Entry> {
        self.catalog
            .tabs
            .iter()
            .find(|tab| tab.id == "doctor")
            .map(|tab| tab.entries().into_iter().cloned().collect())
            .unwrap_or_default()
    }

    pub(in crate::tui::app) fn selected_diagnosis(&self) -> Option<&doctor::Diagnosis> {
        self.main_state
            .selected()
            .and_then(|index| self.diagnoses.get(index))
    }

    pub(in crate::tui::app) fn describe_diagnosis(&mut self) {
        if let Some(diagnosis) = self.selected_diagnosis().cloned() {
            self.show_diagnosis(&diagnosis);
        }
    }

    /// A check in full: what was found, what to do, and what f runs.
    pub(in crate::tui::app) fn show_diagnosis(&mut self, diagnosis: &doctor::Diagnosis) {
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
        } else if let Some(entry) = diagnosis
            .entry
            .as_deref()
            .and_then(|id| self.catalog.find(id))
        {
            text.push_str(&format!(
                "\nThe {} entry addresses it; f runs it.\n",
                entry.name
            ));
        }
        if diagnosis.id == "snapshots" && self.screen == Screen::Recovery {
            text.push_str(
                "\nc lists the snapshots as root and looks inside the newest, reading only.\n",
            );
        }
        self.popup = Some(Popup::Text(TextPopup::new(
            diagnosis.title.clone(),
            text,
            true,
        )));
    }

    /// Run the selected check's repair, or the entry that addresses it.
    pub(in crate::tui::app) fn repair_selected(&mut self) {
        let Some(diagnosis) = self.selected_diagnosis().cloned() else {
            return;
        };
        if let Some(repair) = &diagnosis.repair {
            self.confirm_plan(Ok(repair.plan(&diagnosis.id)));
        } else if let Some(entry) = diagnosis
            .entry
            .as_deref()
            .and_then(|id| self.catalog.find(id))
            .cloned()
        {
            self.confirm(vec![entry]);
        }
    }

    pub(in crate::tui::app) fn draw_doctor(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Helix Doctor",
            "The checks only read the system; a repair runs after its own confirmation.",
        );
        let focused = self.focus == Focus::Main;
        let mut items: Vec<Line> = Vec::new();
        if self.diagnoses.is_empty() {
            items.push(Line::styled(
                if self.examining.is_some() {
                    "Checking…"
                } else {
                    "Press r to check this machine."
                },
                Style::default().fg(self.theme.muted()),
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
                doctor::Health::Good => self.theme.success(),
                doctor::Health::Notice => self.theme.warning(),
                doctor::Health::Problem => self.theme.failure(),
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
                    Style::default().fg(self.theme.entry()),
                ),
                Span::styled(
                    diagnosis.summary.clone(),
                    Style::default().fg(self.theme.muted()),
                ),
                Span::styled(offer, Style::default().fg(self.theme.focused())),
            ]));
        }
        for entry in self.doctor_entries() {
            // Queued and pinned, marked as in the entry lists.
            let queued = self.is_queued(&entry);
            let marks = format!(
                "{}{}",
                if queued { " *" } else { "" },
                if self.favorites.contains(&entry.id) {
                    " ★"
                } else {
                    ""
                }
            );
            let status = self
                .statuses
                .get(&entry.id)
                .map(|status| format!("  {}", status.label()))
                .unwrap_or_default();
            items.push(Line::from(vec![
                Span::styled("Report  ", Style::default().fg(self.theme.group())),
                Span::styled(
                    format!("{}{marks}", entry.name),
                    if queued {
                        Style::default().fg(self.theme.focused()).bold()
                    } else {
                        Style::default().fg(self.theme.entry())
                    },
                ),
                Span::styled(status, Style::default().fg(self.theme.muted())),
            ]));
        }
        self.fill(frame, inner);
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
                if self.examining.is_some() {
                    " (checking again…)"
                } else {
                    ""
                }
            )
        };
        let list = List::new(self.measure(items))
            .block(self.card(&title))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused));
        bordered_list(frame, list, inner, &mut self.main_state, &self.hits);
    }
}
