//! The Reliability screen.

use super::super::*;
use super::{Ask, Cx, ToolScreen, View};

/// How far each entry has been tested.
#[derive(Default)]
pub(in crate::tui::app) struct Reliability {
    /// Its rows: entry ids, names and standing.
    pub(in crate::tui::app) rows: Vec<(String, String, reliability::Assessment)>,
    pub(in crate::tui::app) coverage: Coverage,
}

/// Which entries the Reliability screen lists.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::tui::app) enum Coverage {
    #[default]
    All,
    /// Boot, login-screen, driver and whole-system entries.
    Critical,
    /// Not yet validated, or tested with stand-ins only.
    Untested,
}

impl Coverage {
    fn next(self) -> Self {
        match self {
            Coverage::All => Coverage::Critical,
            Coverage::Critical => Coverage::Untested,
            Coverage::Untested => Coverage::All,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Coverage::All => "all",
            Coverage::Critical => "critical",
            Coverage::Untested => "untested",
        }
    }

    fn allows(self, assessment: &reliability::Assessment) -> bool {
        match self {
            Coverage::All => true,
            Coverage::Critical => assessment.critical,
            Coverage::Untested => assessment.level >= reliability::Level::StandInTested,
        }
    }
}

impl Reliability {
    fn reload(&mut self, cx: &mut Cx) {
        let evidence = cx.evidence();
        let records = cx.host.history(1000);
        self.rows = crate::inspect::rows(cx.catalog, &evidence, &records)
            .into_iter()
            .filter(|(_, assessment)| self.coverage.allows(assessment))
            .map(|(entry, assessment)| (entry.id.clone(), entry.name.clone(), assessment))
            .collect();
        cx.clamp();
    }

    fn selected(&self, at: Option<usize>) -> Option<&(String, String, reliability::Assessment)> {
        at.and_then(|index| self.rows.get(index))
    }

    fn describe(&self, cx: &mut Cx) {
        let Some((id, name, assessment)) = self.selected(cx.at()) else {
            return;
        };
        let mut text = format!(
            "{}{}\n\n{}\n",
            assessment.level.label(),
            assessment
                .verified
                .as_deref()
                .map(|date| format!(", last verified {date}"))
                .unwrap_or_default(),
            assessment.level.explain()
        );
        if assessment.critical {
            text.push_str("\nA critical entry: if it fails, the machine may not boot, show its login screen or display, or it changes disks or the whole system. Keep a snapshot or a live USB at hand.\n");
        }
        if !assessment.evidence.is_empty() {
            text.push_str("\nWhat shows it:\n");
            for line in &assessment.evidence {
                text.push_str(&format!("  {line}\n"));
            }
        }
        if assessment.here.is_some() && assessment.level != reliability::Level::HardwareTested {
            text.push_str(
                "\nIt ran here: w reports that it worked, so that it can become Hardware Tested.\n",
            );
        }
        text.push_str(&format!(
            "\nv shows where {id} comes from and what checks it."
        ));
        *cx.popup = Some(Popup::Text(TextPopup::new(name.clone(), text, true)));
    }
}

impl ToolScreen for Reliability {
    fn open(&mut self, cx: &mut Cx) {
        self.reload(cx);
    }

    fn len(&self, _view: &View) -> usize {
        self.rows.len()
    }

    fn key(&mut self, cx: &mut Cx, code: KeyCode) {
        match code {
            KeyCode::Enter | KeyCode::Char('d') => self.describe(cx),
            KeyCode::Char('v') => {
                if let Some((id, _, _)) = self.selected(cx.at()) {
                    cx.ask(Ask::Inspect(id.clone()));
                }
            }
            KeyCode::Char('i') => {
                self.coverage = self.coverage.next();
                cx.selected.select(Some(0));
                self.reload(cx);
            }
            KeyCode::Char('w') => {
                if let Some((id, name, _)) = self.selected(cx.at()).cloned() {
                    match hardware_report::last_success(&cx.host.history(1000), &id).cloned() {
                        Some(record) => cx.ask(Ask::ReportWorked(record, Some(id))),
                        None => {
                            *cx.popup = Some(Popup::Text(TextPopup::new(
                                "Not run here",
                                format!(
                                    "A hardware report comes from a run that worked on this machine, and {name} has no successful run in this machine's history."
                                ),
                                true,
                            )))
                        }
                    }
                }
            }
            KeyCode::Char('h') | KeyCode::Left => cx.ask(Ask::Sidebar),
            _ => {}
        }
    }

