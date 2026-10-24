//! The Backups screen.

use super::super::*;

impl App {
    pub(in crate::tui::app) fn draw_backups(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Backups",
            "Configuration backups Helix Toolbox's entries left, newest first.",
        );
        let snapshots = match (self.snapshot_tool, self.snapshots) {
            (Some(tool), true) => vec![
                format!(
                    "{} is set up: a snapshot is taken before runs that change system files, the kernel or disks.",
                    tool.label()
                ),
                format!("Roll back with {}.", tool.how_to_restore()),
            ],
            (Some(tool), false) => vec![
                format!(
                    "{} is set up, but Settings has snapshots before system changes off.",
                    tool.label()
                ),
                format!("Roll back with {}.", tool.how_to_restore()),
            ],
            (None, _) => vec![
                "Neither Snapper (with a root configuration) nor Timeshift is set up, so no snapshots are taken.".to_string(),
            ],
        };
        let closing =
            "Helix Toolbox restores configuration backups; it does not roll packages back.";
        // The card is as tall as its text once wrapped to the card's width.
        let width = (inner.width.saturating_sub(4) as usize).max(1);
        let rows: usize = snapshots
            .iter()
            .map(String::as_str)
            .chain([closing])
            .map(|text| text.chars().count().div_ceil(width).max(1))
            .sum();
        let mut lines: Vec<Line> = snapshots.into_iter().map(Line::from).collect();
        lines.push(Line::styled(
            closing,
            Style::default().fg(self.theme.muted()),
        ));
        let parts = Layout::vertical([
            Constraint::Length(rows as u16 + 2),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);
        self.fill(frame, parts[0]);
        frame.render_widget(
            Paragraph::new(Text::from(lines))
                .block(self.card("Snapshots"))
                .wrap(Wrap { trim: true }),
            parts[0],
        );
        self.fill(frame, parts[2]);
        if self.backups.is_empty() {
            frame.render_widget(
                Paragraph::new(
                    "No backups yet. Entries that change your configuration or system files keep the old copy here.",
                )
                .block(self.card("Backups"))
                .style(Style::default().fg(self.theme.muted()))
                .wrap(Wrap { trim: true }),
                parts[2],
            );
            return;
        }
        let home = std::env::var("HOME").unwrap_or_default();
        let shown = |path: &std::path::Path| {
            let text = helix_toolbox_core::printable(&path.display().to_string());
            match text.strip_prefix(&home) {
                Some(rest) if !home.is_empty() => format!("~{rest}"),
                _ => text,
            }
        };
        let focused = self.focus == Focus::Main;
        let items: Vec<Line> = self
            .backups
            .iter()
            .map(|backup| {
                let (scope, color) = if backup.system {
                    ("system", self.theme.warning())
                } else {
                    ("user  ", self.theme.success())
                };
                Line::from(vec![
                    Span::styled(
                        format!("{:<21}", backup.taken.as_deref().unwrap_or("unknown time")),
                        Style::default().fg(self.theme.muted()),
                    ),
                    Span::styled(format!("{scope}  "), Style::default().fg(color)),
                    Span::styled(
                        shown(&backup.original),
                        Style::default().fg(self.theme.entry()),
                    ),
                    Span::styled(
                        if backup.directory { "/" } else { "" },
                        Style::default().fg(self.theme.entry()),
                    ),
                ])
            })
            .collect();
        let list = List::new(self.measure(items))
            .block(self.card("Backups"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused));
        bordered_list(frame, list, parts[2], &mut self.main_state, &self.hits);
    }

    pub(in crate::tui::app) fn reload_backups(&mut self) {
        self.backups = backups::discover(&backups::Roots::here());
        self.clamp_main();
    }

    pub(in crate::tui::app) fn selected_backup(&self) -> Option<&backups::Backup> {
        self.main_state
            .selected()
            .and_then(|index| self.backups.get(index))
    }

    pub(in crate::tui::app) fn describe_backup(&mut self) {
        let Some(backup) = self.selected_backup() else {
            return;
        };
        let text = format!(
            "Backup of: {}\nTaken: {}\nCopy: {}\nKind: {}, {}\n\nEnter restores it. The current {} is moved aside as its own backup first, so the restore can be undone the same way.",
            helix_toolbox_core::printable(&backup.original.display().to_string()),
            backup.taken.as_deref().unwrap_or("unknown"),
            helix_toolbox_core::printable(&backup.backup.display().to_string()),
            if backup.directory {
                "a folder"
            } else {
                "a file"
            },
            if backup.system {
                "a system file (restoring asks for your password)"
            } else {
                "your own file"
            },
            if backup.directory { "folder" } else { "file" },
        );
        self.popup = Some(Popup::Text(TextPopup::new("Backup", text, true)));
    }

    pub(in crate::tui::app) fn show_log(&mut self) {
        let Some(record) = self
            .main_state
            .selected()
            .and_then(|index| self.records.get(index))
        else {
            return;
        };
        let title = record.names.join(", ");
        let text = match &record.log {
            Some(path) => std::fs::read(path)
                .map(|bytes| strip_escapes(&String::from_utf8_lossy(&bytes)))
                .unwrap_or_else(|error| format!("The log cannot be read: {error}")),
            None => format!(
                "{}\n\nNo log was saved for this run. Press l in the running window after a run finishes to save one.",
                record.result
            ),
        };
        self.popup = Some(Popup::Text(TextPopup::new(title, text, false)));
    }
}
