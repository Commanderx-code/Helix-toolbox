//! Drive the real `helixforge` program in a pseudo-terminal against the
//! fixture catalog: keyboard navigation, search, the install plan, the
//! confirmation, typing into a running entry, stopping one with Ctrl-C, the
//! history screen and terminal resizing. Nothing outside temporary
//! directories is touched.

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

const PROGRAM: &str = env!("CARGO_BIN_EXE_helixforge");
const PATIENCE: Duration = Duration::from_secs(20);

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

/// A running `helixforge` in a pseudo-terminal, with a parsed screen.
struct Terminal {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    screen: Arc<Mutex<vt100::Parser>>,
    state: PathBuf,
}

impl Terminal {
    fn start(args: &[&str], state: &Path, rows: u16, cols: u16) -> Self {
        let mut command = CommandBuilder::new(PROGRAM);
        command.args(args);
        Self::spawn(command, state, rows, cols)
    }

    fn spawn(mut command: CommandBuilder, state: &Path, rows: u16, cols: u16) -> Self {
        let pty = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        command.env("HOME", state);
        command.env("XDG_STATE_HOME", state.join("state"));
        command.env("TMPDIR", state.join("tmp"));
        command.env("HELIXFORGE_TEST_OUT", state.join("marker.txt"));
        command.env("TERM", "xterm-256color");
        fs::create_dir_all(state.join("tmp")).unwrap();
        let child = pty.slave.spawn_command(command).unwrap();
        drop(pty.slave);
        let screen = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 0)));
        let mut reader = pty.master.try_clone_reader().unwrap();
        let parser = Arc::clone(&screen);
        thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            while let Ok(count) = reader.read(&mut buffer) {
                if count == 0 {
                    break;
                }
                parser.lock().unwrap().process(&buffer[..count]);
            }
        });
        let writer = pty.master.take_writer().unwrap();
        Self {
            master: pty.master,
            writer,
            child,
            screen,
            state: state.to_path_buf(),
        }
    }

    fn contents(&self) -> String {
        self.screen.lock().unwrap().screen().contents()
    }

    /// Wait until the screen shows every one of `texts`.
    fn wait_for(&self, texts: &[&str]) {
        self.wait_until(&format!("{texts:?}"), |screen| {
            texts.iter().all(|text| screen.contains(text))
        });
    }

    fn wait_until(&self, what: &str, test: impl Fn(&str) -> bool) {
        let start = Instant::now();
        loop {
            let screen = self.contents();
            if test(&screen) {
                return;
            }
            if start.elapsed() > PATIENCE {
                panic!("timed out waiting for {what}; the screen shows:\n{screen}");
            }
            thread::sleep(Duration::from_millis(25));
        }
    }

    /// Send keys and give the program time to read them as separate events.
    fn keys(&mut self, bytes: &str) {
        self.writer.write_all(bytes.as_bytes()).unwrap();
        self.writer.flush().unwrap();
        thread::sleep(Duration::from_millis(120));
    }

    /// Replace the search with `text` and move to the results.
    fn search(&mut self, text: &str) {
        self.keys("/");
        self.keys(ESC);
        self.keys("/");
        self.keys(text);
        self.keys("\r");
    }

    fn resize(&mut self, rows: u16, cols: u16) {
        self.screen
            .lock()
            .unwrap()
            .screen_mut()
            .set_size(rows, cols);
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
    }

    fn wait_for_exit(&mut self) -> u32 {
        let start = Instant::now();
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status.exit_code();
            }
            if start.elapsed() > PATIENCE {
                let _ = self.child.kill();
                panic!(
                    "helixforge did not exit; the screen shows:\n{}",
                    self.contents()
                );
            }
            thread::sleep(Duration::from_millis(25));
        }
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        if let Ok(None) = self.child.try_wait() {
            let _ = self.child.kill();
        }
        // A failed test can leave a fixture's child behind, above all one that
        // ignores the hangup; stop those by the process ids they recorded.
        for suffix in ["child", "stubborn"] {
            if let Ok(pid) = fs::read_to_string(self.state.join(format!("marker.txt.{suffix}"))) {
                let _ = Command::new("kill").args(["-KILL", pid.trim()]).status();
            }
        }
    }
}

