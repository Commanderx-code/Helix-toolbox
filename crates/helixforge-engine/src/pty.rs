//! A shell program running in a pseudo-terminal, with its output collected
//! for an interface to draw and its input forwarded from the keyboard.
//!
//! The shell leads its own session and process group, and everything it starts
//! stays in that group unless it leaves on purpose. Stopping a run signals the
//! whole group, so a child that outlives the shell is still reached.

use crate::{EngineError, LIB_VARIABLE};
use nix::{
    sys::signal::{Signal, killpg},
    unistd::Pid,
};
use portable_pty::{CommandBuilder, ExitStatus, MasterPty, NativePtySystem, PtySize, PtySystem};
use std::{
    io::{Read, Write},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, SyncSender, channel},
    },
    thread::JoinHandle,
};

/// Set whenever new output arrived, for interfaces that redraw on demand.
pub static OUTPUT_CHANGED: AtomicBool = AtomicBool::new(false);

pub struct Session {
    output: Arc<Mutex<Vec<u8>>>,
    /// True while the output is still being read.
    reading: Arc<AtomicBool>,
    waiter: Option<JoinHandle<Option<ExitStatus>>>,
    /// Delivers the shell's process id, which is also its process group.
    started: Option<Receiver<u32>>,
    group: Option<Pid>,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    status: Option<Option<ExitStatus>>,
    killed: bool,
}

impl Session {
    pub fn spawn(program: &str, lib_dir: &Path) -> Result<Self, EngineError> {
        Self::spawn_sized(program, lib_dir, 24, 80, None)
    }

