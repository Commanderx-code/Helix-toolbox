//! The Recovery screen.

use super::super::*;
use super::doctor::diagnosis_popup;
use super::{Ask, Cx, ToolScreen, View};

/// Whether the machine could be brought back after a bad update.
#[derive(Default)]
pub(in crate::tui::app) struct Recovery {
    /// Recovery readiness, as last checked.
    pub(in crate::tui::app) found: Vec<doctor::Diagnosis>,
}

impl Recovery {
    fn reload(&mut self, cx: &mut Cx) {
        self.found = recovery::examine_on(cx.host, cx.platform.manager);
        cx.clamp();
    }

    fn selected(&self, at: Option<usize>) -> Option<&doctor::Diagnosis> {
        self.found.get(at?)
    }
}

impl ToolScreen for Recovery {
    fn open(&mut self, cx: &mut Cx) {
        self.reload(cx);
    }

    fn len(&self, _view: &View) -> usize {
        self.found.len()
    }

    fn key(&mut self, cx: &mut Cx, code: KeyCode) {
        match code {
            KeyCode::Enter | KeyCode::Char('d') | KeyCode::Char('l') | KeyCode::Right => {
                if let Some(check) = self.selected(cx.at()) {
                    *cx.popup = Some(diagnosis_popup(
                        check,
                        cx.catalog,
                        cx.screen == Screen::Recovery,
                    ));
                }
            }
            KeyCode::Char('f') => {
                if let Some(entry) = self
                    .selected(cx.at())
                    .and_then(|check| check.entry.as_deref())
                    .and_then(|id| cx.catalog.find(id))
                    .cloned()
                {
                    cx.ask(Ask::Run(vec![entry]));
                }
            }
            KeyCode::Char('c') => match cx.host.snapshots() {
                Some(tool) => cx.confirm(recovery::plan_check(tool)),
                None => {
                    *cx.popup = Some(Popup::Text(TextPopup::new(
                        "No snapshots",
                        "Neither Snapper (with a root configuration) nor Timeshift is set up, so there are no snapshots to check.",
                        true,
                    )))
                }
            },
            KeyCode::Char('r') => self.reload(cx),
            KeyCode::Char('h') | KeyCode::Left => cx.ask(Ask::Sidebar),
            _ => {}
        }
    }

    fn hints(&self, view: &View, at: Option<usize>, list: &mut Vec<Hint>) {
        list.push(Hint::new(&["↑↓"], "Navigate"));
        list.push(Hint::new(&["Enter"], "Details"));
        if self
            .selected(at)
            .and_then(|check| check.entry.as_deref())
            .and_then(|id| view.catalog.find(id))
            .is_some()
        {
            list.push(Hint::new(&["f"], "Set it up"));
        }
        if view.host.snapshots().is_some() {
            list.push(Hint::new(&["c"], "Check snapshots"));
        }
        list.push(Hint::new(&["r"], "Check again"));
    }

    fn after_run(&mut self, cx: &mut Cx, reading: Reading) {
        if reading == Reading::Tools {
            self.reload(cx);
        }
    }

    fn draw(&self, view: &View, frame: &mut Frame, area: Rect, state: &mut ListState) {
        let inner = view.page(
            frame,
            area,
            "Recovery",
            "Whether this machine could be brought back after a bad update or a lost disk. It only reads.",
        );
        let focused = view.focused;
        let title_width = self
            .found
            .iter()
            .map(|check| check.title.chars().count())
            .max()
            .unwrap_or(0)
            + 2;
        let can_check = view.host.snapshots().is_some();
        let mut items: Vec<Line> = Vec::new();
        if self.found.is_empty() {
            items.push(Line::styled(
                "Press r to check this machine.",
                Style::default().fg(view.theme.muted()),
            ));
        }
        for check in &self.found {
            let color = match check.health {
                doctor::Health::Good => view.theme.success(),
                doctor::Health::Notice => view.theme.warning(),
                doctor::Health::Problem => view.theme.failure(),
            };
            let offer = if check.id == "snapshots" && can_check {
                "  · c checks them"
            } else if check
                .entry
                .as_deref()
                .and_then(|id| view.catalog.find(id))
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
                    Style::default().fg(view.theme.entry()),
                ),
                Span::styled(
                    check.summary.clone(),
                    Style::default().fg(view.theme.muted()),
                ),
                Span::styled(offer, Style::default().fg(view.theme.focused())),
            ]));
        }
        view.fill(frame, inner);
        let list = List::new(view.measure(items))
            .block(view.card("Readiness"))
            .highlight_style(view.highlight(focused))
            .highlight_symbol(view.marker(focused));
        bordered_list(frame, list, inner, state, view.hits);
    }
}
