//! Drive the real `helixforge` program in a pseudo-terminal against the
//! fixture catalog: keyboard navigation, search, the install plan, the
//! confirmation, typing into a running entry, stopping one with Ctrl-C, the
//! history screen and terminal resizing; and package entries falling back and
//! being verified, against stand-in package managers. Nothing outside
//! temporary directories is touched.

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
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
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

    fn spawn(command: CommandBuilder, state: &Path, rows: u16, cols: u16) -> Self {
        Self::spawn_with(command, state, rows, cols, false)
    }

    /// Start a program; with `graphics`, answer its graphics query as a
    /// terminal with Kitty's graphics does, cells 9 by 19 pixels.
    fn spawn_with(
        mut command: CommandBuilder,
        state: &Path,
        rows: u16,
        cols: u16,
        graphics: bool,
    ) -> Self {
        let pty = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        command.env("HOME", state);
        // The interface's remembered look lives here, away from the user's.
        command.env("XDG_CONFIG_HOME", state.join("config"));
        command.env("XDG_STATE_HOME", state.join("state"));
        command.env("TMPDIR", state.join("tmp"));
        command.env("HELIXFORGE_TEST_OUT", state.join("marker.txt"));
        command.env("TERM", "xterm-256color");
        fs::create_dir_all(state.join("tmp")).unwrap();
        let child = pty.slave.spawn_command(command).unwrap();
        drop(pty.slave);
        let screen = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 0)));
        let mut reader = pty.master.try_clone_reader().unwrap();
        let writer = Arc::new(Mutex::new(pty.master.take_writer().unwrap()));
        let parser = Arc::clone(&screen);
        let answers = Arc::clone(&writer);
        thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            while let Ok(count) = reader.read(&mut buffer) {
                if count == 0 {
                    break;
                }
                let output = &buffer[..count];
                if graphics && output.windows(8).any(|part| part == b"\x1b_Gi=31,") {
                    let mut answers = answers.lock().unwrap();
                    answers
                        .write_all(b"\x1b_Gi=31;OK\x1b\\\x1b[6;19;9t\x1b[?62;22c")
                        .unwrap();
                    answers.flush().unwrap();
                }
                parser.lock().unwrap().process(output);
            }
        });
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
        let mut writer = self.writer.lock().unwrap();
        writer.write_all(bytes.as_bytes()).unwrap();
        writer.flush().unwrap();
        drop(writer);
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
                let _ = Command::new("kill")
                    .args(["-KILL", pid.trim()])
                    .stderr(std::process::Stdio::null())
                    .status();
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
    // The check-circle icon marks a queued entry.
    tui.wait_for(&["\u{f058}  Marker"]);
    // Escape in the search box clears it and the whole list returns.
    tui.keys("/");
    tui.keys(ESC);
    tui.wait_for(&["Sleeper", "Failing"]);

    // The install plan, one place up the sidebar, lists it; declining the
    // confirmation runs nothing.
    tui.keys(BACK_TAB);
    tui.wait_for(&["Review planned changes", "Marker"]);
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
    tui.wait_for(&["Past runs, newest first"]);
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
fn the_banner_is_drawn_where_the_terminal_shows_pictures() {
    let home = tempfile::tempdir().unwrap();
    let mut command = CommandBuilder::new(PROGRAM);
    command.args([
        "--catalog-dir",
        fixtures().to_str().unwrap(),
        "tui",
        "--bypass-root",
    ]);
    let mut tui = Terminal::spawn_with(command, home.path(), 40, 120, true);
    tui.wait_for(&["SYSTEM READY", "MAIN MENU"]);
    let screen = tui.contents();
    assert!(screen.contains('\u{10EEEE}'), "{screen}");
    assert!(!screen.contains("BUILD • CONFIGURE"));
    // The answers were all read, so the first key reaches the menu.
    tui.keys("?");
    tui.wait_for(&[" Keys "]);
    tui.keys(ESC);
    tui.keys("q");
    assert_eq!(tui.wait_for_exit(), 0);
}

