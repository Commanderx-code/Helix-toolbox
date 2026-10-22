//! Drive the real `helix-toolbox` program in a pseudo-terminal against the
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

const PROGRAM: &str = env!("CARGO_BIN_EXE_helix-toolbox");
const PATIENCE: Duration = Duration::from_secs(20);

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

/// A running `helix-toolbox` in a pseudo-terminal, with a parsed screen.
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
        command.env("HELIX_TOOLBOX_TEST_OUT", state.join("marker.txt"));
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
                    "helix-toolbox did not exit; the screen shows:\n{}",
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
fn pinned_entries_are_kept_and_listed_under_favorites() {
    let home = tempfile::tempdir().unwrap();
    let catalog = fixtures();
    let args = [
        "--catalog-dir",
        catalog.to_str().unwrap(),
        "tui",
        "--bypass-root",
    ];
    let mut tui = Terminal::start(&args, home.path(), 40, 120);
    tui.wait_for(&["SYSTEM READY", "Favorites"]);
    tui.keys("/");
    tui.keys("mark");
    tui.wait_for(&["Marker"]);
    tui.keys("\r");
    tui.keys("*");
    tui.wait_for(&["Marker ★"]);
    let saved = home.path().join("config/helix-toolbox/favorites.toml");
    tui.wait_until("the favorites to be saved", |_| {
        fs::read_to_string(&saved).is_ok_and(|text| text.contains("\"marker\""))
    });
    tui.keys("q");
    tui.wait_for_exit();

    // The next start lists it under Favorites, two places up the sidebar.
    let mut tui = Terminal::start(&args, home.path(), 40, 120);
    tui.wait_for(&["SYSTEM READY"]);
    tui.keys("/");
    tui.keys(ESC);
    tui.keys(BACK_TAB);
    tui.keys(BACK_TAB);
    tui.wait_for(&["Entries pinned with *", "Marker ★"]);
    tui.keys("q");
    tui.wait_for_exit();
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

    // Drift comes next. Ctrl-K's command palette opens Updates by name,
    // and the history screen after it lists the four runs, newest first.
    tui.keys("\t");
    tui.wait_for(&["How this machine differs", "No baseline saved yet"]);
    tui.keys("\x0b");
    tui.wait_for(&["COMMANDS", "Type a command"]);
    tui.keys("open updates");
    tui.wait_for(&["Open Updates"]);
    tui.keys("\r");
    tui.wait_for(&["What each tool has waiting"]);
    tui.keys("\t");
    tui.wait_for(&["Past runs, newest first"]);
    tui.wait_until("the history screen", |screen| {
        screen.contains("exit 9") && screen.contains("interrupted")
    });
    let screen = tui.contents();
    for name in ["Marker", "Greeter", "Sleeper", "Failing"] {
        assert!(screen.contains(name), "history lacks {name}:\n{screen}");
    }
    let history =
        fs::read_to_string(home.path().join("state/helix-toolbox/history.jsonl")).unwrap();
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
    assert!(
        !home
            .path()
            .join("state/helix-toolbox/history.jsonl")
            .exists()
    );
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
                    .starts_with("helix-toolbox-")
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

    // A clean quit removes its own copy. The first start offers the setup
    // wizard (the killed one never got to remember it); Escape declines.
    let mut tui = Terminal::start(&["tui", "--bypass-root"], home.path(), 40, 120);
    tui.wait_for(&[
        "SYSTEM READY",
        "SET UP THIS MACHINE",
        "Welcome to Helix Toolbox",
    ]);
    assert_eq!(copies(), 1);
    tui.keys(ESC);
    tui.wait_until("the wizard to close", |screen| {
        !screen.contains("SET UP THIS MACHINE")
    });
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
    // The spaced-out name beside the emblem, not the text header's.
    assert!(screen.contains("H E L I X   T O O L B O X"), "{screen}");
    assert!(!screen.contains("HELIX TOOLBOX"));
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
    tui.wait_for(&["HELIX TOOLBOX", "BUILD • CONFIGURE • OPTIMIZE"]);
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
fn damaged_settings_profiles_and_history_never_stop_it() {
    let home = tempfile::tempdir().unwrap();
    let catalog = fixtures();
    let config = home.path().join("config/helix-toolbox");
    let state = home.path().join("state/helix-toolbox");
    fs::create_dir_all(config.join("profiles")).unwrap();
    fs::create_dir_all(&state).unwrap();
    fs::write(config.join("settings.toml"), b"theme = 42\n\x00\xff[[[").unwrap();
    fs::write(config.join("favorites.toml"), b"\xff\xfe entries = ").unwrap();
    fs::write(config.join("profiles/broken.toml"), "name = [").unwrap();
    fs::write(
        config.join("profiles/unknown.toml"),
        "name = \"Unknown\"\nentries = [\"no-such-entry\"]\n",
    )
    .unwrap();
    fs::write(
        state.join("history.jsonl"),
        "not json\n{\"unix_timestamp\":1,\"entries\":[\"marker\"],\"names\":[\"Marker\"],\"success\":true,\"result\":\"exit 0\",\"log\":null}\n{\"unix_timestamp\":",
    )
    .unwrap();
    let toolbox = |args: &[&str]| {
        Command::new(PROGRAM)
            .arg("--catalog-dir")
            .arg(&catalog)
            .args(args)
            .env("HOME", home.path())
            .env("XDG_CONFIG_HOME", home.path().join("config"))
            .env("XDG_STATE_HOME", home.path().join("state"))
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
    };

    // The readable record is kept; the rest is skipped.
    let history = toolbox(&["history"]);
    assert!(history.status.success(), "{history:?}");
    let text = String::from_utf8_lossy(&history.stdout);
    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(text.contains("Marker"), "{text}");

    // Broken profiles are named and skipped.
    let profiles = toolbox(&["profile", "list"]);
    assert!(profiles.status.success(), "{profiles:?}");
    let problems = String::from_utf8_lossy(&profiles.stderr);
    assert!(problems.contains("broken"), "{problems}");
    assert!(problems.contains("no-such-entry"), "{problems}");

    // The menu opens with the default settings, and a pin replaces the
    // unreadable favorites.
    let args = [
        "--catalog-dir",
        catalog.to_str().unwrap(),
        "tui",
        "--bypass-root",
    ];
    let mut tui = Terminal::start(&args, home.path(), 40, 120);
    tui.wait_for(&["SYSTEM READY", "Favorites"]);
    tui.keys("/");
    tui.keys("mark");
    tui.wait_for(&["Marker"]);
    tui.keys("\r");
    tui.keys("*");
    tui.wait_for(&["Marker ★"]);
    tui.keys("q");
    assert_eq!(tui.wait_for_exit(), 0);
    let favorites = fs::read_to_string(config.join("favorites.toml")).unwrap();
    assert!(favorites.contains("\"marker\""), "{favorites}");
}

#[test]
fn completions_and_manual_pages_describe_the_commands() {
    for shell in ["bash", "zsh", "fish"] {
        let output = Command::new(PROGRAM)
            .args(["completions", shell])
            .output()
            .unwrap();
        assert!(output.status.success(), "{shell}");
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(text.contains("helix-toolbox"), "{shell}");
        assert!(text.contains("updates"), "{shell}: {text}");
    }
    let pages = tempfile::tempdir().unwrap();
    let output = Command::new(PROGRAM)
        .arg("manpages")
        .arg(pages.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let main = fs::read_to_string(pages.path().join("helix-toolbox.1")).unwrap();
    assert!(main.contains("helix\\-toolbox\\-updates(1)"), "{main}");
    assert!(pages.path().join("helix-toolbox-run.1").exists());
    assert!(
        !pages.path().join("helix-toolbox-native.1").exists(),
        "hidden commands stay out"
    );
}

#[test]
fn the_command_line_prints_json_for_other_programs() {
    let home = tempfile::tempdir().unwrap();
    let catalog = fixtures();
    let toolbox = |args: &[&str]| {
        Command::new(PROGRAM)
            .arg("--catalog-dir")
            .arg(&catalog)
            .args(args)
            .arg("--json")
            .env("HOME", home.path())
            .env("XDG_CONFIG_HOME", home.path().join("config"))
            .env("XDG_STATE_HOME", home.path().join("state"))
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
    };
    let document = |args: &[&str]| -> serde_json::Value {
        let output = toolbox(args);
        assert!(output.status.success(), "{args:?}: {output:?}");
        let value: serde_json::Value = serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|error| panic!("{args:?} is not JSON: {error}: {output:?}"));
        assert_eq!(value["schema"], 1);
        value
    };

    let listed = document(&["list"]);
    assert_eq!(listed["command"], "list");
    let text = listed["data"]["tabs"].to_string();
    assert!(text.contains("\"id\":\"marker\""), "{text}");

    let found = document(&["search", "mark"]);
    assert_eq!(found["data"]["results"][0]["id"], "marker");
    assert_eq!(found["data"]["results"][0]["kind"], "script");
    let none = document(&["search", "qqqzzz"]);
    assert_eq!(none["data"]["results"], serde_json::json!([]));

    let info = document(&["info", "marker"]);
    assert_eq!(info["data"]["name"], "Marker");

    let planned = document(&["plan", "bundle"]);
    let steps = planned["data"]["steps"].as_array().unwrap();
    assert_eq!(steps[0]["name"], "Marker");
    assert_eq!(steps[1]["name"], "Report");
    assert_eq!(planned["data"]["runnable"], true);
    let dry = document(&["run", "bundle", "--dry-run"]);
    assert_eq!(dry["command"], "plan");
    assert!(
        !home.path().join("marker.txt").exists(),
        "a dry run runs nothing"
    );

    assert_eq!(
        document(&["history"])["data"]["runs"],
        serde_json::json!([])
    );
    assert_eq!(
        document(&["profile", "list"])["data"]["profiles"],
        serde_json::json!([])
    );
    assert!(document(&["system"])["data"]["distribution"]["id"].is_string());
    let report = document(&["report", "--no-checks"]);
    assert_eq!(report["data"]["installed_from"], "source build");
    assert_eq!(report["data"]["health_checks"], serde_json::Value::Null);

    // Changing anything is refused, before anything runs.
    let refused = toolbox(&["run", "marker", "--yes"]);
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("--dry-run"));
    assert!(refused.stdout.is_empty());
    assert!(!toolbox(&["docgen", "walkthrough.md"]).status.success());
}