    fn hints(&self, _view: &View, at: Option<usize>, list: &mut Vec<Hint>) {
        list.push(Hint::new(&["↑↓"], "Navigate"));
        if let Some((_, _, assessment)) = self.selected(at) {
            list.push(Hint::new(&["Enter"], "Details"));
            list.push(Hint::new(&["v"], "Trust"));
            if assessment.here.is_some() {
                list.push(Hint::new(&["w"], "Report it worked"));
            }
        }
        list.push(Hint::new(
            &["i"],
            match self.coverage {
                Coverage::All => "Show: all",
                Coverage::Critical => "Show: critical",
                Coverage::Untested => "Show: untested",
            },
        ));
    }

    fn draw(&self, view: &View, frame: &mut Frame, area: Rect, state: &mut ListState) {
        let inner = view.page(
            frame,
            area,
            "Reliability",
            "How far each entry has been tested; critical entries first. v shows where one comes from.",
        );
        let evidence = view.evidence();
        let counts: Vec<String> = {
            let records = view.host.history(1000);
            let all = crate::inspect::rows(view.catalog, &evidence, &records);
            crate::inspect::counts(&all)
                .into_iter()
                .map(|(level, count)| format!("{count} {}", level.label()))
                .collect()
        };
        let summary = evidence.summary();
        let counts = counts.join(" · ");
        // As tall as its text once wrapped to the card's width.
        let width = (inner.width.saturating_sub(4) as usize).max(1);
        // What could not be read of the record, as the window's warning card.
        let problems: Vec<String> = evidence
            .problems
            .iter()
            .map(|problem| format!("⚠ {problem}"))
            .collect();
        let rows: usize = [summary.as_str(), counts.as_str()]
            .into_iter()
            .chain(problems.iter().map(String::as_str))
            .map(|text| text.chars().count().div_ceil(width).max(1))
            .sum();
        let mut lines = vec![
            Line::from(summary),
            Line::styled(counts, Style::default().fg(view.theme.muted())),
        ];
        lines.extend(
            problems
                .into_iter()
                .map(|problem| Line::styled(problem, Style::default().fg(view.theme.warning()))),
        );
        let parts = Layout::vertical([
            Constraint::Length(rows as u16 + 2),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);
        view.fill(frame, parts[0]);
        frame.render_widget(
            Paragraph::new(Text::from(lines))
                .block(view.card("Record"))
                .wrap(Wrap { trim: true }),
            parts[0],
        );
        let focused = view.focused;
        let name_width = 34;
        let items: Vec<Line> = self
            .rows
            .iter()
            .map(|(_, name, assessment)| {
                let color = match assessment.level {
                    reliability::Level::NotYetValidated if assessment.critical => {
                        view.theme.failure()
                    }
                    reliability::Level::NotYetValidated => view.theme.muted(),
                    reliability::Level::StandInTested => view.theme.warning(),
                    _ => view.theme.success(),
                };
                let name: String = name.chars().take(name_width - 2).collect();
                let mut detail = assessment.systems.join(", ");
                if let Some(when) = assessment.here {
                    if !detail.is_empty() {
                        detail.push_str(" · ");
                    }
                    detail.push_str(&format!("ran here {}", helix_toolbox_engine::date(when)));
                }
                Line::from(vec![
                    Span::styled(
                        format!("{:<19}", assessment.level.label()),
                        Style::default().fg(color).bold(),
                    ),
                    Span::styled(
                        if assessment.critical { "! " } else { "  " },
                        Style::default().fg(view.theme.failure()).bold(),
                    ),
                    Span::styled(
                        format!("{name:<name_width$}"),
                        Style::default().fg(view.theme.entry()),
                    ),
                    Span::styled(
                        format!("{:<12}", assessment.verified.as_deref().unwrap_or("")),
                        Style::default().fg(view.theme.muted()),
                    ),
                    Span::styled(detail, Style::default().fg(view.theme.faint())),
                ])
            })
            .collect();
        let title = match self.coverage {
            Coverage::All => "Entries".to_string(),
            coverage => format!("Entries · {}", coverage.label()),
        };
        view.fill(frame, parts[2]);
        let list = List::new(view.measure(items))
            .block(
                view.card(&title).title(
                    Line::styled(
                        " ! CRITICAL · LAST VERIFIED ",
                        Style::default().fg(view.theme.muted()),
                    )
                    .right_aligned(),
                ),
            )
            .highlight_style(view.highlight(focused))
            .highlight_symbol(view.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, parts[2], state, view.hits);
    }
}