#[test]
fn a_terminal_without_pictures_keeps_the_text_header() {
    let home = tempfile::tempdir().unwrap();
    let mut tui = Terminal::start(
        &[
            "--catalog-dir",
            fixtures().to_str().unwrap(),
            "tui",
            "--bypass-root",
        ],
        home.path(),
        40,
        120,
    );
    tui.wait_for(&["HELIXFORGE", "BUILD • CONFIGURE • OPTIMIZE"]);
    assert!(!tui.contents().contains('\u{10EEEE}'));
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

    // Without a terminal at all (setsid drops the controlling one), there is
    // nowhere to ask, so the run is refused.
    let refused = Command::new("setsid")
        .arg("--wait")
        .arg(PROGRAM)
        .arg("--catalog-dir")
        .arg(&catalog)
        .args(["run", "marker"])
        .env("XDG_STATE_HOME", &state)
        .env("HELIXFORGE_TEST_OUT", home.path().join("marker.txt"))
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
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
        tui.writer.lock().unwrap().write_all(b"?").unwrap();
        tui.writer.lock().unwrap().flush().unwrap();
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

fn history_of(home: &Path) -> String {
    let output = Command::new(PROGRAM)
        .arg("--catalog-dir")
        .arg(fixtures())
        .arg("history")
        .env("XDG_STATE_HOME", home.join("state"))
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn wait_for_file(path: &Path) -> String {
    let start = Instant::now();
    loop {
        if let Ok(text) = fs::read_to_string(path)
            && !text.trim().is_empty()
        {
            return text.trim().to_string();
        }
        assert!(
            start.elapsed() < PATIENCE,
            "{} never appeared",
            path.display()
        );
        thread::sleep(Duration::from_millis(25));
    }
}

fn kill_hard(pid: u32) {
    Command::new("kill")
        .args(["-KILL", &pid.to_string()])
        .status()
        .unwrap();
}

fn fixture_menu(home: &Path) -> Terminal {
    let catalog = fixtures();
    let tui = Terminal::start(
        &[
            "--catalog-dir",
            catalog.to_str().unwrap(),
            "tui",
            "--bypass-root",
        ],
        home,
        40,
        120,
    );
    tui.wait_for(&["SYSTEM READY"]);
    tui
}

#[test]
fn a_menu_killed_during_a_run_leaves_it_recorded_as_unfinished() {
    let home = tempfile::tempdir().unwrap();
    let mut tui = fixture_menu(home.path());
    tui.search("sleep");
    tui.keys("\r");
    tui.wait_for(&["CONFIRM"]);
    tui.keys("y");
    tui.wait_for(&["sleeper started"]);
    wait_for_file(&home.path().join("marker.txt.child"));
    // Nothing is recorded yet: the run is in progress.
    assert!(history_of(home.path()).contains("No runs recorded yet."));
    kill_hard(tui.child.process_id().unwrap());
    tui.wait_for_exit();
    let history = history_of(home.path());
    assert!(
        history.contains("failed | Sleeper | did not finish: HelixForge stopped during the run"),
        "{history}"
    );
    // Recorded once, and the marker is gone.
    assert_eq!(history_of(home.path()).lines().count(), 1);
    assert_eq!(
        fs::read_dir(home.path().join("state/helixforge/running"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn a_menu_killed_before_closing_a_finished_run_records_its_real_outcome() {
    let home = tempfile::tempdir().unwrap();
    let mut tui = fixture_menu(home.path());
    tui.search("mark");
    tui.keys("\r");
    tui.wait_for(&["CONFIRM"]);
    tui.keys("y");
    tui.wait_for(&["FINISHED"]);
    kill_hard(tui.child.process_id().unwrap());
    tui.wait_for_exit();
    let history = history_of(home.path());
    assert!(history.contains("ok     | Marker | exit 0"), "{history}");
}

#[test]
fn the_command_line_killed_during_a_run_leaves_it_recorded_as_unfinished() {
    let home = tempfile::tempdir().unwrap();
    let mut run = Command::new(PROGRAM)
        .arg("--catalog-dir")
        .arg(fixtures())
        .args(["run", "sleeper", "--yes"])
        .env("XDG_STATE_HOME", home.path().join("state"))
        .env("HELIXFORGE_TEST_OUT", home.path().join("marker.txt"))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let child = wait_for_file(&home.path().join("marker.txt.child"));
    kill_hard(run.id());
    run.wait().unwrap();
    // Without a terminal, nothing else stops the sleeping child.
    Command::new("kill")
        .args(["-KILL", &child])
        .status()
        .unwrap();
    let history = history_of(home.path());
    assert!(
        history.contains("failed | Sleeper | did not finish: HelixForge stopped during the run"),
        "{history}"
    );
}

#[test]
fn the_command_line_asks_on_the_terminal_when_input_is_a_pipe() {
    let home = tempfile::tempdir().unwrap();
    let catalog = fixtures();
    let mut command = CommandBuilder::new("sh");
    command.args([
        "-c",
        "echo not-an-answer | \"$0\" --catalog-dir \"$1\" run marker",
        PROGRAM,
        catalog.to_str().unwrap(),
    ]);
    let mut cli = Terminal::spawn(command, home.path(), 40, 120);
    cli.wait_for(&["1. Marker", "Type RUN to continue:"]);
    assert!(!home.path().join("marker.txt").exists());
    cli.keys("RUN\r");
    assert_eq!(cli.wait_for_exit(), 0);
    assert!(home.path().join("marker.txt").exists());

    // A wrong word cancels.
    fs::remove_file(home.path().join("marker.txt")).unwrap();
    let mut command = CommandBuilder::new("sh");
    command.args([
        "-c",
        "echo RUN | \"$0\" --catalog-dir \"$1\" run marker",
        PROGRAM,
        catalog.to_str().unwrap(),
    ]);
    let mut cli = Terminal::spawn(command, home.path(), 40, 120);
    cli.wait_for(&["Type RUN to continue:"]);
    cli.keys("no\r");
    assert_ne!(cli.wait_for_exit(), 0);
    assert!(!home.path().join("marker.txt").exists());
}

#[test]
fn the_theme_and_icon_choices_are_remembered() {
    let home = tempfile::tempdir().unwrap();
    let mut tui = fixture_menu(home.path());
    tui.wait_for(&["\u{f0e4}"]);
    // Settings is the last item in the sidebar.
    tui.keys(BACK_TAB);
    tui.wait_for(&["theme and icons are remembered", "default", "Nerd Font"]);
    tui.keys("\r");
    tui.keys("\r");
    tui.wait_for(&["solid"]);
    tui.keys("j");
    tui.keys("\r");
    tui.wait_for(&["plain"]);
    tui.keys("q");
    assert_eq!(tui.wait_for_exit(), 0);
    let saved = fs::read_to_string(home.path().join("config/helixforge/settings.toml")).unwrap();
    assert!(saved.contains("theme = \"solid\""), "{saved}");
    assert!(saved.contains("icons = false"), "{saved}");

    let mut tui = fixture_menu(home.path());
    tui.keys(BACK_TAB);
    tui.wait_for(&["solid", "plain"]);
    assert!(!tui.contents().contains("\u{f0e4}"));
    tui.keys("q");
    assert_eq!(tui.wait_for_exit(), 0);
}

fn package_fixtures() -> PathBuf {
    fixtures().join("packages")
}

/// Stand-ins for sudo, every package manager and flatpak, written to `bin`;
/// returns a PATH that finds them first. The stand-in sudo runs only the
/// stand-ins beside it, so nothing real is reached. Package managers find no
/// package; flatpak installs into `$HOME/.local/share/flatpak`, except for
/// the Missing sample, which it only pretends to install.
fn stand_in_tools(bin: &Path) -> String {
    fs::create_dir_all(bin).unwrap();
    let mut tools = vec![
        (
            "sudo",
            "#!/bin/sh\ntool=$1\nshift\nexec \"$(dirname \"$0\")/$tool\" \"$@\"\n".to_string(),
        ),
        (
            "flatpak",
            "#!/bin/sh\ncase $1 in\n  install) case $3 in *Missing*) ;; *) mkdir -p \"$HOME/.local/share/flatpak/app/$3\" ;; esac ;;\n  uninstall) rm -r \"$HOME/.local/share/flatpak/app/$2\" ;;\nesac\n"
                .to_string(),
        ),
    ];
    for manager in [
        "pacman",
        "apt-get",
        "dnf",
        "zypper",
        "apk",
        "xbps-install",
        "xbps-query",
        "eopkg",
        "rpm",
        "dpkg-query",
    ] {
        tools.push((
            manager,
            "#!/bin/sh\necho \"stand-in: no such package\" >&2\nexit 1\n".to_string(),
        ));
    }
    for (name, text) in tools {
        let path = bin.join(name);
        fs::write(&path, text).unwrap();
        fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    }
    format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    )
}

#[test]
fn package_entries_fall_back_and_are_verified_from_the_command_line() {
    let home = tempfile::tempdir().unwrap();
    let path = stand_in_tools(&home.path().join("bin"));
    let helixforge = |args: &[&str]| {
        let output = Command::new(PROGRAM)
            .arg("--catalog-dir")
            .arg(package_fixtures())
            .args(args)
            .env("PATH", &path)
            .env("HOME", home.path())
            .env("XDG_STATE_HOME", home.path().join("state"))
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        (
            output.status.success(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    };

    // The distribution's package fails, so Flathub is tried and checked.
    let (ok, out, err) = helixforge(&["run", "fallback-sample", "--yes"]);
    assert!(ok, "{out}\n{err}");
    assert!(out.contains("failed (exit 1)"), "{out}");
    assert!(out.contains("HelixForge: trying Flathub"), "{out}");
    assert!(
        out.contains(
            "Verified: Fallback Sample: org.helixforge.FallbackSample is installed from Flathub"
        ),
        "{out}"
    );
    let (_, out, _) = helixforge(&["plan", "fallback-sample"]);
    assert!(out.contains("already installed"), "{out}");

    // Removal goes the way it was installed and checks it is gone.
    let (ok, out, err) = helixforge(&["remove", "fallback-sample", "--yes"]);
    assert!(ok, "{out}\n{err}");
    assert!(
        out.contains("Verified: Remove Fallback Sample: removed"),
        "{out}"
    );
    let (_, out, _) = helixforge(&["remove", "fallback-sample", "--dry-run"]);
    assert!(out.contains("not installed"), "{out}");

    // A source that says it worked without installing anything is not trusted.
    let (ok, out, err) = helixforge(&["run", "missing-sample", "--yes"]);
    assert!(!ok);
    assert!(
        out.contains("HelixForge: Flathub finished, but Missing Sample is not installed"),
        "{out}"
    );
    assert!(err.contains("exit 1"), "{err}");

    let (_, history, _) = helixforge(&["history"]);
    assert!(
        history.contains("ok     | Fallback Sample | exit 0, verified"),
        "{history}"
    );
    assert!(
        history.contains("ok     | Remove Fallback Sample | exit 0, verified"),
        "{history}"
    );
    assert!(
        history.contains("failed | Missing Sample | exit 1"),
        "{history}"
    );
}

#[test]
fn the_running_window_says_whether_a_package_was_verified() {
    let home = tempfile::tempdir().unwrap();
    let path = stand_in_tools(&home.path().join("bin"));
    let mut command = CommandBuilder::new(PROGRAM);
    command.args([
        "--catalog-dir",
        package_fixtures().to_str().unwrap(),
        "tui",
        "--bypass-root",
    ]);
    command.env("PATH", &path);
    let mut tui = Terminal::spawn(command, home.path(), 40, 120);
    tui.wait_for(&["SYSTEM READY"]);

    tui.search("fallback");
    tui.wait_for(&["Fallback Sample"]);
    tui.keys(" ");
    tui.keys(BACK_TAB);
    tui.wait_for(&["Review planned changes", "Fallback Sample"]);
    tui.keys("\r");
    tui.wait_for(&["CONFIRM", "Fallback Sample"]);
    tui.keys("y");
    tui.wait_for(&["FINISHED · VERIFIED", "HelixForge: trying Flathub"]);
    tui.keys("\r");
    tui.wait_until("the running window to close", |screen| {
        !screen.contains("FINISHED")
    });
    let history = fs::read_to_string(home.path().join("state/helixforge/history.jsonl")).unwrap();
    assert!(
        history.contains("\"result\":\"exit 0, verified\""),
        "{history}"
    );
    tui.keys("q");
    assert_eq!(tui.wait_for_exit(), 0);
}
