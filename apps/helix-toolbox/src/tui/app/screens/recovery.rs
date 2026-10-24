//! The Recovery screen.

use super::super::*;

impl App {
    pub(in crate::tui::app) fn snapshots_here(&self) -> Option<backups::Snapshots> {
        self.host.snapshots()
    }

    pub(in crate::tui::app) fn reload_recovery(&mut self) {
        self.recovery_found = recovery::examine_on(&self.host, self.platform.manager);
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
}