const ESC: &str = "\x1b";
const CTRL_C: &str = "\x03";
const BACK_TAB: &str = "\x1b[Z";

fn running(pid: &str) -> bool {
    fs::read_to_string(format!("/proc/{pid}/stat"))
        .map(|stat| {
            let state = stat.rsplit(')').next().unwrap_or("").trim_start();
            !state.starts_with('Z')
        })
        .unwrap_or(false)
}

#[test]
fn the_menu_runs_fixture_entries_from_the_keyboard() {
    let home = tempfile::tempdir().unwrap();
    let catalog = fixtures();
    let mut tui = Terminal::start(
        &[
            "--catalog-dir",
            catalog.to_str().unwrap(),
            "tui",
            "--bypass-root",
        ],
        home.path(),
        40,
        120,
    );
    tui.wait_for(&[
        "SYSTEM READY",
        "Dashboard",
        "Install Plan",
        "Tools",
        "History",
    ]);

    // Help opens over the screen and closes with Escape.
    tui.keys("?");
    tui.wait_for(&["Keys", "Running window"]);
    tui.keys(ESC);
    tui.wait_until("the help to close", |screen| {
        !screen.contains("Running window")
    });

    // Search finds one entry; Space adds it to the install plan.
    tui.keys("/");
    tui.keys("mark");
    tui.wait_for(&["Marker"]);
    tui.wait_until("the search to narrow the list", |screen| {
        !screen.contains("Sleeper") && !screen.contains("Failing")
    });
    tui.keys("\r");
    tui.keys(" ");
    tui.wait_for(&["[*]  Marker"]);
    // Escape in the search box clears it and the whole list returns.
    tui.keys("/");
    tui.keys(ESC);
    tui.wait_for(&["Sleeper", "Failing"]);

    // The install plan, one place up the sidebar, lists it; declining the
    // confirmation runs nothing.
    tui.keys(BACK_TAB);
    tui.wait_for(&["> Install Plan", "Marker"]);
    tui.keys("\r");
    tui.wait_for(&["CONFIRM", "[Y] Run", "Marker"]);
    tui.keys("n");
    tui.wait_until("the confirmation to close", |screen| {
        !screen.contains("CONFIRM")
    });
    let marker = home.path().join("marker.txt");
    assert!(!marker.exists(), "declining must not run the entry");

    // Accepting runs it in the running window.
    tui.keys("\r");
    tui.wait_for(&["CONFIRM"]);
    tui.keys("y");
    tui.wait_for(&["FINISHED"]);
    assert!(
        fs::read_to_string(&marker)
            .unwrap()
            .starts_with("marker from ")
    );
    tui.keys("\r");
    tui.wait_until("the running window to close", |screen| {
        !screen.contains("FINISHED")
    });

    // Typing reaches an entry that asks a question.
    tui.search("greet");
    tui.wait_for(&["Greeter"]);
    tui.keys("\r");
    tui.wait_for(&["CONFIRM"]);
    tui.keys("y");
    tui.wait_for(&["Your name?"]);
    tui.keys("Ada\r");
    tui.wait_for(&["Hello, Ada.", "FINISHED"]);
    tui.keys("\r");

    // Ctrl-C stops a running entry and its child, and the run is interrupted.
    tui.search("sleep");
    tui.wait_for(&["Sleeper"]);
    tui.keys("\r");
    tui.wait_for(&["CONFIRM"]);
    tui.keys("y");
    tui.wait_for(&["sleeper started", "RUNNING", "Ctrl-C stops the command"]);
    let pid_file = home.path().join("marker.txt.child");
    let start = Instant::now();
    while !pid_file.exists() && start.elapsed() < PATIENCE {
        thread::sleep(Duration::from_millis(25));
    }
    let child = fs::read_to_string(&pid_file).unwrap().trim().to_string();
    assert!(running(&child));

    // A resize while the window is open redraws it at the new size, and a
    // Ctrl-C right after a resize is not held back.
    tui.resize(30, 100);
    tui.wait_for(&["RUNNING", "sleeper started"]);
    tui.resize(40, 120);

    tui.keys(CTRL_C);
    tui.wait_for(&["INTERRUPTED"]);
    let start = Instant::now();
    while running(&child) && start.elapsed() < PATIENCE {
        thread::sleep(Duration::from_millis(25));
    }
    assert!(!running(&child), "the sleeping child {child} kept running");
    assert!(!tui.contents().contains("sleeper finished"));
    tui.keys("\r");

    // A failing entry shows its status.
    tui.search("fail");
    tui.wait_for(&["Failing"]);
    tui.keys("\r");
    tui.wait_for(&["CONFIRM"]);
    tui.keys("y");
    tui.wait_for(&["about to fail", "FAILED (exit 9)"]);
    tui.keys("\r");
    tui.keys(ESC);

    // The history screen lists the four runs, newest first.
    tui.keys("\t");
    tui.wait_for(&["> History"]);
    tui.wait_until("the history screen", |screen| {
        screen.contains("exit 9") && screen.contains("interrupted")
    });
    let screen = tui.contents();
    for name in ["Marker", "Greeter", "Sleeper", "Failing"] {
        assert!(screen.contains(name), "history lacks {name}:\n{screen}");
    }
    let history = fs::read_to_string(home.path().join("state/helixforge/history.jsonl")).unwrap();
    let results: Vec<&str> = history
        .lines()
        .map(|line| {
            line.split("\"result\":\"")
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap()
        })
        .collect();
    assert_eq!(results, ["exit 0", "exit 0", "interrupted", "exit 9"]);

    // Too small a terminal shows a warning, and the screen comes back.
    tui.resize(20, 60);
    tui.wait_for(&["The terminal is 60 by 20"]);
    tui.resize(40, 120);
    tui.wait_for(&["SYSTEM READY", "History"]);

    tui.keys("q");
    assert_eq!(tui.wait_for_exit(), 0);
}

