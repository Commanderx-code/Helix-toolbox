//! The window that shows entries running in their pseudo-terminal, drawn
//! from the engine's execution events.

use super::{hints::Hint, theme::Theme};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use helix_toolbox_engine::{
    Check, Finished, Outcome, Plan, PtyRunner, RunSession, StepState,
    execution::{ExecutionEvent, Options},
    history,
    session::Reading,
};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Clear, Padding, Paragraph},
};
use std::{
    path::{Path, PathBuf},
    sync::mpsc::Receiver,
};
use tui_term::widget::PseudoTerminal;

/// How a step of the run stands, as its events said.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Standing {
    Waiting,
    Running,
    Done,
    Failed(u32),
    Interrupted,
    Skipped,
    NotRun,
}

impl Standing {
    fn mark(self) -> &'static str {
        match self {
            Standing::Waiting => "○",
            Standing::Running => "◐",
            Standing::Done => "✓",
            Standing::Failed(_) => "✗",
            Standing::Interrupted => "■",
            Standing::Skipped | Standing::NotRun => "–",
        }
    }

    fn color(self, theme: Theme) -> Color {
        match self {
            Standing::Waiting | Standing::Skipped | Standing::NotRun => theme.faint(),
            Standing::Running => theme.focused(),
            Standing::Done => theme.success(),
            Standing::Failed(_) | Standing::Interrupted => theme.failure(),
        }
    }
}

impl From<&StepState> for Standing {
    fn from(state: &StepState) -> Self {
        match state {
            StepState::Done => Standing::Done,
            StepState::Failed(code) => Standing::Failed(*code),
            StepState::Interrupted => Standing::Interrupted,
            StepState::Skipped => Standing::Skipped,
            StepState::NotRun => Standing::NotRun,
        }
    }
}

/// The steps are drawn beside the terminal from this width on, and the
/// messages under it from this height on.
const STEPS_FROM_WIDTH: u16 = 70;
const MESSAGES_FROM_HEIGHT: u16 = 12;

/// What the run's events said: each step's standing, how many have ended,
/// the warnings and, once it ends, the outcome with its checks.
#[derive(Default)]
struct Shown {
    /// Each step's name and how it stands, in the plan's order.
    steps: Vec<(String, Standing)>,
    /// How many steps have ended, of all.
    ended: usize,
    /// What the run warned about without stopping.
    warnings: Vec<String>,
    /// The outcome with the checks of its package steps, once it ends.
    finished: Option<Finished>,
}

impl Shown {
    fn new(plan: &Plan) -> Self {
        Self {
            steps: plan
                .steps
                .iter()
                .map(|step| (step.name().to_string(), Standing::Waiting))
                .collect(),
            ..Self::default()
        }
    }

    /// Take an event other than output; true when it changes the window.
    fn take(&mut self, event: ExecutionEvent) -> bool {
        let mut mark = |index: usize, standing: Standing| {
            if let Some(step) = self.steps.get_mut(index) {
                step.1 = standing;
            }
        };
        match event {
            ExecutionEvent::RunStarted { steps, .. } => {
                self.steps = steps
                    .into_iter()
                    .map(|step| (step.name, Standing::Waiting))
                    .collect();
            }
            ExecutionEvent::StepStarted(step) => mark(step.index, Standing::Running),
            ExecutionEvent::StepCompleted(step) => mark(step.index, Standing::Done),
            ExecutionEvent::StepFailed {
                step,
                exit_code,
                interrupted,
            } => mark(
                step.index,
                if interrupted {
                    Standing::Interrupted
                } else {
                    Standing::Failed(exit_code)
                },
            ),
            ExecutionEvent::StepSkipped(step) => mark(step.index, Standing::Skipped),
            ExecutionEvent::Progress { ended, .. } => self.ended = ended,
            // The note along the bottom counts what is still running as it
            // changes, so this one is not kept.
            ExecutionEvent::Warning(message)
                if message.ends_with("of this run are still running") => {}
            ExecutionEvent::Warning(message) => self.warnings.push(message),
            ExecutionEvent::Completed(finished) | ExecutionEvent::Interrupted(finished) => {
                // The end says how every step stands, those not run included.
                if finished.steps.len() == self.steps.len() {
                    for (shown, step) in self.steps.iter_mut().zip(&finished.steps) {
                        shown.1 = Standing::from(&step.state);
                    }
                }
                self.finished = Some(finished);
            }
            ExecutionEvent::Output { .. } => {}
        }
        true
    }

