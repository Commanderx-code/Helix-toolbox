//! The History screen.

use super::super::*;
use super::{Ask, Cx, ToolScreen, View};

/// Past runs, newest first, with their saved logs.
#[derive(Default)]
pub(in crate::tui::app) struct History {
    pub(in crate::tui::app) records: Vec<history::Record>,
}

impl History {
    fn reload(&mut self, cx: &mut Cx) {
        self.records = cx.host.history(200);
        cx.clamp();
    }

    fn selected(&self, at: Option<usize>) -> Option<&history::Record> {
        at.and_then(|index| self.records.get(index))
    }

    fn show_log(&self, cx: &mut Cx) {
        let Some(record) = self.selected(cx.at()) else {
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
        *cx.popup = Some(Popup::Text(TextPopup::new(title, text, false)));
    }
}

impl ToolScreen for History {
    fn open(&mut self, cx: &mut Cx) {
        self.reload(cx);
    }

    fn len(&self, _view: &View) -> usize {
        self.records.len()
    }

    fn key(&mut self, cx: &mut Cx, code: KeyCode) {
        match code {
            KeyCode::Enter => self.show_log(cx),
            KeyCode::Char('w') => {
                if let Some(record) = self.selected(cx.at()).cloned() {
                    cx.ask(Ask::ReportWorked(record, None));
                }
            }
            KeyCode::Char('r') => self.reload(cx),
            KeyCode::Char('h') | KeyCode::Left => cx.ask(Ask::Sidebar),
            _ => {}
        }
    }

    fn hints(&self, _view: &View, at: Option<usize>, list: &mut Vec<Hint>) {
        list.push(Hint::new(&["Enter"], "Show log"));
        if self.selected(at).is_some_and(|record| record.success) {
            list.push(Hint::new(&["w"], "Report it worked"));
        }
        list.push(Hint::new(&["r"], "Reload"));
    }

    fn after_run(&mut self, cx: &mut Cx, reading: Reading) {
        if reading == Reading::History {
            self.reload(cx);
        }
    }

    fn draw(&self, view: &View, frame: &mut Frame, area: Rect, state: &mut ListState) {
        let inner = view.page(frame, area, "History", "Past runs, newest first.");
        if self.records.is_empty() {
            frame.render_widget(
                Paragraph::new("No runs recorded yet.")
                    .style(Style::default().fg(view.theme.muted())),
                inner,
            );
            return;
        }
        let focused = view.focused;
        let items: Vec<Line> = self
            .records
            .iter()
            .map(|record| {
                let (status, color) = if record.success {
                    ("ok", view.theme.success())
                } else {
                    ("failed", view.theme.failure())
                };
                Line::from(vec![
                    Span::styled("● ", Style::default().fg(color)),
                    Span::styled(format!("{status:<8}"), Style::default().fg(color).bold()),
                    Span::styled(
                        format!("{:<22}", timestamp(record.unix_timestamp)),
                        Style::default().fg(view.theme.muted()),
                    ),
                    Span::styled(
                        record.names.join(", "),
                        Style::default().fg(view.theme.entry()),
                    ),
                    Span::styled(
                        format!("   {}", record.result),
                        Style::default().fg(view.theme.muted()),
                    ),
                    Span::styled(
                        if record.log.is_some() { "   log" } else { "" },
                        Style::default().fg(view.theme.focused()),
                    ),
                ])
            })
            .collect();
        view.fill(frame, inner);
        let list = List::new(view.measure(items))
            .block(view.card("Runs"))
            .highlight_style(view.highlight(focused))
            .highlight_symbol(view.marker(focused));
        bordered_list(frame, list, inner, state, view.hits);
    }
}
