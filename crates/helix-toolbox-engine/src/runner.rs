//! The seam a run session drives its program through.
//!
//! A [`Runner`] starts a run's shell program and answers how it is going:
//! whether it has ended and how, what it wrote to its terminal, how many of
//! its processes are left. The run session does everything else the same
//! way whichever runner it has: the requirements, the marker, the steps and
//! their events, the checks, the record. [`PtyRunner`] runs the program in a
//! pseudo-terminal (the terminal interface and the window),
//! [`InheritedRunner`] in the caller's own terminal (the command line), and
//! [`MockRunner`] runs nothing, for tests.

use crate::{EngineError, LIB_VARIABLE, OLD_LIB_VARIABLE, Outcome, pty};
use std::{
    path::Path,
    process::{Child, Command},
    sync::{Arc, Mutex, mpsc::SyncSender},
};

/// What a runner is given to start a run's program.
pub struct Launch<'a> {
    /// The shell program, from `shell_program_with`.
    pub program: &'a str,
    /// The catalog's script folder, exported to the run.
    pub lib_dir: &'a Path,
    /// Where the program writes each step's outcome, if anywhere.
    pub steps: Option<&'a Path>,
    /// The terminal's first size.
    pub rows: u16,
    pub cols: u16,
    /// Rung, without waiting, whenever output arrives or the program ends.
    pub bell: SyncSender<()>,
}

/// Something that runs a run's shell program for the run session: a
/// pseudo-terminal, the caller's terminal, or a test double. Nothing here
/// waits for the program.
pub trait Runner: Send {
    /// Start the program. An error means nothing started.
    fn start(&mut self, launch: Launch<'_>) -> Result<(), EngineError>;

    /// How the program ended, once it has.
    fn outcome(&mut self) -> Option<Outcome>;

    /// What the program has written to its terminal, shared as it grows;
    /// empty when its output goes elsewhere.
    fn transcript(&self) -> Arc<Mutex<Vec<u8>>>;

    /// Whether output can still arrive.
    fn reading(&self) -> bool {
        false
    }

    /// Send keyboard input to the program.
    fn write(&mut self, _bytes: &[u8]) {}

    /// Give the terminal a new size.
    fn resize(&mut self, _rows: u16, _cols: u16) {}

    /// Stop the run: the first request hangs it up, a later one kills what
    /// is left.
    fn stop(&mut self) {}

    /// Whether a stop was requested.
    fn stop_requested(&self) -> bool {
        false
    }

    /// How many processes of the run are still alive; 0 where it cannot
    /// tell.
    fn survivors(&mut self) -> usize {
        0
    }
}

/// Runs the program in a pseudo-terminal of its own, in its own process
/// group, for an interface that draws the output.
#[derive(Default)]
pub struct PtyRunner {
    session: Option<pty::Session>,
    empty: Arc<Mutex<Vec<u8>>>,
}

impl Runner for PtyRunner {
    fn start(&mut self, launch: Launch<'_>) -> Result<(), EngineError> {
        self.session = Some(pty::Session::spawn_sized(
            launch.program,
            launch.lib_dir,
            launch.rows,
            launch.cols,
            Some(launch.bell),
        )?);
        Ok(())
    }

    fn outcome(&mut self) -> Option<Outcome> {
        self.session.as_mut().and_then(pty::Session::outcome)
    }

    fn transcript(&self) -> Arc<Mutex<Vec<u8>>> {
        self.session
            .as_ref()
            .map_or_else(|| self.empty.clone(), pty::Session::transcript)
    }

    fn reading(&self) -> bool {
        self.session.as_ref().is_some_and(pty::Session::reading)
    }

    fn write(&mut self, bytes: &[u8]) {
        if let Some(session) = &mut self.session {
            session.write(bytes);
        }
    }

    fn resize(&mut self, rows: u16, cols: u16) {
        if let Some(session) = &self.session {
            session.resize(rows, cols);
        }
    }