    /// A bar of the steps that have ended, and how many of all.
    fn progress_line(&self, width: u16, theme: Theme) -> Line<'static> {
        let total = self.steps.len();
        let ended = self.ended.min(total);
        let count = format!(" {ended} of {total} steps ended");
        let bar = usize::from(width).saturating_sub(count.chars().count());
        let filled = (bar * ended).checked_div(total).unwrap_or(0);
        let failed = self
            .steps
            .iter()
            .any(|(_, standing)| matches!(standing, Standing::Failed(_) | Standing::Interrupted));
        Line::from(vec![
            Span::styled(
                "━".repeat(filled),
                Style::default().fg(if failed {
                    theme.failure()
                } else {
                    theme.success()
                }),
            ),
            Span::styled("─".repeat(bar - filled), Style::default().fg(theme.line())),
            Span::styled(count, Style::default().fg(theme.muted())),
        ])
    }

    /// Each step with its mark and, when it failed, its exit status; the
    /// list follows the running step when it is longer than the room.
    fn step_list(&self, area: Rect, theme: Theme) -> Paragraph<'static> {
        let width = usize::from(area.width);
        let current = self
            .steps
            .iter()
            .position(|(_, standing)| *standing == Standing::Running)
            .or_else(|| {
                self.steps
                    .iter()
                    .rposition(|(_, standing)| *standing != Standing::Waiting)
            })
            .unwrap_or(0);
        let height = usize::from(area.height).max(1);
        let first = current
            .saturating_sub(height.saturating_sub(1) / 2)
            .min(self.steps.len().saturating_sub(height));
        let lines: Vec<Line> = self
            .steps
            .iter()
            .skip(first)
            .take(height)
            .map(|(name, standing)| {
                let exit = match standing {
                    Standing::Failed(code) => format!(" exit {code}"),
                    _ => String::new(),
                };
                let room = width.saturating_sub(2 + exit.chars().count());
                let name: String = if name.chars().count() > room {
                    let mut cut: String = name.chars().take(room.saturating_sub(1)).collect();
                    cut.push('…');
                    cut
                } else {
                    name.clone()
                };
                let style = Style::default().fg(standing.color(theme));
                let name_style = match standing {
                    Standing::Running => style.bold(),
                    Standing::Done => Style::default().fg(theme.entry()),
                    _ => style,
                };
                Line::from(vec![
                    Span::styled(format!("{} ", standing.mark()), style),
                    Span::styled(name, name_style),
                    Span::styled(exit, Style::default().fg(theme.failure())),
                ])
            })
            .collect();
        Paragraph::new(lines)
    }

    /// The warnings, then once it ends the result and each check: failures
    /// first, then those checked after the reboot or not at all.
    fn messages(&self, theme: Theme) -> Vec<Line<'static>> {
        let mut lines: Vec<Line> = self
            .warnings
            .iter()
            .map(|warning| {
                Line::styled(format!("⚠ {warning}"), Style::default().fg(theme.warning()))
            })
            .collect();
        let Some(finished) = &self.finished else {
            return lines;
        };
        let good = finished.succeeded();
        lines.push(Line::styled(
            format!(
                "{} Result: {}",
                if good { "✓" } else { "✗" },
                finished.describe()
            ),
            Style::default().fg(if good {
                theme.success()
            } else {
                theme.failure()
            }),
        ));
        let mut checks: Vec<&Check> = finished.checks.iter().collect();
        checks.sort_by_key(|check| {
            if check.later || check.unsure {
                1
            } else if check.passed {
                2
            } else {
                0
            }
        });
        for check in checks {
            let (mark, prefix, color) = if check.later {
                ("↻", "After the reboot: ", theme.muted())
            } else if check.unsure {
                ("i", "Not checked: ", theme.muted())
            } else if check.passed {
                ("✓", "", theme.faint())
            } else {
                ("✗", "", theme.failure())
            };
            lines.push(Line::styled(
                format!("{mark} {prefix}{}: {}", check.name, check.detail),
                Style::default().fg(color),
            ));
        }
        lines
    }
}