#[test]
fn the_command_line_runs_fixtures_and_says_when_history_cannot_be_written() {
    let home = tempfile::tempdir().unwrap();
    let catalog = fixtures();
    let toolbox = |state: &Path, args: &[&str]| {
        Command::new(PROGRAM)
            .arg("--catalog-dir")
            .arg(&catalog)
            .args(args)
            .env("XDG_STATE_HOME", state)
            .env("HELIX_TOOLBOX_TEST_OUT", home.path().join("marker.txt"))
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
    };
    let state = home.path().join("state");

    let listed = toolbox(&state, &["list"]);
    let text = String::from_utf8_lossy(&listed.stdout);
    for id in [
        "bundle", "failing", "greeter", "marker", "report", "sleeper",
    ] {
        assert!(text.contains(id), "list lacks {id}:\n{text}");
    }

    let planned = toolbox(&state, &["run", "bundle", "--dry-run"]);
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
        .env("HELIX_TOOLBOX_TEST_OUT", home.path().join("marker.txt"))
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("confirmation needs a terminal"));
    assert!(!home.path().join("marker.txt").exists());

    let ran = toolbox(&state, &["run", "marker", "--yes"]);
    assert!(ran.status.success());
    assert!(home.path().join("marker.txt").exists());
    let failed = toolbox(&state, &["run", "failing", "--yes"]);
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("exit 9"));
    let history = toolbox(&state, &["history"]);
    let text = String::from_utf8_lossy(&history.stdout);
    assert!(text.contains("failed | Failing | exit 9"), "{text}");
    assert!(text.contains("ok     | Marker | exit 0"), "{text}");

    // A state directory that cannot be created: the run still happens, and
    // the user is told it was not recorded.
    let blocked = home.path().join("blocked");
    fs::write(&blocked, "not a directory").unwrap();
    let ran = toolbox(&blocked, &["run", "marker", "--yes"]);
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
    let history =
        fs::read_to_string(home.path().join("state/helix-toolbox/history.jsonl")).unwrap();
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
        history.contains("failed | Sleeper | did not finish: Helix Toolbox stopped during the run"),
        "{history}"
    );
    // Recorded once, and the marker is gone.
    assert_eq!(history_of(home.path()).lines().count(), 1);
    assert_eq!(
        fs::read_dir(home.path().join("state/helix-toolbox/running"))
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
        .env("HELIX_TOOLBOX_TEST_OUT", home.path().join("marker.txt"))
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
        history.contains("failed | Sleeper | did not finish: Helix Toolbox stopped during the run"),
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
    tui.wait_for(&["theme and icons are remembered", "solid", "Nerd Font"]);
    tui.keys("\r");
    tui.keys("\r");
    tui.wait_for(&["transparent"]);
    tui.keys("j");
    tui.keys("\r");
    tui.wait_for(&["plain"]);
    tui.keys("q");
    assert_eq!(tui.wait_for_exit(), 0);
    let saved = fs::read_to_string(home.path().join("config/helix-toolbox/settings.toml")).unwrap();
    assert!(saved.contains("theme = \"transparent\""), "{saved}");
    assert!(saved.contains("icons = false"), "{saved}");

    let mut tui = fixture_menu(home.path());
    tui.keys(BACK_TAB);
    tui.wait_for(&["transparent", "plain"]);
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
            // Installs into the user's directory; an app's version is in its
            // version file, and a newer file there means an update waits.
            // Like Flatpak away from a terminal, it changes nothing without -y.
            r#"#!/bin/sh
apps="$HOME/.local/share/flatpak/app"
case $1 in install|uninstall|update)
  case " $* " in *" -y "*) ;; *) echo "stand-in: Proceed with these changes? n" >&2; exit 1 ;; esac ;;