    /// `spawn` with the terminal's first size, ringing `bell` whenever
    /// output arrives.
    pub(crate) fn spawn_sized(
        program: &str,
        lib_dir: &Path,
        rows: u16,
        cols: u16,
        bell: Option<SyncSender<()>>,
    ) -> Result<Self, EngineError> {
        let mut builder = CommandBuilder::new("sh");
        builder.args(["-c", program]);
        builder.env("TERM", "xterm-256color");
        builder.env("COLORTERM", "truecolor");
        builder.env(LIB_VARIABLE, lib_dir);
        let pair = NativePtySystem::default()
            .openpty(PtySize {
                rows: rows.max(1),
                cols: cols.max(1),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| EngineError::Spawn(std::io::Error::other(error)))?;
        let (sender, receiver) = channel();
        let waiter = std::thread::spawn(move || {
            let mut child = pair.slave.spawn_command(builder).ok()?;
            if let Some(pid) = child.process_id() {
                let _ = sender.send(pid);
            }
            child.wait().ok()
        });
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| EngineError::Spawn(std::io::Error::other(error)))?;
        let output: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let shared = output.clone();
        let reading = Arc::new(AtomicBool::new(true));
        let still_reading = reading.clone();
        let ring = move || {
            OUTPUT_CHANGED.store(true, Ordering::Release);
            if let Some(bell) = &bell {
                // A full bell has already been rung.
                let _ = bell.try_send(());
            }
        };
        std::thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            while let Ok(size) = reader.read(&mut buffer) {
                if size == 0 {
                    break;
                }
                shared.lock().unwrap().extend_from_slice(&buffer[..size]);
                ring();
            }
            still_reading.store(false, Ordering::Release);
            ring();
        });
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| EngineError::Spawn(std::io::Error::other(error)))?;
        OUTPUT_CHANGED.store(true, Ordering::Release);
        Ok(Self {
            output,
            reading,
            waiter: Some(waiter),
            started: Some(receiver),
            group: None,
            master: pair.master,
            writer,
            status: None,
            killed: false,
        })
    }

    /// How the run ended, once it has.
    pub fn outcome(&mut self) -> Option<crate::Outcome> {
        if !self.is_finished() {
            return None;
        }
        Some(match self.exit_status() {
            _ if self.killed => crate::Outcome::Interrupted,
            Some(status) if status.success() => crate::Outcome::Success,
            Some(status) => {
                let code = status.exit_code();
                if code >= 128 {
                    crate::Outcome::Interrupted
                } else {
                    crate::Outcome::Failed { exit_code: code }
                }
            }
            None => crate::Outcome::CouldNotStart("the shell did not start".into()),
        })
    }

    /// Everything the program has written so far.
    pub fn output(&self) -> Vec<u8> {
        self.output.lock().unwrap().clone()
    }

    /// The output as it grows, shared with the thread that reads it.
    pub(crate) fn transcript(&self) -> Arc<Mutex<Vec<u8>>> {
        self.output.clone()
    }

    /// Whether output can still arrive: false once every process holding
    /// the terminal has closed it.
    pub(crate) fn reading(&self) -> bool {
        self.reading.load(Ordering::Acquire)
    }

    pub fn is_finished(&self) -> bool {
        self.waiter.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// The exit status once the program has finished; blocks until then.
    pub fn exit_status(&mut self) -> Option<ExitStatus> {
        if self.status.is_none()
            && let Some(waiter) = self.waiter.take()
        {
            self.status = Some(waiter.join().ok().flatten());
        }
        self.status.clone().flatten()
    }

    /// The shell's process group, once it has started.
    fn group(&mut self) -> Option<Pid> {
        if self.group.is_none()
            && let Some(receiver) = self.started.take()
        {
            // The sender is dropped without sending only when the spawn failed.
            if let Ok(pid) = receiver.recv() {
                self.group = i32::try_from(pid).ok().map(Pid::from_raw);
            }
        }
        self.group
    }

    /// Stop the run. The first request sends the hangup a closing terminal
    /// would send to every process in the run's group; a later one, while any
    /// of them is still running, kills them outright.
    pub fn kill(&mut self) {
        let Some(group) = self.group() else {
            return;
        };
        if !self.killed {
            self.killed = true;
            match killpg(group, Signal::SIGHUP) {
                Ok(()) => tracing::info!("run stopped by request"),
                Err(error) => tracing::warn!(%error, "the run could not be stopped"),
            }
        } else if self.survivors() > 0 {
            // The group still has members, so its id cannot have been reused.
            match killpg(group, Signal::SIGKILL) {
                Ok(()) => tracing::info!("remaining processes killed by request"),
                Err(error) => tracing::warn!(%error, "the remaining processes could not be killed"),
            }
        }
    }

    /// How many processes of the run are still alive, for example ones that
    /// ignored the hangup after the shell itself ended.
    pub fn survivors(&mut self) -> usize {
        let Some(group) = self.group() else {
            return 0;
        };
        let Ok(listing) = std::fs::read_dir("/proc") else {
            return 0;
        };
        listing
            .flatten()
            .filter(|item| {
                item.file_name()
                    .to_str()
                    .is_some_and(|name| name.bytes().all(|b| b.is_ascii_digit()))
            })
            .filter_map(|item| std::fs::read_to_string(item.path().join("stat")).ok())
            .filter(|stat| {
                // After the command name: state, parent, process group, ...
                let mut fields = stat
                    .rsplit_once(')')
                    .map(|(_, rest)| rest)
                    .unwrap_or("")
                    .split_whitespace();
                let state = fields.next().unwrap_or("Z");
                let process_group = fields.nth(1).and_then(|field| field.parse::<i32>().ok());
                state != "Z" && state != "X" && process_group == Some(group.as_raw())
            })
            .count()
    }

    /// True once a stop was requested.
    pub fn stop_requested(&self) -> bool {
        self.killed
    }

    pub fn resize(&self, rows: u16, cols: u16) {
        let _ = self.master.resize(PtySize {
            rows: rows.max(1),
            cols: cols.max(1),
            pixel_width: 0,
            pixel_height: 0,
        });
    }

    /// Send keyboard input to the program.
    pub fn write(&mut self, bytes: &[u8]) {
        let _ = self.writer.write_all(bytes);
        let _ = self.writer.flush();
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if !self.is_finished() {
            self.kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn output_and_exit_status_come_back() {
        let mut session = Session::spawn(
            "printf 'hello %s\\n' \"$HELIXFORGE_LIB\"; exit 3",
            Path::new("/lib/dir"),
        )
        .unwrap();
        let start = Instant::now();
        while !session.is_finished() && start.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(
            session.exit_status().map(|status| status.exit_code()),
            Some(3)
        );
        let start = Instant::now();
        while !String::from_utf8_lossy(&session.output()).contains("hello /lib/dir")
            && start.elapsed() < Duration::from_secs(5)
        {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(String::from_utf8_lossy(&session.output()).contains("hello /lib/dir"));
    }

    #[test]
    fn input_reaches_the_program_and_kill_stops_it() {
        let mut session =
            Session::spawn("read -r line; echo \"got $line\"; sleep 30", Path::new("/")).unwrap();
        session.write(b"abc\r");
        let start = Instant::now();
        while !String::from_utf8_lossy(&session.output()).contains("got abc")
            && start.elapsed() < Duration::from_secs(5)
        {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(String::from_utf8_lossy(&session.output()).contains("got abc"));
        assert!(!session.is_finished());
        session.kill();
        let start = Instant::now();
        while !session.is_finished() && start.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(session.is_finished());
        assert_eq!(session.outcome(), Some(crate::Outcome::Interrupted));
    }

    #[test]
    fn a_child_that_ignores_the_hangup_is_reported_and_killed_on_the_second_stop() {
        let directory = tempfile::tempdir().unwrap();
        let pid_file = directory.path().join("pid");
        let program = format!(
            "(trap '' HUP; sh -c 'echo $$ > {}; exec sleep 300')",
            pid_file.display()
        );
        let mut session = Session::spawn(&program, Path::new("/")).unwrap();
        let start = Instant::now();
        while std::fs::read_to_string(&pid_file).map_or(true, |text| text.trim().is_empty())
            && start.elapsed() < Duration::from_secs(5)
        {
            std::thread::sleep(Duration::from_millis(20));
        }
        let child = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .to_string();
        let alive = || {
            std::fs::read_to_string(format!("/proc/{child}/stat")).is_ok_and(|stat| {
                !stat
                    .rsplit_once(')')
                    .unwrap()
                    .1
                    .trim_start()
                    .starts_with('Z')
            })
        };
        session.kill();
        let start = Instant::now();
        while !session.is_finished() && start.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(session.outcome(), Some(crate::Outcome::Interrupted));
        std::thread::sleep(Duration::from_millis(200));
        assert!(alive(), "the child should have ignored the hangup");
        assert_eq!(session.survivors(), 1);
        session.kill();
        let start = Instant::now();
        while alive() && start.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!alive(), "the second stop must kill the child");
        assert_eq!(session.survivors(), 0);
    }
}