pub struct RunningView {
    session: RunSession,
    events: Receiver<ExecutionEvent>,
    /// The terminal's output, as the events brought it.
    output: Vec<u8>,
    /// The steps, warnings and end, as the events said.
    shown: Shown,
    recorded: bool,
    log: Option<PathBuf>,
    /// Why the history record or the log could not be written, if so.
    problem: Option<String>,
    scrollback: usize,
}

impl RunningView {
    /// Start a confirmed plan's run session in a pseudo-terminal.
    pub fn start(mut session: RunSession, lib_dir: &Path, this: &Path) -> Result<Self, String> {
        let events = session
            .start(PtyRunner::default(), lib_dir, this, Options::default())
            .map_err(|error| error.to_string())?;
        let shown = Shown::new(session.plan());
        Ok(Self {
            session,
            events,
            output: Vec::new(),
            shown,
            recorded: false,
            log: None,
            problem: None,
            scrollback: 0,
        })
    }

    pub fn is_finished(&self) -> bool {
        self.shown.finished.is_some()
    }

    fn finished(&self) -> Option<&Finished> {
        self.shown.finished.as_ref()
    }

    /// Take the events that arrived; true when any changes the window.
    pub fn needs_redraw(&mut self) -> bool {
        let mut changed = false;
        while let Ok(event) = self.events.try_recv() {
            match event {
                ExecutionEvent::Output { offset, bytes } => {
                    let offset = usize::try_from(offset).unwrap_or(usize::MAX);
                    if offset > self.output.len() {
                        // Never expected: the events lose nothing.
                        self.output = self.session.transcript();
                    } else if let Some(new) = bytes.get(self.output.len() - offset..) {
                        self.output.extend_from_slice(new);
                    }
                    changed = true;
                }
                event => changed |= self.shown.take(event),
            }
        }
        changed
    }

    /// Write the history record once, when the window closes, so a saved log
    /// is part of it.
    fn record(&mut self) {
        if self.recorded || !self.is_finished() {
            return;
        }
        self.recorded = true;
        if let Err(error) = self.session.record(self.log.clone()) {
            self.problem = Some(format!("The run was not recorded in the history: {error}"));
        }
    }

    /// The queued entries that stay queued after the run, of those `known`
    /// names: what did not finish.
    pub fn remaining(&self, known: &dyn Fn(&str) -> bool) -> Vec<String> {
        self.session.remaining(known)
    }

    /// What to read again now that the run is over.
    pub fn read_again(&self) -> Vec<Reading> {
        self.session.read_again()
    }

