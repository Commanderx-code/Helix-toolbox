//! The window that shows entries running in their pseudo-terminal.

use super::{hints::Hint, theme::Theme};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use helixforge_engine::{Outcome, Plan, history, pty::Session, shell_program};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    symbols::border,
    text::Line,
    widgets::Block,
};
use std::path::{Path, PathBuf};
use tui_term::widget::PseudoTerminal;

pub struct RunningView {
    session: Session,
    entries: Vec<(String, String)>,
    recorded: bool,
    log: Option<PathBuf>,
    scrollback: usize,
    finish_drawn: bool,
}

impl RunningView {
    pub fn start(plan: &Plan, lib_dir: &Path, this: &Path) -> Result<Self, String> {
        helixforge_engine::check_requirements(plan).map_err(|error| error.to_string())?;
        tracing::info!(steps = ?plan.entries(), "run started");
        let session = Session::spawn(&shell_program(plan, lib_dir, this), lib_dir)
            .map_err(|error| error.to_string())?;
        Ok(Self {
            session,
            entries: plan.entries(),
            recorded: false,
            log: None,
            scrollback: 0,
            finish_drawn: false,
        })
    }

    pub fn is_finished(&self) -> bool {
        self.session.is_finished()
    }

    /// True once after the program finishes, so the screen shows it even
    /// when no further output or key arrives.
    pub fn needs_redraw(&mut self) -> bool {
        if self.is_finished() && !self.finish_drawn {
            self.finish_drawn = true;
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
        let outcome = self
            .session
            .outcome()
            .unwrap_or_else(|| Outcome::CouldNotStart("unknown".into()));
        tracing::info!(outcome = outcome.describe(), "run finished");
        let _ = history::record(
            &self.entries,
            outcome.succeeded(),
            &outcome.describe(),
            self.log.clone(),
        );
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect, theme: Theme) {
        let (title, colour) = if self.is_finished() {
            match self.session.outcome() {
                Some(Outcome::Success) => (" FINISHED ".to_string(), theme.success()),
                Some(Outcome::Interrupted) => (" INTERRUPTED ".to_string(), theme.failure()),
                Some(outcome) => (
                    format!(" FAILED ({}) ", outcome.describe()),
                    theme.failure(),
                ),
                None => (" FAILED ".to_string(), theme.failure()),
            }
        } else {
            (" RUNNING ".to_string(), theme.focused())
        };
        let note = if self.is_finished() {
            match &self.log {
                Some(path) => format!(" Log saved to {} ", path.display()),
                None => " Enter closes, l saves the log ".to_string(),
            }
        } else {
            " Ctrl-C stops the command ".to_string()
        };
        let block = Block::bordered()
            .border_set(border::PLAIN)
            .border_style(Style::default().fg(colour))
            .title(title)
            .title_alignment(Alignment::Center)
            .title_style(Style::default().fg(theme.title()).bold())
            .title_bottom(Line::from(note).centered());
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
                if let Ok(path) = history::save_log(&self.session.output()) {
                    self.log = Some(path);
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
    }
}
