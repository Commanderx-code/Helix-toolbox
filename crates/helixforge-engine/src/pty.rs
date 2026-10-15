//! A shell program running in a pseudo-terminal, with its output collected
//! for an interface to draw and its input forwarded from the keyboard.

use crate::{EngineError, LIB_VARIABLE};
use portable_pty::{
    ChildKiller, CommandBuilder, ExitStatus, MasterPty, NativePtySystem, PtySize, PtySystem,
};
use std::{
    io::{Read, Write},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, channel},
    },
    thread::JoinHandle,
};

/// Set whenever new output arrived, for interfaces that redraw on demand.
pub static OUTPUT_CHANGED: AtomicBool = AtomicBool::new(false);

pub struct Session {
    output: Arc<Mutex<Vec<u8>>>,
    waiter: Option<JoinHandle<Option<ExitStatus>>>,
    killer: Option<Receiver<Box<dyn ChildKiller + Send + Sync>>>,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    status: Option<Option<ExitStatus>>,
}

impl Session {
    pub fn spawn(program: &str, lib_dir: &Path) -> Result<Self, EngineError> {
        let mut builder = CommandBuilder::new("sh");
        builder.args(["-c", program]);
        builder.env("TERM", "xterm-256color");
        builder.env("COLORTERM", "truecolor");
        builder.env(LIB_VARIABLE, lib_dir);
        let pair = NativePtySystem::default()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| EngineError::Spawn(std::io::Error::other(error)))?;
        let (sender, receiver) = channel();
        let waiter = std::thread::spawn(move || {
            let mut child = pair.slave.spawn_command(builder).ok()?;
            let _ = sender.send(child.clone_killer());
            child.wait().ok()
        });
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| EngineError::Spawn(std::io::Error::other(error)))?;
        let output: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let shared = output.clone();
        std::thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            while let Ok(size) = reader.read(&mut buffer) {
                if size == 0 {
                    break;
                }
                shared.lock().unwrap().extend_from_slice(&buffer[..size]);
                OUTPUT_CHANGED.store(true, Ordering::Release);
            }
            OUTPUT_CHANGED.store(true, Ordering::Release);
        });
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| EngineError::Spawn(std::io::Error::other(error)))?;
        OUTPUT_CHANGED.store(true, Ordering::Release);
        Ok(Self {
            output,
            waiter: Some(waiter),
            killer: Some(receiver),
            master: pair.master,
            writer,
            status: None,
        })
    }

    /// Everything the program has written so far.
    pub fn output(&self) -> Vec<u8> {
        self.output.lock().unwrap().clone()
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

    pub fn kill(&mut self) {
        if let Some(receiver) = self.killer.take()
            && let Ok(mut killer) = receiver.recv()
        {
            let _ = killer.kill();
        }
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
    }
}
