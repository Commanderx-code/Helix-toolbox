//! The History screen.

use super::super::*;

impl App {
    pub(in crate::tui::app) fn reload_history(&mut self) {
        self.records = history::read(200).unwrap_or_default();
        self.clamp_main();
    }

    pub(in crate::tui::app) fn draw_history(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(frame, area, "History", "Past runs, newest first.");
        if self.records.is_empty() {
            frame.render_widget(
                Paragraph::new("No runs recorded yet.")
                    .style(Style::default().fg(self.theme.muted())),
                inner,
            );
            return;
        }
        let focused = self.focus == Focus::Main;
        let items: Vec<Line> = self
            .records
            .iter()
            .map(|record| {
                let (status, color) = if record.success {
                    ("ok", self.theme.success())
                } else {
                    ("failed", self.theme.failure())
                };
                Line::from(vec![
                    Span::styled("● ", Style::default().fg(color)),
                    Span::styled(format!("{status:<8}"), Style::default().fg(color).bold()),
                    Span::styled(
                        format!("{:<22}", timestamp(record.unix_timestamp)),
                        Style::default().fg(self.theme.muted()),
                    ),
                    Span::styled(
                        record.names.join(", "),
                        Style::default().fg(self.theme.entry()),
                    ),
                    Span::styled(
                        format!("   {}", record.result),
                        Style::default().fg(self.theme.muted()),
                    ),
                    Span::styled(
                        if record.log.is_some() { "   log" } else { "" },
                        Style::default().fg(self.theme.focused()),
                    ),
                ])
            })
            .collect();
        self.fill(frame, inner);
        let list = List::new(self.measure(items))
            .block(self.card("Runs"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused));
        bordered_list(frame, list, inner, &mut self.main_state, &self.hits);
    }
}
