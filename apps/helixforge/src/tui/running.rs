//! The window that shows entries running in their pseudo-terminal.

use super::{hints::Hint, theme::Theme};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use helixforge_engine::{
    Finished, Outcome, Plan, StepState, history, pty::Session, shell_program_with, status_file,
};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    symbols::border,
    text::Line,
    widgets::{Block, Padding},
};
use std::path::{Path, PathBuf};
use tui_term::widget::PseudoTerminal;

pub struct RunningView {
    session: Session,
    plan: Plan,
    /// The outcome with the checks of its package steps, once it ends.
    finished: Option<Finished>,
    entries: Vec<(String, String)>,
    /// The history's marker for this run, until the record is written.
    pending: Option<history::Pending>,
    recorded: bool,
    log: Option<PathBuf>,
    /// Why the history record or the log could not be written, if so.
    problem: Option<String>,
    scrollback: usize,
    finish_drawn: bool,
    /// Where the run's program writes each step's outcome.
    status: Option<PathBuf>,
}

impl RunningView {
    pub fn start(plan: &Plan, lib_dir: &Path, this: &Path) -> Result<Self, String> {
        helixforge_engine::check_requirements(plan).map_err(|error| error.to_string())?;
        tracing::info!(steps = ?plan.entries(), "run started");
        let status = status_file();
        let session = Session::spawn(
            &shell_program_with(plan, lib_dir, this, status.as_deref()),
            lib_dir,
        )
        .map_err(|error| error.to_string())?;
        let pending = history::begin(&plan.entries())
            .inspect_err(|error| tracing::warn!(%error, "the run start was not recorded"))
            .ok();
        Ok(Self {
            session,
            plan: plan.clone(),
            finished: None,
            entries: plan.entries(),
            pending,
            recorded: false,
            log: None,
            problem: None,
            scrollback: 0,
            finish_drawn: false,
            status,
        })
    }

    pub fn is_finished(&self) -> bool {
        self.session.is_finished()
    }

    /// The finished run, checked once when the program ends.
    fn finished(&mut self) -> Option<&Finished> {
        if self.finished.is_none() && self.is_finished() {
            let outcome = self
                .session
                .outcome()
                .unwrap_or_else(|| Outcome::CouldNotStart("unknown".into()));
            self.finished = Some(Finished::from_run(
                &self.plan,
                outcome,
                self.status.as_deref(),
            ));
        }
        self.finished.as_ref()
    }

    /// True once after the program finishes, so the screen shows it even
    /// when no further output or key arrives.
    pub fn needs_redraw(&mut self) -> bool {
        if self.is_finished() && !self.finish_drawn {
            self.finish_drawn = true;
            // The record waits for the window to close; until then the
            // marker keeps the outcome in case HelixForge stops first.
            if let Some(finished) = self.finished().cloned()
                && let Some(pending) = &mut self.pending
            {
                let _ = pending.ended(finished.succeeded(), &finished.describe());
            }
            return true;
        }
        false
    }

    /// Write the history record once, when the window closes, so a saved log
    /// is part of it.
    fn record(&mut self) {
        if self.recorded || !self.is_finished() {
            return;
        }
        self.recorded = true;
        let Some(finished) = self.finished().cloned() else {
            return;
        };
        tracing::info!(outcome = finished.describe(), "run finished");
        let mut result = finished.describe();
        let left = self.session.survivors();
        if left > 0 {
            result.push_str(&format!("; {left} process(es) left running"));
        }
        let written = match self.pending.take() {
            Some(pending) => pending.finish(finished.succeeded(), &result, self.log.clone()),
            None => history::record(
                &self.entries,
                finished.succeeded(),
                &result,
                self.log.clone(),
            ),
        };
        if let Err(error) = written {
            tracing::warn!(%error, "the run was not recorded");
            self.problem = Some(format!("The run was not recorded in the history: {error}"));
        }
    }

    /// The ids of the steps that did not finish, once the run has ended.
    pub fn unfinished(&mut self) -> Vec<String> {
        self.finished()
            .map(|finished| {
                finished
                    .unfinished()
                    .iter()
                    .map(|step| step.id.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Each step and how it ended, one per line.
    pub fn summary(&mut self) -> String {
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
        let checks: Vec<&helixforge_engine::Check> = finished
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
        self.session.resize(inner.height, inner.width);
        let mut parser = vt100::Parser::new(inner.height.max(1), inner.width.max(1), 2000);
        parser.process(&self.session.output());
        parser.screen_mut().set_scrollback(self.scrollback);
        frame.render_widget(PseudoTerminal::new(parser.screen()).block(block), area);
    }

    /// Returns true when the window should close.
    pub fn handle_key(&mut self, key: &KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.session.kill()
            }
            KeyCode::Enter | KeyCode::Char('q') if self.is_finished() => {
                self.record();
                return true;
            }
            KeyCode::Char('l') if self.is_finished() && self.log.is_none() => {
                match history::save_log(&self.session.output()) {
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

impl Drop for RunningView {
    fn drop(&mut self) {
        self.record();
        if let Some(status) = &self.status {
            let _ = std::fs::remove_file(status);
        }
    }
}
