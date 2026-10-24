//! The Reliability screen.

use super::super::*;

impl App {
    /// This build's testing record, read on first use.
    pub(in crate::tui::app) fn evidence(&mut self) -> reliability::Evidence {
        if self.evidence.is_none() {
            self.evidence = Some(reliability::Evidence::load(self.catalog.root()));
        }
        self.evidence.clone().unwrap_or_default()
    }

    pub(in crate::tui::app) fn run_history(&self) -> Vec<history::Record> {
        if self.examine_here {
            history::read(1000).unwrap_or_default()
        } else {
            self.records.clone()
        }
    }

    pub(in crate::tui::app) fn reload_reliability(&mut self) {
        let evidence = self.evidence();
        let records = self.run_history();
        self.reliability_rows = crate::inspect::rows(&self.catalog, &evidence, &records)
            .into_iter()
            .filter(|(_, assessment)| self.coverage.allows(assessment))
            .map(|(entry, assessment)| (entry.id.clone(), entry.name.clone(), assessment))
            .collect();
        self.clamp_main();
    }

    pub(in crate::tui::app) fn describe_reliability(&mut self) {
        let Some((id, name, assessment)) = self
            .main_state
            .selected()
            .and_then(|index| self.reliability_rows.get(index))
        else {
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
        self.popup = Some(Popup::Text(TextPopup::new(name.clone(), text, true)));
    }

    /// The trust inspector for an entry.
    pub(in crate::tui::app) fn inspect(&mut self, id: &str) {
        let Some(entry) = self.catalog.find(id).cloned() else {
            return;
        };
        let evidence = self.evidence();
        let records = self.run_history();
        let build = if self.examine_here {
            crate::inspect::build()
        } else {
            helix_toolbox_engine::trust::Build {
                version: crate::VERSION.to_string(),
                release: false,
                commit: None,
                install: helix_toolbox_engine::updates::Install::Other,
            }
        };
        let found = helix_toolbox_engine::trust::inspect(
            &entry,
            self.catalog.root(),
            &self.platform,
            &build,
            &doctor::Here,
            &evidence,
            &records,
        );
        self.popup = Some(Popup::Text(TextPopup::new(
            format!("Trust: {}", entry.name),
            crate::inspect::text(&found),
            true,
        )));
    }

    pub(in crate::tui::app) fn draw_reliability(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Reliability",
            "How far each entry has been tested; critical entries first. v shows where one comes from.",
        );
        let evidence = self.evidence();
        let counts: Vec<String> = {
            let records = self.run_history();
            let all = crate::inspect::rows(&self.catalog, &evidence, &records);
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
            Line::styled(counts, Style::default().fg(self.theme.muted())),
        ];
        lines.extend(
            problems
                .into_iter()
                .map(|problem| Line::styled(problem, Style::default().fg(self.theme.warning()))),
        );
        let parts = Layout::vertical([
            Constraint::Length(rows as u16 + 2),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);
        self.fill(frame, parts[0]);
        frame.render_widget(
            Paragraph::new(Text::from(lines))
                .block(self.card("Record"))
                .wrap(Wrap { trim: true }),
            parts[0],
        );
        let focused = self.focus == Focus::Main;
        let name_width = 34;
        let items: Vec<Line> = self
            .reliability_rows
            .iter()
            .map(|(_, name, assessment)| {
                let color = match assessment.level {
                    reliability::Level::NotYetValidated if assessment.critical => {
                        self.theme.failure()
                    }
                    reliability::Level::NotYetValidated => self.theme.muted(),
                    reliability::Level::StandInTested => self.theme.warning(),
                    _ => self.theme.success(),
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
                        Style::default().fg(self.theme.failure()).bold(),
                    ),
                    Span::styled(
                        format!("{name:<name_width$}"),
                        Style::default().fg(self.theme.entry()),
                    ),
                    Span::styled(
                        format!("{:<12}", assessment.verified.as_deref().unwrap_or("")),
                        Style::default().fg(self.theme.muted()),
                    ),
                    Span::styled(detail, Style::default().fg(self.theme.faint())),
                ])
            })
            .collect();
        let title = match self.coverage {
            Coverage::All => "Entries".to_string(),
            coverage => format!("Entries · {}", coverage.label()),
        };
        self.fill(frame, parts[2]);
        let list = List::new(self.measure(items))
            .block(
                self.card(&title).title(
                    Line::styled(
                        " ! CRITICAL · LAST VERIFIED ",
                        Style::default().fg(self.theme.muted()),
                    )
                    .right_aligned(),
                ),
            )
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, parts[2], &mut self.main_state, &self.hits);
    }
}
