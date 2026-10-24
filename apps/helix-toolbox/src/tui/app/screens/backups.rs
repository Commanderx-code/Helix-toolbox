//! The Backups screen.

use super::super::*;
use super::{Ask, Cx, ToolScreen, View};

/// The backups Helix Toolbox's entries left, newest first, and the
/// snapshots taken before system changes.
#[derive(Default)]
pub(in crate::tui::app) struct Backups {
    pub(in crate::tui::app) list: Vec<backups::Backup>,
}

impl Backups {
    fn reload(&mut self, cx: &mut Cx) {
        self.list = cx.host.backups();
        cx.clamp();
    }

    fn selected(&self, at: Option<usize>) -> Option<&backups::Backup> {
        at.and_then(|index| self.list.get(index))
    }

    fn describe(&self, cx: &mut Cx) {
        let Some(backup) = self.selected(cx.at()) else {
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
        *cx.popup = Some(Popup::Text(TextPopup::new("Backup", text, true)));
    }
}

impl ToolScreen for Backups {
    fn open(&mut self, cx: &mut Cx) {
        self.reload(cx);
    }

    fn len(&self, _view: &View) -> usize {
        self.list.len()
    }

    fn key(&mut self, cx: &mut Cx, code: KeyCode) {
        match code {
            KeyCode::Enter => {
                if let Some(backup) = self.selected(cx.at()).cloned() {
                    cx.confirm(backups::plan_restore(&backup));
                }
            }
            KeyCode::Char('d') => self.describe(cx),
            KeyCode::Char('e') => {
                if let Some(backup) = self.selected(cx.at()).cloned() {
                    cx.ask(Ask::Effects(backups::plan_restore(&backup)));
                }
            }
            KeyCode::Char('r') => self.reload(cx),
            KeyCode::Char('h') | KeyCode::Left => cx.ask(Ask::Sidebar),
            _ => {}
        }
    }

    fn hints(&self, _view: &View, _at: Option<usize>, list: &mut Vec<Hint>) {
        list.push(Hint::new(&["↑↓"], "Navigate"));
        if !self.list.is_empty() {
            list.push(Hint::new(&["Enter"], "Restore"));
            list.push(Hint::new(&["e"], "Side by side"));
            list.push(Hint::new(&["d"], "Details"));
        }
        list.push(Hint::new(&["r"], "Reload"));
    }

    fn draw(&self, view: &View, frame: &mut Frame, area: Rect, state: &mut ListState) {
        let inner = view.page(
            frame,
            area,
            "Backups",
            "Configuration backups Helix Toolbox's entries left, newest first.",
        );
        let snapshots = match (view.snapshot_tool, view.snapshots) {
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
            Style::default().fg(view.theme.muted()),
        ));
        let parts = Layout::vertical([
            Constraint::Length(rows as u16 + 2),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);
        view.fill(frame, parts[0]);
        frame.render_widget(
            Paragraph::new(Text::from(lines))
                .block(view.card("Snapshots"))
                .wrap(Wrap { trim: true }),
            parts[0],
        );
        view.fill(frame, parts[2]);
        if self.list.is_empty() {
            frame.render_widget(
                Paragraph::new(
                    "No backups yet. Entries that change your configuration or system files keep the old copy here.",
                )
                .block(view.card("Backups"))
                .style(Style::default().fg(view.theme.muted()))
                .wrap(Wrap { trim: true }),
                parts[2],
            );
            return;
        }
        let home = view.host.home().to_string_lossy().into_owned();
        let shown = |path: &std::path::Path| {
            let text = helix_toolbox_core::printable(&path.display().to_string());
            match text.strip_prefix(&home) {
                Some(rest) if !home.is_empty() => format!("~{rest}"),
                _ => text,
            }
        };
        let focused = view.focused;
        let items: Vec<Line> = self
            .list
            .iter()
            .map(|backup| {
                let (scope, color) = if backup.system {
                    ("system", view.theme.warning())
                } else {
                    ("user  ", view.theme.success())
                };
                Line::from(vec![
                    Span::styled(
                        format!("{:<21}", backup.taken.as_deref().unwrap_or("unknown time")),
                        Style::default().fg(view.theme.muted()),
                    ),
                    Span::styled(format!("{scope}  "), Style::default().fg(color)),
                    Span::styled(
                        shown(&backup.original),
                        Style::default().fg(view.theme.entry()),
                    ),
                    Span::styled(
                        if backup.directory { "/" } else { "" },
                        Style::default().fg(view.theme.entry()),
                    ),
                ])
            })
            .collect();
        let list = List::new(view.measure(items))
            .block(view.card("Backups"))
            .highlight_style(view.highlight(focused))
            .highlight_symbol(view.marker(focused));
        bordered_list(frame, list, parts[2], state, view.hits);
    }
}