    fn stop(&mut self) {
        if let Some(session) = &mut self.session {
            session.kill();
        }
    }

    fn stop_requested(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(pty::Session::stop_requested)
    }

    fn survivors(&mut self) -> usize {
        self.session.as_mut().map_or(0, pty::Session::survivors)
    }
}

/// Runs the program in the caller's terminal, inheriting its input and
/// output; Ctrl-C there reaches the program directly.
#[derive(Default)]
pub struct InheritedRunner {
    child: Option<Child>,
    ended: Option<Outcome>,
    empty: Arc<Mutex<Vec<u8>>>,
}

impl Runner for InheritedRunner {
    fn start(&mut self, launch: Launch<'_>) -> Result<(), EngineError> {
        match Command::new("sh")
            .arg("-c")
            .arg(launch.program)
            .env(LIB_VARIABLE, launch.lib_dir)
            .env(OLD_LIB_VARIABLE, launch.lib_dir)
            .spawn()
        {
            Ok(child) => self.child = Some(child),
            // Recorded like any run, as one that could not start.
            Err(error) => self.ended = Some(Outcome::CouldNotStart(error.to_string())),
        }
        Ok(())
    }

    fn outcome(&mut self) -> Option<Outcome> {
        if self.ended.is_none()
            && let Some(child) = &mut self.child
        {
            self.ended = match child.try_wait() {
                Ok(Some(status)) => Some(Outcome::from_status(&status)),
                Ok(None) => None,
                Err(error) => Some(Outcome::CouldNotStart(error.to_string())),
            };
        }
        self.ended.clone()
    }

    fn transcript(&self) -> Arc<Mutex<Vec<u8>>> {
        self.empty.clone()
    }
}

/// Runs nothing, for tests: it keeps the programs it is given, writes the
/// step lines and output it is told to, and ends as it is told.
#[derive(Clone, Default)]
pub struct MockRunner {
    /// The programs it was given, shared with the test that made it.
    pub programs: Arc<Mutex<Vec<String>>>,
    /// How the program ends; success when not set.
    pub outcome: Option<Outcome>,
    /// Lines written to the step file as the program would
    /// (`index state exit`).
    pub steps: Vec<String>,
    /// What the program writes to its terminal.
    pub output: Vec<u8>,
    /// How many processes it says are left after the end.
    pub left: usize,
    /// What it has done so far, which it keeps itself.
    pub done: MockState,
}

/// A [`MockRunner`]'s own record of its run.
#[derive(Clone, Default)]
pub struct MockState {
    transcript: Arc<Mutex<Vec<u8>>>,
    started: bool,
    stopped: bool,
}

impl Runner for MockRunner {
    fn start(&mut self, launch: Launch<'_>) -> Result<(), EngineError> {
        self.programs
            .lock()
            .unwrap()
            .push(launch.program.to_string());
        if let Some(path) = launch.steps {
            let lines: String = self.steps.iter().map(|line| format!("{line}\n")).collect();
            std::fs::write(path, lines).map_err(EngineError::Spawn)?;
        }
        self.done
            .transcript
            .lock()
            .unwrap()
            .extend_from_slice(&self.output);
        self.done.started = true;
        let _ = launch.bell.try_send(());
        Ok(())
    }

    fn outcome(&mut self) -> Option<Outcome> {
        if !self.done.started {
            return None;
        }
        Some(if self.done.stopped {
            Outcome::Interrupted
        } else {
            self.outcome.clone().unwrap_or(Outcome::Success)
        })
    }

    fn transcript(&self) -> Arc<Mutex<Vec<u8>>> {
        self.done.transcript.clone()
    }

    fn stop(&mut self) {
        self.done.stopped = true;
    }

    fn stop_requested(&self) -> bool {
        self.done.stopped
    }

    fn survivors(&mut self) -> usize {
        self.left
    }
}