esac
for app; do :; done
case $1 in
  install) case $app in *Missing*) ;; *) mkdir -p "$apps/$app" ;; esac ;;
  uninstall) rm -r "$apps/$app" ;;
  list) case " $* " in *" --user "*)
    for app in "$apps"/*; do [ -d "$app" ] && printf '%s\t%s\n' "${app##*/}" "$(cat "$app/version" 2>/dev/null)"; done ;; esac ;;
  remote-ls) case " $* " in *" --user "*)
    for app in "$apps"/*; do [ -f "$app/newer" ] && printf '%s\n' "${app##*/}"; done ;; esac ;;
  update) mv "$apps/$app/newer" "$apps/$app/version" ;;
esac
"#
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
fn profiles_are_imported_planned_exported_and_deleted() {
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join("config");
    let toolbox = |args: &[&str]| {
        let output = Command::new(PROGRAM)
            .args(args)
            .current_dir(home.path())
            .env("HOME", home.path())
            .env("XDG_CONFIG_HOME", &config)
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
    let shared = home.path().join("shared.toml");
    fs::write(
        &shared,
        "name = \"Shared Box\"\nentries = [\"git\", \"meld\"]\n",
    )
    .unwrap();
    let (ok, out, err) = toolbox(&["profile", "import", shared.to_str().unwrap()]);
    assert!(ok, "{out}\n{err}");
    assert!(
        out.contains("run it with helix-toolbox run profile-shared-box"),
        "{out}"
    );
    let bad = home.path().join("bad.toml");
    fs::write(
        &bad,
        "name = \"Bad\"\nentries = [\"git\", \"no-such-entry\"]\n",
    )
    .unwrap();
    let (ok, _, err) = toolbox(&["profile", "import", bad.to_str().unwrap()]);
    assert!(!ok);
    assert!(err.contains("no-such-entry"), "{err}");
    let (_, out, _) = toolbox(&["profile", "list"]);
    assert!(
        out.contains("profile-shared-box  Shared Box") && out.contains("git, meld"),
        "{out}"
    );
    // It plans like any entry; a dry run changes nothing.
    let (ok, out, _) = toolbox(&["run", "profile-shared-box", "--dry-run"]);
    assert!(ok, "{out}");
    // Planned or already installed, depending on the machine; never run.
    assert!(out.contains("Git") && out.contains("Meld"), "{out}");
    assert!(!out.contains("==>"), "{out}");
    let (ok, _, _) = toolbox(&["profile", "export", "Shared Box"]);
    assert!(ok);
    let exported = fs::read_to_string(home.path().join("shared-box.toml")).unwrap();
    assert!(
        exported.contains("entries = [\"git\", \"meld\"]"),
        "{exported}"
    );
    let (ok, _, _) = toolbox(&["profile", "delete", "shared-box"]);
    assert!(ok);
    let (_, out, _) = toolbox(&["profile", "list"]);
    assert!(out.contains("No profiles yet"), "{out}");
}

#[test]
fn a_backup_is_listed_and_restored_with_the_current_file_kept() {
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join("config");
    let original = config.join("ghostty/config");
    let backup = config.join("ghostty/config.commander-backup-20261001000000-1");
    fs::create_dir_all(original.parent().unwrap()).unwrap();
    fs::write(&original, "current").unwrap();
    fs::write(&backup, "older").unwrap();
    let toolbox = |args: &[&str]| {
        let output = Command::new(PROGRAM)
            .arg("--catalog-dir")
            .arg(fixtures())
            .args(args)
            .env("HOME", home.path())
            .env("XDG_CONFIG_HOME", &config)
            .env("XDG_DATA_HOME", home.path().join("data"))
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
    let (ok, out, _) = toolbox(&["backups"]);
    assert!(ok);
    assert!(out.contains("2026-10-01 00:00:00  user    "), "{out}");
    assert!(out.contains(&original.display().to_string()));
    let (ok, out, err) = toolbox(&["restore", backup.to_str().unwrap(), "--yes"]);
    assert!(ok, "{out}\n{err}");
    assert!(out.contains("Helix Toolbox: restored"), "{out}");
    assert_eq!(fs::read_to_string(&original).unwrap(), "older");
    let kept: Vec<_> = fs::read_dir(original.parent().unwrap())
        .unwrap()
        .flatten()
        .filter(|item| {
            item.file_name()
                .to_string_lossy()
                .contains(".helix-toolbox-backup-")
        })
        .collect();
    assert_eq!(kept.len(), 1, "the file it replaced is kept");
    assert_eq!(fs::read_to_string(kept[0].path()).unwrap(), "current");
    // Only backups Helix Toolbox finds can be restored.
    let (ok, _, err) = toolbox(&["restore", "/etc/hostname", "--yes"]);
    assert!(!ok);
    assert!(err.contains("is not a backup Helix Toolbox knows"), "{err}");
}

#[test]
fn unfinished_entries_stay_in_the_install_plan_after_a_run() {
    let home = tempfile::tempdir().unwrap();
    let mut tui = fixture_menu(home.path());
    for name in ["Failing", "Marker"] {
        tui.search(&name.to_lowercase()[..4]);
        tui.wait_for(&[name]);
        tui.keys(" ");
    }
    tui.keys(BACK_TAB);
    tui.wait_for(&["Review planned changes", "Failing", "Marker"]);
    tui.keys("\r");
    tui.wait_for(&["CONFIRM", "After a failed step: stop"]);
    tui.keys("y");
    tui.wait_for(&["FAILED", "0 of 2 steps done"]);
    tui.keys("\r");
    tui.wait_for(&[
        "Not everything finished",
        "Failing: failed (exit 9)",
        "Marker: not run",
    ]);
    tui.keys(ESC);
    tui.wait_until("both entries to stay queued", |screen| {
        !screen.contains("Not everything finished")
            && screen.contains("Review planned changes")
            && screen.contains("Failing")
            && screen.contains("Marker")
    });
    assert!(!home.path().join("marker.txt").exists());
    tui.keys("q");
    assert_eq!(tui.wait_for_exit(), 0);
}

#[test]
fn a_failed_step_stops_the_run_or_is_stepped_over_with_keep_going() {
    let home = tempfile::tempdir().unwrap();
    let catalog = fixtures();
    let run = |extra: &[&str]| {
        let output = Command::new(PROGRAM)
            .arg("--catalog-dir")
            .arg(&catalog)
            .args(["run", "failing", "marker", "--yes"])
            .args(extra)
            .env("HOME", home.path())
            .env("XDG_STATE_HOME", home.path().join("state"))
            .env("HELIX_TOOLBOX_TEST_OUT", home.path().join("marker.txt"))
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        (
            output.status.success(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    };
    // Stop at the failure: the marker never runs.
    let (ok, out, err) = run(&[]);
    assert!(!ok);
    assert!(out.contains("Failing: failed (exit 9)"), "{out}");
    assert!(
        out.contains("Marker: not run: the run stopped first"),
        "{out}"
    );
    assert!(
        out.contains("To try the unfinished ones again: helix-toolbox run failing marker"),
        "{out}"
    );
    assert!(err.contains("exit 9, 0 of 2 steps done"), "{err}");
    assert!(!home.path().join("marker.txt").exists());
    // Keep going: the marker does not depend on it, so it runs.
    let (ok, out, err) = run(&["--keep-going"]);
    assert!(!ok, "a failed step still fails the run");
    assert!(out.contains("Marker: done"), "{out}");
    assert!(out.contains("helix-toolbox run failing\n"), "{out}");
    assert!(err.contains("1 of 2 steps done"), "{err}");
    assert!(home.path().join("marker.txt").exists());
    assert!(history_of(home.path()).contains("1 of 2 steps done"));
}

#[test]
fn entries_that_replace_each_other_are_blocked_before_anything_runs() {
    let home = tempfile::tempdir().unwrap();
    let path = stand_in_tools(&home.path().join("bin"));
    let output = Command::new(PROGRAM)
        .arg("--catalog-dir")
        .arg(package_fixtures())
        .args(["run", "conflict-a", "conflict-b", "--yes"])
        .env("PATH", &path)
        .env("HOME", home.path())
        .env("XDG_STATE_HOME", home.path().join("state"))
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    let out = String::from_utf8_lossy(&output.stdout);
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{out}\n{err}");
    assert!(
        out.contains("Blocked: Conflict A and Conflict B replace each other; queue only one"),
        "{out}"
    );
    assert!(
        err.contains("not run: fix what is blocked above first"),
        "{err}"
    );
    assert!(!out.contains("==>"), "no step started: {out}");
    assert!(
        !home
            .path()
            .join("state/helix-toolbox/history.jsonl")
            .exists(),
        "nothing was recorded as run"
    );
    // One of them alone is fine to plan.
    let output = Command::new(PROGRAM)
        .arg("--catalog-dir")
        .arg(package_fixtures())
        .args(["plan", "conflict-a"])
        .env("PATH", &path)
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Blocked"));
}

#[test]
fn an_update_is_found_run_and_verified_from_the_command_line() {
    let home = tempfile::tempdir().unwrap();
    let path = stand_in_tools(&home.path().join("bin"));
    let toolbox = |args: &[&str]| {
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
    let (ok, out, err) = toolbox(&["run", "fallback-sample", "--yes"]);
    assert!(ok, "{out}\n{err}");
    let app = home
        .path()
        .join(".local/share/flatpak/app/org.helixtoolbox.FallbackSample");
    fs::write(app.join("version"), "1.0").unwrap();
    let (_, out, _) = toolbox(&["update", "fallback-sample", "--dry-run"]);
    assert!(
        out.contains("Fallback Sample: up to date (Flatpak 1.0)"),
        "{out}"
    );

    // Flathub has a newer version: status says so, and update with no
    // names finds it, runs it and checks the version moved.
    fs::write(app.join("newer"), "1.1").unwrap();
    let (_, out, _) = toolbox(&["status", "fallback-sample"]);
    assert!(
        out.contains("Update") && out.contains("Flatpak 1.0"),
        "{out}"
    );
    let (ok, out, err) = toolbox(&["update", "--yes"]);
    assert!(ok, "{out}\n{err}");
    assert!(
        out.contains("flatpak update -y --user org.helixtoolbox.FallbackSample"),
        "{out}"
    );
    assert!(
        out.contains("Verified: Update Fallback Sample: updated 1.0 → 1.1"),
        "{out}"
    );
    let (_, out, _) = toolbox(&["status", "fallback-sample"]);
    assert!(
        out.contains("Installed") && out.contains("Flatpak 1.1"),
        "{out}"
    );
    let (_, out, _) = toolbox(&["update"]);
    assert!(out.contains("No package entry has an update"), "{out}");

    // An update that installs nothing newer is not verified.
    fs::write(app.join("newer"), "1.1").unwrap();
    let (ok, out, _) = toolbox(&["update", "--yes"]);
    assert!(!ok, "{out}");
    assert!(
        out.contains("Not verified: Update Fallback Sample: still 1.1"),
        "{out}"
    );
}

#[test]
fn package_entries_fall_back_and_are_verified_from_the_command_line() {
    let home = tempfile::tempdir().unwrap();
    let path = stand_in_tools(&home.path().join("bin"));
    let toolbox = |args: &[&str]| {
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
    let (ok, out, err) = toolbox(&["run", "fallback-sample", "--yes"]);
    assert!(ok, "{out}\n{err}");
    assert!(out.contains("failed (exit 1)"), "{out}");
    assert!(out.contains("Helix Toolbox: trying Flathub"), "{out}");
    assert!(
        out.contains(
            "Verified: Fallback Sample: org.helixtoolbox.FallbackSample is installed from Flathub"
        ),
        "{out}"
    );
    let (_, out, _) = toolbox(&["plan", "fallback-sample"]);
    assert!(out.contains("already installed"), "{out}");

    // Removal goes the way it was installed and checks it is gone.
    let (ok, out, err) = toolbox(&["remove", "fallback-sample", "--yes"]);
    assert!(ok, "{out}\n{err}");
    assert!(
        out.contains("Verified: Remove Fallback Sample: removed"),
        "{out}"
    );
    let (_, out, _) = toolbox(&["remove", "fallback-sample", "--dry-run"]);
    assert!(out.contains("not installed"), "{out}");

    // A source that says it worked without installing anything is not trusted.
    let (ok, out, err) = toolbox(&["run", "missing-sample", "--yes"]);
    assert!(!ok);
    assert!(
        out.contains("Helix Toolbox: Flathub finished, but Missing Sample is not installed"),
        "{out}"
    );
    assert!(err.contains("exit 1"), "{err}");

    let (_, history, _) = toolbox(&["history"]);
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
    tui.wait_for(&["FINISHED · VERIFIED", "Helix Toolbox: trying Flathub"]);
    tui.keys("\r");
    tui.wait_until("the running window to close", |screen| {
        !screen.contains("FINISHED")
    });
    let history =
        fs::read_to_string(home.path().join("state/helix-toolbox/history.jsonl")).unwrap();
    assert!(
        history.contains("\"result\":\"exit 0, verified\""),
        "{history}"
    );
    tui.keys("q");
    assert_eq!(tui.wait_for_exit(), 0);
}