    /// Each step and how it ended, one per line.
    pub fn summary(&self) -> String {
        self.finished()
            .map(|finished| {
                finished
                    .steps
                    .iter()
                    .map(|step| format!("{}: {}", step.name, step.state.describe()))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default()
    }

    /// Why the run could not be recorded, once the window has closed.
    pub fn take_problem(&mut self) -> Option<String> {
        self.problem.take()
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect, theme: Theme) {
        let finished = self.finished().cloned();
        let (title, color) = match &finished {
            Some(finished) if finished.verified() => {
                (" FINISHED · VERIFIED ".to_string(), theme.success())
            }
            Some(finished) if finished.succeeded() => (" FINISHED ".to_string(), theme.success()),
            Some(Finished {
                outcome: Outcome::Success,
                ..
            }) => (" FINISHED · NOT VERIFIED ".to_string(), theme.failure()),
            Some(Finished {
                outcome: Outcome::Interrupted,
                ..
            }) => (" INTERRUPTED ".to_string(), theme.failure()),
            Some(finished) => {
                let done = finished
                    .steps
                    .iter()
                    .filter(|step| step.state == StepState::Done)
                    .count();
                let steps = if finished.steps.len() > 1 {
                    format!(" · {done} of {} steps done", finished.steps.len())
                } else {
                    String::new()
                };
                (
                    format!(" FAILED ({}){steps} ", finished.outcome.describe()),
                    theme.failure(),
                )
            }
            None => (" RUNNING ".to_string(), theme.focused()),
        };
        // What the checks found: the failures if any, else every pass.
        let checks: Vec<&helix_toolbox_engine::Check> = finished
            .iter()
            .flat_map(|finished| &finished.checks)
            .collect();
        let failed = checks.iter().any(|check| !check.passed);
        let found: Vec<String> = checks
            .iter()
            .filter(|check| !failed || !check.passed)
            .map(|check| format!("{}: {}", check.name, check.detail))
            .collect();
        let left = if self.is_finished() && self.session.stop_requested() {
            self.session.survivors()
        } else {
            0
        };
        let note = if left > 0 {
            format!(" {left} process(es) of this run still running; Ctrl-C again kills them ")
        } else if self.is_finished() {
            match (&self.log, &self.problem) {
                (_, Some(problem)) => format!(" {problem} "),
                (None, None) if failed => format!(" {} ", found.join("; ")),
                (Some(path), None) => format!(" Log saved to {} ", path.display()),
                (None, None) if !found.is_empty() => {
                    format!(" {} · Enter closes, l saves the log ", found.join("; "))
                }
                (None, None) => " Enter closes, l saves the log ".to_string(),
            }
        } else {
            " Ctrl-C stops the command ".to_string()
        };
        let block = Block::bordered()
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(color))
            .title(title)
            .title_alignment(Alignment::Center)
            .title_style(Style::default().fg(theme.title()).bold())
            .title_bottom(Line::from(note).centered())
            .padding(Padding::horizontal(1));
        let inner = block.inner(area);
        frame.render_widget(Clear, area);
        frame.render_widget(block, area);
        let messages = self.shown.messages(theme);
        let room = if inner.height >= MESSAGES_FROM_HEIGHT {
            (messages.len() as u16).min(inner.height / if finished.is_some() { 2 } else { 4 })
        } else {
            0
        };
        let [progress, body, below] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(room),
        ])
        .areas(inner);
        frame.render_widget(self.shown.progress_line(progress.width, theme), progress);
        let terminal = if inner.width >= STEPS_FROM_WIDTH && !self.shown.steps.is_empty() {
            let width = (inner.width / 3).min(36);
            let [steps, _, terminal] = Layout::horizontal([
                Constraint::Length(width),
                Constraint::Length(1),
                Constraint::Fill(1),
            ])
            .areas(body);
            frame.render_widget(self.shown.step_list(steps, theme), steps);
            terminal
        } else {
            body
        };
        if room > 0 {
            frame.render_widget(
                Paragraph::new(fit(messages, usize::from(room), theme)),
                below,
            );
        }
        self.session.resize(terminal.height, terminal.width);
        let mut parser = vt100::Parser::new(terminal.height.max(1), terminal.width.max(1), 2000);
        parser.process(&self.output);
        parser.screen_mut().set_scrollback(self.scrollback);
        frame.render_widget(PseudoTerminal::new(parser.screen()), terminal);
    }

    /// Scroll the output: back by a positive number of lines, forward by a
    /// negative one.
    pub fn scroll(&mut self, lines: isize) {
        self.scrollback = self.scrollback.saturating_add_signed(lines);
    }

    /// Returns true when the window should close.
    pub fn handle_key(&mut self, key: &KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.session.stop()
            }
            KeyCode::Enter | KeyCode::Char('q') if self.is_finished() => {
                self.record();
                return true;
            }
            KeyCode::Char('l') if self.is_finished() && self.log.is_none() => {
                match history::save_log(&self.session.transcript()) {
                    Ok(path) => self.log = Some(path),
                    Err(error) => self.problem = Some(format!("The log was not saved: {error}")),
                }
            }
            KeyCode::PageUp => self.scrollback += 10,
            KeyCode::PageDown => self.scrollback = self.scrollback.saturating_sub(10),
            _ => self.forward(key),
        }
        false
    }

    fn forward(&mut self, key: &KeyEvent) {
        let bytes: Vec<u8> = match key.code {
            KeyCode::Char(character) if key.modifiers.contains(KeyModifiers::CONTROL) => {
                match character.to_ascii_uppercase() {
                    '@' | ' ' | '2' => vec![0],
                    '[' | '3' => vec![27],
                    '\\' | '4' => vec![28],
                    ']' | '5' => vec![29],
                    '^' | '6' => vec![30],
                    '_' | '-' | '7' => vec![31],
                    letter @ 'A'..='_' => vec![letter as u8 - 64],
                    _ => character.to_string().into_bytes(),
                }
            }
            KeyCode::Char(character) => character.to_string().into_bytes(),
            KeyCode::Enter => vec![b'\r'],
            KeyCode::Backspace => vec![0x7f],
            KeyCode::Tab => vec![9],
            KeyCode::BackTab => b"\x1b[Z".to_vec(),
            KeyCode::Esc => vec![27],
            KeyCode::Up => b"\x1b[A".to_vec(),
            KeyCode::Down => b"\x1b[B".to_vec(),
            KeyCode::Right => b"\x1b[C".to_vec(),
            KeyCode::Left => b"\x1b[D".to_vec(),
            KeyCode::Home => b"\x1b[H".to_vec(),
            KeyCode::End => b"\x1b[F".to_vec(),
            KeyCode::Delete => b"\x1b[3~".to_vec(),
            KeyCode::Insert => b"\x1b[2~".to_vec(),
            _ => return,
        };
        self.session.write(&bytes);
    }

    pub fn hints(&self) -> (&'static str, Vec<Hint>) {
        if self.is_finished() {
            (
                "Finished",
                vec![
                    Hint::new(&["Enter", "q"], "Close"),
                    Hint::new(&["l"], "Save the log"),
                    Hint::new(&["Page up", "Page down"], "Scroll"),
                ],
            )
        } else {
            (
                "Running",
                vec![
                    Hint::new(&["Ctrl-c"], "Stop"),
                    Hint::new(&["Page up", "Page down"], "Scroll"),
                ],
            )
        }
    }
}

