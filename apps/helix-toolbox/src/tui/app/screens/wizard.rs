//! The setup wizard.

use super::super::*;

impl App {
    /// Open the wizard; on the first run it says welcome.
    pub fn open_wizard(&mut self, first_run: bool) {
        let setup = match helix_toolbox_engine::setup::load(self.catalog.root()) {
            Ok(setup) => setup,
            // A catalog without workstations (a test's) has nothing to offer at the start.
            Err(_) if first_run => return,
            Err(error) => {
                self.popup = Some(Popup::Text(TextPopup::new("No setup wizard", error, true)));
                return;
            }
        };
        let desktop = self.desktop_here();
        let survey = if self.examine_here {
            crate::setup::survey_here(&self.platform, &desktop)
        } else {
            helix_toolbox_engine::setup::survey(
                &self.platform,
                &desktop,
                Vec::new(),
                false,
                &|_| false,
            )
        };
        self.wizard_ids = setup
            .workstations
            .iter()
            .map(|known| known.id.clone())
            .collect();
        let workstations = setup
            .workstations
            .iter()
            .map(|known| (known.name.clone(), known.description.clone()))
            .collect();
        self.popup = Some(Popup::Wizard(Box::new(Wizard::new(
            crate::setup::survey_lines(&survey),
            workstations,
            first_run,
        ))));
    }

    /// Whether this is the first run, when the wizard opens by itself.
    pub fn first_run(&self) -> bool {
        !self.set_up
    }

    pub(in crate::tui::app) fn close_wizard(&mut self) {
        self.popup = None;
        if !self.set_up {
            self.set_up = true;
            self.save_look();
        }
    }

    /// The chosen workstation's entries for this machine, ticked where they
    /// can run and are not installed.
    pub(in crate::tui::app) fn wizard_picks(&mut self, index: usize) -> Vec<Pick> {
        let Ok(setup) = helix_toolbox_engine::setup::load(self.catalog.root()) else {
            return Vec::new();
        };
        let desktop = self.desktop_here();
        let survey = if self.examine_here {
            crate::setup::survey_here(&self.platform, &desktop)
        } else {
            helix_toolbox_engine::setup::survey(
                &self.platform,
                &desktop,
                Vec::new(),
                false,
                &|_| false,
            )
        };
        let Some(chosen) = self
            .wizard_ids
            .get(index)
            .and_then(|id| setup.workstations.iter().find(|known| known.id == *id))
        else {
            return Vec::new();
        };
        let custom = chosen.entries.is_empty() && chosen.when.is_empty();
        let ids = if custom {
            setup.everything(&survey)
        } else {
            chosen.entries_for(&survey)
        };
        helix_toolbox_engine::setup::suggestions(&ids, &self.catalog, &self.platform)
            .into_iter()
            .map(|suggestion| {
                let status = self.statuses.get(&suggestion.id);
                let installed = matches!(
                    status,
                    Some(
                        Status::Installed { .. }
                            | Status::Update { .. }
                            | Status::Managed { .. }
                            | Status::Elsewhere { .. }
                    )
                );
                let note = if installed {
                    Some(format!(
                        "installed ({})",
                        status.map(Status::detail).unwrap_or_default()
                    ))
                } else {
                    suggestion.note.clone()
                };
                Pick {
                    ticked: suggestion.available && !installed && !custom,
                    id: suggestion.id,
                    name: suggestion.name,
                    available: suggestion.available,
                    note,
                }
            })
            .collect()
    }

    /// Put the wizard's choices in the Install Plan and show it.
    pub(in crate::tui::app) fn queue_from_wizard(&mut self, ids: &[String]) {
        let mut added = Vec::new();
        let mut alone = Vec::new();
        for id in ids {
            let Some(entry) = self.catalog.find(id).cloned() else {
                continue;
            };
            if !entry.multi_select {
                alone.push(entry.name.clone());
            } else if !self.is_queued(&entry) {
                added.push(entry.name.clone());
                self.queue.push(entry);
            }
        }
        let mut text = if added.is_empty() {
            "Nothing was added to the Install Plan.".to_string()
        } else {
            format!(
                "Added to the Install Plan: {}.\n\nNothing has run. Review the plan, e shows what it would change, and Enter runs it.",
                added.join(", ")
            )
        };
        if !alone.is_empty() {
            text.push_str(&format!(
                "\n\nThese run on their own; run each from its area: {}.",
                alone.join(", ")
            ));
        }
        self.open_screen(Screen::Plan);
        self.focus = Focus::Main;
        self.popup = Some(Popup::Text(TextPopup::new("Install Plan", text, true)));
    }
}