#[test]
fn ctrl_c_in_the_confirmation_leaves_without_running() {
    let home = tempfile::tempdir().unwrap();
    let catalog = fixtures();
    let mut tui = Terminal::start(
        &[
            "--catalog-dir",
            catalog.to_str().unwrap(),
            "tui",
            "--bypass-root",
        ],
        home.path(),
        40,
        120,
    );
    tui.wait_for(&["SYSTEM READY"]);
    tui.keys("/");
    tui.keys("mark");
    tui.keys("\r");
    tui.keys("\r");
    tui.wait_for(&["CONFIRM"]);
    tui.keys(CTRL_C);
    assert_eq!(tui.wait_for_exit(), 0);
    assert!(!home.path().join("marker.txt").exists());
    assert!(!home.path().join("state/helixforge/history.jsonl").exists());
}

#[test]
fn a_killed_menu_leaves_a_copy_that_the_next_start_removes() {
    let home = tempfile::tempdir().unwrap();
    let tmp = home.path().join("tmp");
    let mut tui = Terminal::start(&["tui", "--bypass-root"], home.path(), 40, 120);
    tui.wait_for(&["SYSTEM READY"]);
    let copies = || {
        fs::read_dir(&tmp)
            .unwrap()
            .flatten()
            .filter(|item| {
                item.file_name()
                    .to_string_lossy()
                    .starts_with("helixforge-")
            })
            .count()
    };
    assert_eq!(copies(), 1);
    // SIGKILL leaves no chance to clean up, like a closed terminal often does.
    let pid = tui.child.process_id().unwrap();
    Command::new("kill")
        .args(["-KILL", &pid.to_string()])
        .status()
        .unwrap();
    tui.wait_for_exit();
    assert_eq!(copies(), 1);
    let listed = Command::new(PROGRAM)
        .arg("list")
        .env("TMPDIR", &tmp)
        .env("XDG_STATE_HOME", home.path().join("state"))
        .output()
        .unwrap();
    assert!(listed.status.success());
    assert_eq!(copies(), 0, "the stale copy was not removed");

    // A clean quit removes its own copy.
    let mut tui = Terminal::start(&["tui", "--bypass-root"], home.path(), 40, 120);
    tui.wait_for(&["SYSTEM READY"]);
    assert_eq!(copies(), 1);
    tui.keys("q");
    assert_eq!(tui.wait_for_exit(), 0);
    assert_eq!(copies(), 0);
}