/// At most `room` lines; the last says how many more there are.
fn fit(mut lines: Vec<Line<'static>>, room: usize, theme: Theme) -> Vec<Line<'static>> {
    if lines.len() > room && room > 0 {
        let more = lines.len() - room + 1;
        lines.truncate(room - 1);
        lines.push(Line::styled(
            format!("… and {more} more"),
            Style::default().fg(theme.faint()),
        ));
    }
    lines
}

impl Drop for RunningView {
    fn drop(&mut self) {
        self.record();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use helix_toolbox_engine::{StepResult, execution::StepInfo};

    fn info(index: usize, name: &str) -> StepInfo {
        StepInfo {
            index,
            id: name.to_lowercase(),
            name: name.to_string(),
        }
    }

    fn text(line: &Line) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    fn texts(lines: &[Line]) -> Vec<String> {
        lines.iter().map(text).collect()
    }

    /// Three steps: the first done, the second failed, the third skipped.
    fn after_a_failure() -> Shown {
        let mut shown = Shown::default();
        let steps = vec![info(0, "Ripgrep"), info(1, "Meld"), info(2, "Meld's theme")];
        shown.take(ExecutionEvent::RunStarted {
            entries: Vec::new(),
            steps: steps.clone(),
        });
        shown.take(ExecutionEvent::StepStarted(steps[0].clone()));
        assert_eq!(shown.steps[0].1, Standing::Running);
        shown.take(ExecutionEvent::StepCompleted(steps[0].clone()));
        shown.take(ExecutionEvent::Progress { ended: 1, total: 3 });
        shown.take(ExecutionEvent::Warning("the mirror is slow".into()));
        // The note along the bottom keeps this count, as it changes.
        shown.take(ExecutionEvent::Warning(
            "2 process(es) of this run are still running".into(),
        ));
        shown.take(ExecutionEvent::StepStarted(steps[1].clone()));
        shown.take(ExecutionEvent::StepFailed {
            step: steps[1].clone(),
            exit_code: 7,
            interrupted: false,
        });
        shown.take(ExecutionEvent::StepSkipped(steps[2].clone()));
        shown.take(ExecutionEvent::Progress { ended: 3, total: 3 });
        shown
    }

    #[test]
    fn each_step_is_marked_as_its_events_say() {
        let shown = after_a_failure();
        assert_eq!(
            shown.steps,
            [
                ("Ripgrep".to_string(), Standing::Done),
                ("Meld".to_string(), Standing::Failed(7)),
                ("Meld's theme".to_string(), Standing::Skipped),
            ]
        );
        let theme = Theme::default();
        let progress = text(&shown.progress_line(30, theme));
        assert!(progress.ends_with(" 3 of 3 steps ended"), "{progress}");
        assert_eq!(progress.chars().count(), 30);
        let list = format!("{:?}", shown.step_list(Rect::new(0, 0, 24, 5), theme));
        assert!(
            list.contains("Ripgrep") && list.contains(" exit 7"),
            "{list}"
        );
        // The warning is shown while it runs, too.
        assert_eq!(texts(&shown.messages(theme)), ["⚠ the mirror is slow"]);
    }

    #[test]
    fn the_end_gives_the_result_and_each_check_failures_first() {
        let mut shown = after_a_failure();
        let check = |name: &str, passed, unsure, later| Check {
            name: name.to_string(),
            passed,
            detail: "found".to_string(),
            unsure,
            later,
        };
        let result = |id: &str, state| StepResult {
            id: id.to_string(),
            name: id.to_string(),
            state,
        };
        shown.take(ExecutionEvent::Completed(Finished {
            outcome: Outcome::Failed { exit_code: 7 },
            checks: vec![
                check("Ripgrep", true, false, false),
                check("SDDM theme", true, false, true),
                check("Firmware", true, true, false),
                check("Meld", false, false, false),
            ],
            steps: vec![
                result("ripgrep", StepState::Done),
                result("meld", StepState::Failed(7)),
                result("theme", StepState::NotRun),
            ],
        }));
        // The end's own word wins: the third step was not run.
        assert_eq!(shown.steps[2].1, Standing::NotRun);
        let lines = texts(&shown.messages(Theme::default()));
        assert_eq!(lines[0], "⚠ the mirror is slow");
        assert!(lines[1].starts_with("✗ Result: exit 7"), "{}", lines[1]);
        assert_eq!(
            lines[2..],
            [
                "✗ Meld: found",
                "↻ After the reboot: SDDM theme: found",
                "i Not checked: Firmware: found",
                "✓ Ripgrep: found",
            ]
        );
    }

    #[test]
    fn messages_that_do_not_fit_say_how_many_more() {
        let lines: Vec<Line> = (0..6).map(|n| Line::from(format!("line {n}"))).collect();
        let fitted = texts(&fit(lines.clone(), 3, Theme::default()));
        assert_eq!(fitted, ["line 0", "line 1", "… and 4 more"]);
        assert_eq!(fit(lines, 6, Theme::default()).len(), 6);
    }

    #[test]
    fn a_long_list_follows_the_running_step() {
        let mut shown = Shown::default();
        let steps: Vec<StepInfo> = (0..20).map(|n| info(n, &format!("Step {n}"))).collect();
        shown.take(ExecutionEvent::RunStarted {
            entries: Vec::new(),
            steps: steps.clone(),
        });
        shown.take(ExecutionEvent::StepStarted(steps[15].clone()));
        let list = format!(
            "{:?}",
            shown.step_list(Rect::new(0, 0, 20, 5), Theme::default())
        );
        assert!(
            list.contains("Step 15") && !list.contains("Step 1\""),
            "{list}"
        );
    }
}
