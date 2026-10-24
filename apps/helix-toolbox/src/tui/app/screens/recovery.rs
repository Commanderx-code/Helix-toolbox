//! The Recovery screen.

use super::super::*;

impl App {
    pub(in crate::tui::app) fn snapshots_here(&self) -> Option<backups::Snapshots> {
        if self.examine_here {
            backups::Snapshots::here()
        } else {
            self.snapshot_tool
        }
    }

    /// The restic repository for the recovery checks, with its newest
    /// snapshot when the Restic screen read them already.
    pub(in crate::tui::app) fn restic_repository(&self) -> Option<recovery::Repository> {
        let reading = self.restic_reading.as_ref();
        match reading.map(|reading| &reading.found) {
            Some(restic::Found::Ready(config)) => Some(recovery::Repository {
                name: config.repository.clone(),
                newest: reading
                    .and_then(|reading| reading.snapshots.as_ref().ok()?.first())
                    .map(restic::Snapshot::when),
            }),
            Some(_) => None,
            None => restic::repository_here(),
        }
    }

    pub(in crate::tui::app) fn reload_recovery(&mut self) {
        if self.examine_here {
            let found_backups = backups::discover(&backups::Roots::here());
            let baselines = drift::dir()
                .map(|directory| drift::load_all(&directory).0)
                .unwrap_or_default();
            self.recovery_found = recovery::examine(
                &doctor::Here,
                &recovery::Known {
                    manager: self.platform.manager,
                    backups: &found_backups,
                    baselines: &baselines,
                    now: history::now(),
                    restic: self.restic_repository(),
                },
            );
        }
        self.clamp_main();
    }

    pub(in crate::tui::app) fn selected_readiness(&self) -> Option<&doctor::Diagnosis> {
        self.recovery_found.get(self.main_state.selected()?)
    }

    pub(in crate::tui::app) fn draw_recovery(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Recovery",
            "Whether this machine could be brought back after a bad update or a lost disk. It only reads.",
        );
        let focused = self.focus == Focus::Main;
        let title_width = self
            .recovery_found
            .iter()
            .map(|check| check.title.chars().count())
            .max()
            .unwrap_or(0)
            + 2;
        let can_check = self.snapshots_here().is_some();
        let mut items: Vec<Line> = Vec::new();
        if self.recovery_found.is_empty() {
            items.push(Line::styled(
                "Press r to check this machine.",
                Style::default().fg(self.theme.muted()),
            ));
        }
        for check in &self.recovery_found {
            let color = match check.health {
                doctor::Health::Good => self.theme.success(),
                doctor::Health::Notice => self.theme.warning(),
                doctor::Health::Problem => self.theme.failure(),
            };
            let offer = if check.id == "snapshots" && can_check {
                "  · c checks them"
            } else if check
                .entry
                .as_deref()
                .and_then(|id| self.catalog.find(id))
                .is_some()
            {
                "  · f sets it up"
            } else {
                ""
            };
            items.push(Line::from(vec![
                Span::styled(
                    format!("{:<8}", check.health.label()),
                    Style::default().fg(color).bold(),
                ),
                Span::styled(
                    format!("{:<title_width$}", check.title),
                    Style::default().fg(self.theme.entry()),
                ),
                Span::styled(
                    check.summary.clone(),
                    Style::default().fg(self.theme.muted()),
                ),
                Span::styled(offer, Style::default().fg(self.theme.focused())),
            ]));
        }
        self.fill(frame, inner);
        let list = List::new(self.measure(items))
            .block(self.card("Readiness"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused));
        bordered_list(frame, list, inner, &mut self.main_state, &self.hits);
    }

    /// What planning asks of the machine in tests: nothing is installed
    /// but Flatpak, so what a test sees never depends on this machine.
    pub(in crate::tui::app) fn test_machine() -> Machine<'static> {
        Machine {
            holds: &|_: &Probe| false,
            on_path: &|tool: &str| tool == "flatpak",
            ..Machine::blind()
        }
    }
}