#[test]
fn the_menu_starts_when_standard_input_is_a_pipe_as_with_curl_into_sh() {
    let home = tempfile::tempdir().unwrap();
    let catalog = fixtures();
    let mut command = CommandBuilder::new("sh");
    command.args([
        "-c",
        "echo script | \"$0\" --catalog-dir \"$1\" tui --bypass-root",
        PROGRAM,
        catalog.to_str().unwrap(),
    ]);
    let mut tui = Terminal::spawn(command, home.path(), 40, 120);
    tui.wait_for(&["SYSTEM READY"]);
    tui.keys("?");
    tui.wait_for(&[" Keys "]);
    tui.keys(ESC);
    tui.keys("q");
    assert_eq!(tui.wait_for_exit(), 0);
}

#[test]
fn the_menu_refuses_to_start_without_a_terminal() {
    // setsid leaves the program without a controlling terminal, as under cron.
    let output = Command::new("setsid")
        .args(["--wait", PROGRAM])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("the menu needs a terminal"), "{error}");
    assert!(!error.contains("panicked"), "{error}");
}

#[test]
fn the_command_line_runs_fixtures_and_says_when_history_cannot_be_written() {
    let home = tempfile::tempdir().unwrap();
    let catalog = fixtures();
    let helixforge = |state: &Path, args: &[&str]| {
        Command::new(PROGRAM)
            .arg("--catalog-dir")
            .arg(&catalog)
            .args(args)
            .env("XDG_STATE_HOME", state)
            .env("HELIXFORGE_TEST_OUT", home.path().join("marker.txt"))
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
    };
    let state = home.path().join("state");

    let listed = helixforge(&state, &["list"]);
    let text = String::from_utf8_lossy(&listed.stdout);
    for id in [
        "bundle", "failing", "greeter", "marker", "report", "sleeper",
    ] {
        assert!(text.contains(id), "list lacks {id}:\n{text}");
    }

    let planned = helixforge(&state, &["run", "bundle", "--dry-run"]);
    let text = String::from_utf8_lossy(&planned.stdout);
    assert!(
        text.contains("1. Marker") && text.contains("2. Report"),
        "{text}"
    );
    assert!(text.contains("Dry run: nothing was changed."));
    assert!(!home.path().join("marker.txt").exists());

    let refused = helixforge(&state, &["run", "marker"]);
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("confirmation needs a terminal"));
    assert!(!home.path().join("marker.txt").exists());

    let ran = helixforge(&state, &["run", "marker", "--yes"]);
    assert!(ran.status.success());
    assert!(home.path().join("marker.txt").exists());
    let failed = helixforge(&state, &["run", "failing", "--yes"]);
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("exit 9"));
    let history = helixforge(&state, &["history"]);
    let text = String::from_utf8_lossy(&history.stdout);
    assert!(text.contains("failed | Failing | exit 9"), "{text}");
    assert!(text.contains("ok     | Marker | exit 0"), "{text}");

    // A state directory that cannot be created: the run still happens, and
    // the user is told it was not recorded.
    let blocked = home.path().join("blocked");
    fs::write(&blocked, "not a directory").unwrap();
    let ran = helixforge(&blocked, &["run", "marker", "--yes"]);
    assert!(ran.status.success());
    let error = String::from_utf8_lossy(&ran.stderr);
    assert!(error.contains("could not record this run"), "{error}");
}

#[test]
fn a_key_pressed_right_after_a_resize_is_read_at_once() {
    let home = tempfile::tempdir().unwrap();
    let catalog = fixtures();
    let mut tui = Terminal::start(
        &[
            "--catalog-dir",
            catalog.to_str().unwrap(),
            "tui",
            "--bypass-root",
        ],
        home.path(),
        40,
        120,
    );
    tui.wait_for(&["SYSTEM READY"]);
    for round in 0..8 {
        tui.resize(if round % 2 == 0 { 30 } else { 40 }, 120);
        // No pause: the key and the resize reach the program together. Nothing
        // follows the key, so a key left unread would never show.
        tui.writer.write_all(b"?").unwrap();
        tui.writer.flush().unwrap();
        tui.wait_for(&[" Keys "]);
        tui.keys(ESC);
        tui.wait_until("the help to close", |screen| !screen.contains(" Keys "));
    }
    tui.keys("q");
    assert_eq!(tui.wait_for_exit(), 0);
}

#[test]
fn the_menu_says_when_a_run_cannot_be_recorded() {
    let home = tempfile::tempdir().unwrap();
    // The state directory cannot be created where a plain file is in the way.
    fs::write(home.path().join("state"), "not a directory").unwrap();
    let catalog = fixtures();
    let mut tui = Terminal::start(
        &[
            "--catalog-dir",
            catalog.to_str().unwrap(),
            "tui",
            "--bypass-root",
        ],
        home.path(),
        40,
        120,
    );
    tui.wait_for(&["SYSTEM READY"]);
    tui.search("mark");
    tui.keys("\r");
    tui.wait_for(&["CONFIRM"]);
    tui.keys("y");
    tui.wait_for(&["FINISHED"]);
    assert!(home.path().join("marker.txt").exists());
    tui.keys("l");
    tui.wait_for(&["The log was not saved"]);
    tui.keys("\r");
    tui.wait_for(&["Not recorded", "The run was not recorded in the history"]);
    tui.keys("\r");
    tui.wait_until("the message to close", |screen| {
        !screen.contains("Not recorded")
    });
    tui.keys("q");
    assert_eq!(tui.wait_for_exit(), 0);
}

#[test]
fn a_second_ctrl_c_kills_what_ignored_the_first() {
    let home = tempfile::tempdir().unwrap();
    let catalog = fixtures();
    let mut tui = Terminal::start(
        &[
            "--catalog-dir",
            catalog.to_str().unwrap(),
            "tui",
            "--bypass-root",
        ],
        home.path(),
        40,
        120,
    );
    tui.wait_for(&["SYSTEM READY"]);
    tui.search("stubborn");
    tui.keys("\r");
    tui.wait_for(&["CONFIRM"]);
    tui.keys("y");
    tui.wait_for(&["stubborn started", "RUNNING"]);
    let pid_file = home.path().join("marker.txt.stubborn");
    let start = Instant::now();
    while fs::read_to_string(&pid_file).map_or(true, |text| text.trim().is_empty())
        && start.elapsed() < PATIENCE
    {
        thread::sleep(Duration::from_millis(25));
    }
    let child = fs::read_to_string(&pid_file).unwrap().trim().to_string();

    tui.keys(CTRL_C);
    // The script and its sleeping child both ignored the hangup.
    tui.wait_for(&["INTERRUPTED", "2 process(es) of this run still running"]);
    assert!(running(&child));
    tui.keys(CTRL_C);
    let start = Instant::now();
    while running(&child) && start.elapsed() < PATIENCE {
        thread::sleep(Duration::from_millis(25));
    }
    assert!(!running(&child), "the second Ctrl-C did not stop {child}");
    // Any key redraws; the warning is gone and the window can close.
    tui.keys("l");
    tui.wait_until("the warning to clear", |screen| {
        !screen.contains("still running")
    });
    tui.keys("\r");
    tui.keys("q");
    assert_eq!(tui.wait_for_exit(), 0);
    let history = fs::read_to_string(home.path().join("state/helixforge/history.jsonl")).unwrap();
    assert!(history.contains("\"result\":\"interrupted\""), "{history}");
}
