//! The Restic screen.

use super::super::*;
use super::{Ask, Cx, Read, ToolScreen, View};
use std::sync::Arc;

/// Restic backups of your own files, and recovery tests.
pub(in crate::tui::app) struct Restic {
    /// The repository as last read.
    pub(in crate::tui::app) reading: Option<ResticReading>,
    loading: Read<ResticReading>,
    /// The snapshot folder shown.
    browse: Option<ResticBrowse>,
    browsing: Read<ResticBrowse>,
    /// The last search for a file's history.
    search: Option<ResticSearch>,
    /// Whether that search ignores upper and lower case.
    pub(in crate::tui::app) ignore_case: bool,
    searching: Read<ResticSearch>,
    pub(in crate::tui::app) rows: Vec<ResticRow>,
    /// What runs restic: restic itself, or a test's stand-in.
    pub(in crate::tui::app) runner: Arc<dyn restic::Restic + Send + Sync>,
}

impl Default for Restic {
    fn default() -> Self {
        Self {
            reading: None,
            loading: Read::default(),
            browse: None,
            browsing: Read::default(),
            search: None,
            ignore_case: false,
            searching: Read::default(),
            rows: Vec::new(),
            runner: Arc::new(restic::Here),
        }
    }
}

/// A line of the Restic screen.
#[derive(Clone, Debug)]
pub(in crate::tui::app) enum ResticRow {
    Heading(String),
    Text(String),
    Snapshot(usize),
    /// The browsed folder's parent.
    Up,
    Node(usize),
    /// A version of a file the search found: the file, then the version.
    Version(usize, usize),
    Baseline(usize),
}

/// The repository as last read, and what restic said of it.
#[derive(Clone, Debug)]
pub(in crate::tui::app) struct ResticReading {
    pub(in crate::tui::app) found: restic::Found,
    pub(in crate::tui::app) snapshots: Result<Vec<restic::Snapshot>, String>,
    /// Whether this restic restores without overwriting (0.17 and later).
    pub(in crate::tui::app) restores: bool,
    pub(in crate::tui::app) timer: Option<restic::Every>,
    pub(in crate::tui::app) baselines: Vec<restic::Baseline>,
}

/// A folder of a snapshot, and what it holds.
#[derive(Clone, Debug)]
struct ResticBrowse {
    snapshot: restic::Snapshot,
    folder: String,
    nodes: Result<Vec<restic::Node>, String>,
}

/// A search for a file's history, and what it found.
type ResticSearch = (String, Result<Vec<restic::History>, String>);

impl Restic {
    /// The repository, when one is set up and usable.
    fn config(&self) -> Option<&restic::Config> {
        match &self.reading.as_ref()?.found {
            restic::Found::Ready(config) => Some(config),
            _ => None,
        }
    }

    /// Read the repository and its snapshots in the background. Its
    /// settings and timer are this machine's own, which a made-up host does
    /// not stand in for: tests set what was read themselves.
    fn read(&mut self, cx: &mut Cx) {
        if !cx.host.is_here() {
            self.build_rows(cx);
            return;
        }
        let runner = self.runner.clone();
        self.loading.start(cx.host, move |_| {
            let found = restic::here();
            let snapshots = match &found {
                restic::Found::Ready(config) => restic::snapshots(runner.as_ref(), config),
                _ => Ok(Vec::new()),
            };
            ResticReading {
                restores: matches!(found, restic::Found::Ready(_))
                    && restic::restores_safely(runner.as_ref()),
                found,
                snapshots,
                timer: restic::units_here().and_then(|units| restic::timer(&units)),
                baselines: history::state_dir()
                    .map(|state| restic::baselines(&state))
                    .unwrap_or_default(),
            }
        });
        self.build_rows(cx);
    }

    /// List a folder of a snapshot, in the background.
    fn browse(&mut self, cx: &mut Cx, snapshot: restic::Snapshot, folder: String) {
        let Some(config) = self.config().cloned() else {
            return;
        };
        let runner = self.runner.clone();
        self.browsing.start(cx.host, move |_| {
            let nodes = restic::files(runner.as_ref(), &config, &snapshot.id, &folder);
            ResticBrowse {
                snapshot,
                folder,
                nodes,
            }
        });
        self.build_rows(cx);
    }

    /// Look for a file's history across the snapshots.
    pub(in crate::tui::app) fn search(&mut self, cx: &mut Cx, pattern: String) {
        let Some(config) = self.config().cloned() else {
            return;
        };
        let ignore_case = self.ignore_case;
        let runner = self.runner.clone();
        self.searching.start(cx.host, move |host| {
            let found =
                restic::history(runner.as_ref(), &config, host.home(), &pattern, ignore_case);
            (pattern, found)
        });
        self.build_rows(cx);
    }

    fn snapshots(&self) -> &[restic::Snapshot] {
        self.reading
            .as_ref()
            .and_then(|reading| reading.snapshots.as_ref().ok())
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    fn selected(&self, at: Option<usize>) -> Option<&ResticRow> {
        self.rows.get(at?)
    }

    /// Enter: browse a snapshot or folder, go up, restore a file, test a
    /// baseline.
    fn open_row(&mut self, cx: &mut Cx) {
        match self.selected(cx.at()).cloned() {
            Some(ResticRow::Snapshot(index)) => {
                let snapshot = self.snapshots()[index].clone();
                let folder = snapshot
                    .paths
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "/".into());
                self.browse(cx, snapshot, folder);
            }
            Some(ResticRow::Up) => {
                if let Some(browse) = &self.browse {
                    let parent = Path::new(&browse.folder)
                        .parent()
                        .map(|parent| parent.display().to_string())
                        .unwrap_or_else(|| "/".into());
                    let snapshot = browse.snapshot.clone();
                    self.browse(cx, snapshot, parent);
                }
            }
            Some(ResticRow::Node(index)) => {
                let Some(browse) = &self.browse else {
                    return;
                };
                let Some(node) = browse
                    .nodes
                    .as_ref()
                    .ok()
                    .and_then(|nodes| nodes.get(index))
                else {
                    return;
                };
                if node.is_dir() {
                    let (snapshot, folder) = (browse.snapshot.clone(), node.path.clone());
                    self.browse(cx, snapshot, folder);
                } else {
                    self.ask_restore(cx);
                }
            }
            Some(ResticRow::Version(..)) => self.ask_restore(cx),
            Some(ResticRow::Baseline(_)) => self.ask_test(cx),
            _ => {}
        }
    }

    /// The snapshot and path the selected row would restore.
    fn restore_target(&self, at: Option<usize>) -> Option<(String, Option<String>)> {
        match self.selected(at)? {
            ResticRow::Snapshot(index) => Some((self.snapshots().get(*index)?.id.clone(), None)),
            ResticRow::Node(index) => {
                let browse = self.browse.as_ref()?;
                let node = browse.nodes.as_ref().ok()?.get(*index)?;
                Some((browse.snapshot.id.clone(), Some(node.path.clone())))
            }
            ResticRow::Version(file, version) => {
                let found = self.search.as_ref()?.1.as_ref().ok()?.get(*file)?;
                Some((
                    found.versions.get(*version)?.snapshot.clone(),
                    Some(found.path.clone()),
                ))
            }
            _ => None,
        }
    }

    /// Restore what the selected row names, or, on another row, what is typed.
    fn ask_restore(&self, cx: &mut Cx) {
        match self.restore_target(cx.at()) {
            Some((snapshot, include)) => self.ask_restore_into(cx, snapshot, include),
            None => self.ask_restore_from(cx),
        }
    }

    /// Whether this restic restores without overwriting; says why not when not.
    fn restores(&self, cx: &mut Cx) -> bool {
        if self
            .reading
            .as_ref()
            .is_some_and(|reading| reading.restores)
        {
            return true;
        }
        *cx.popup = Some(Popup::Text(TextPopup::new(
            "Cannot restore here",
            "This restic cannot restore without overwriting and check what it wrote; restic 0.17 or later can.",
            true,
        )));
        false
    }

    /// A restore typed in full: the snapshot (latest unless another is
    /// named) and, after it, only this path.
    fn ask_restore_from(&self, cx: &mut Cx) {
        if self.config().is_none() || !self.restores(cx) {
            return;
        }
        *cx.popup = Some(Popup::Ask(
            InputPopup::new(
                "Restore into a new folder",
                "The snapshot (latest, or its ID), and after it, if only one path of it, that path, such as latest /home/me/Documents:",
            )
            .with_value("latest".to_string())
            .answering("Next"),
            Asked::ResticRestoreFrom,
        ));
    }

    pub(in crate::tui::app) fn ask_restore_into(
        &self,
        cx: &mut Cx,
        snapshot: String,
        include: Option<String>,
    ) {
        if !self.restores(cx) {
            return;
        }
        let what = include
            .as_deref()
            .map(helix_toolbox_core::printable)
            .unwrap_or_else(|| "the whole snapshot".to_string());
        *cx.popup = Some(Popup::Ask(
            InputPopup::new(
                "Restore into a new folder",
                format!(
                    "Where to restore {what} from {}: a new folder of your home folder; nothing is overwritten.",
                    &snapshot[..snapshot.len().min(8)]
                ),
            )
            .with_value("~/restored".to_string())
            .path()
            .answering("Restore"),
            Asked::ResticRestore(snapshot, include),
        ));
    }

    pub(in crate::tui::app) fn restore(
        &self,
        cx: &mut Cx,
        snapshot: &str,
        include: Option<&str>,
        text: &str,
    ) {
        let planned = (|| -> Result<Plan, String> {
            let config = self.config().ok_or("No restic repository is set up.")?;
            let target = restic::check_target(cx.host.home(), &typed_path(text)?)?;
            restic::plan_restore(config, snapshot, &target, include)
        })();
        cx.confirm_or_say(planned);
    }

    fn ask_test(&self, cx: &mut Cx) {
        let Some(ResticRow::Baseline(index)) = self.selected(cx.at()) else {
            return;
        };
        let Some(baseline) = self
            .reading
            .as_ref()
            .and_then(|reading| reading.baselines.get(*index))
        else {
            return;
        };
        *cx.popup = Some(Popup::Ask(
            InputPopup::new(
                "Test a restore",
                format!(
                    "The snapshot to restore {} from and compare (latest, or a snapshot's id):",
                    helix_toolbox_core::printable(&baseline.path)
                ),
            )
            .with_value("latest".to_string())
            .answering("Test"),
            Asked::ResticTest(baseline.id.clone()),
        ));
    }

    pub(in crate::tui::app) fn test(&self, cx: &mut Cx, baseline: &str, snapshot: &str) {
        let planned = (|| -> Result<Plan, String> {
            let config = self.config().ok_or("No restic repository is set up.")?;
            let reading = self.reading.as_ref().ok_or("Nothing read yet.")?;
            let chosen = restic::find_baseline(&reading.baselines, baseline)
                .ok_or("That baseline is not there.")?;
            restic::plan_test(
                config,
                chosen,
                if snapshot.is_empty() {
                    "latest"
                } else {
                    snapshot
                },
            )
        })();
        cx.confirm_or_say(planned);
    }

    /// A backup or check of the repository, to review.
    fn plan(&self, cx: &mut Cx, make: fn(&restic::Config) -> Plan) {
        match self.config() {
            Some(config) => cx.confirm(make(config)),
            None => no_repository(cx),
        }
    }

    pub(in crate::tui::app) fn record_baseline(
        &mut self,
        cx: &mut Cx,
        text: &str,
    ) -> Result<String, String> {
        let state = cx
            .host
            .state()
            .ok_or("cannot find the state folder")?
            .to_path_buf();
        let recorded = restic::record(&state, cx.host.home(), &typed_path(text)?, cx.host.now())?;
        if let Some(reading) = &mut self.reading {
            reading.baselines = restic::baselines(&state);
        }
        self.build_rows(cx);
        Ok(format!(
            "{}: SHA-256 {}.\n\nBack it up, then v on it restores it from a snapshot and compares.",
            recorded.path, recorded.sha256
        ))
    }

    /// The timed backup on, every day or every week, or off when it is on.
    fn toggle_timer(&mut self, cx: &mut Cx, every: restic::Every) {
        // The timer is kept with the user's own units, which a made-up host
        // does not stand in for.
        if !cx.host.is_here() {
            return;
        }
        let on = restic::units_here().and_then(|units| restic::timer(&units));
        self.set_timer(cx, if on.is_some() { None } else { Some(every) });
    }

    /// The timed backup's state, for Settings.
    pub(in crate::tui::app) fn timer_label(&self, host: &Host) -> String {
        if !host.is_here() {
            return "not available here".to_string();
        }
        match restic::units_here().and_then(|units| restic::timer(&units)) {
            Some(every) => format!("yes, {}", every.label()),
            None if self.config().is_some() || restic::repository_here().is_some() => {
                "no".to_string()
            }
            None => "no (name a repository in Restic first)".to_string(),
        }
    }

    /// Turn the timed backup off, or on to run that often.
    pub(in crate::tui::app) fn set_timer(&mut self, cx: &mut Cx, every: Option<restic::Every>) {
        let config = self.config().cloned().or_else(|| match restic::here() {
            restic::Found::Ready(config) => Some(config),
            _ => None,
        });
        let Some(config) = config else {
            no_repository(cx);
            return;
        };
        let said = (|| -> Result<String, String> {
            let units =
                restic::units_here().ok_or("there is no home folder to keep the timer in")?;
            let Some(every) = every else {
                restic::timer_off(&units, &notifications::systemctl)?;
                return Ok("The timed backup is off.".to_string());
            };
            let program = std::env::current_exe().map_err(|error| error.to_string())?;
            let started =
                restic::timer_on(&units, &program, &config, every, &notifications::systemctl)?;
            Ok(format!(
                "The backup runs {}{}. Settings, or w in Restic, change it.",
                every.label(),
                match started {
                    notifications::Started::Now => "",
                    notifications::Started::AtLogin => ", from the next login",
                }
            ))
        })();
        if let Some(reading) = &mut self.reading {
            reading.timer = restic::units_here().and_then(|units| restic::timer(&units));
        }
        self.build_rows(cx);
        *cx.popup = Some(Popup::Text(match said {
            Ok(text) => TextPopup::new("Timed backup", text, true),
            Err(why) => TextPopup::new("The timer did not change", why, true),
        }));
    }

    fn describe(&self, cx: &mut Cx) {
        let text = match self.selected(cx.at()) {
            Some(ResticRow::Snapshot(index)) => {
                let Some(snapshot) = self.snapshots().get(*index) else {
                    return;
                };
                format!(
                    "Snapshot {} of {}, taken {}.\nFolders: {}\n\nEnter browses it; R restores it into a new folder.",
                    snapshot.id,
                    helix_toolbox_core::printable(&snapshot.hostname),
                    snapshot.when(),
                    helix_toolbox_core::printable(&snapshot.paths.join(", "))
                )
            }
            Some(ResticRow::Baseline(index)) => {
                let Some(baseline) = self
                    .reading
                    .as_ref()
                    .and_then(|reading| reading.baselines.get(*index))
                else {
                    return;
                };
                format!(
                    "{}\nSHA-256 {}, recorded {}.\n\nv restores it from a snapshot into a temporary folder and compares.",
                    helix_toolbox_core::printable(&baseline.path),
                    baseline.sha256,
                    timestamp(baseline.recorded)
                )
            }
            _ => return,
        };
        *cx.popup = Some(Popup::Text(TextPopup::new("Restic", text, true)));
    }

    fn build_rows(&mut self, cx: &mut Cx) {
        let mut rows = vec![ResticRow::Heading("The repository".into())];
        match self.reading.as_ref().map(|reading| &reading.found) {
            None if self.loading.busy() => rows.push(ResticRow::Text("Asking restic…".into())),
            None => rows.push(ResticRow::Text("Press r to read the repository.".into())),
            Some(restic::Found::Missing) => {
                rows.push(ResticRow::Text("No restic repository is set up.".into()));
                rows.push(ResticRow::Text(
                    "Name it under [restic] in settings.toml: repository = \"…\", password_file = \"…\"; helix-toolbox restic shows how.".into(),
                ));
            }
            Some(restic::Found::Problem(why)) => rows.push(ResticRow::Text(why.clone())),
            Some(restic::Found::Ready(config)) => {
                rows.push(ResticRow::Text(format!(
                    "{} (from {})",
                    helix_toolbox_core::printable(&config.repository),
                    config.origin.label()
                )));
                rows.push(ResticRow::Text(format!(
                    "Password from {}",
                    config.password.describe()
                )));
                rows.push(ResticRow::Text(format!(
                    "Backs up {}; the timer is {}",
                    config
                        .paths
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                    self.reading
                        .as_ref()
                        .and_then(|reading| reading.timer)
                        .map(restic::Every::label)
                        .unwrap_or("off")
                )));
                if !config.excludes.is_empty() {
                    rows.push(ResticRow::Text(format!(
                        "Leaves out {}",
                        helix_toolbox_core::printable(&config.excludes.join(", "))
                    )));
                }
                rows.push(ResticRow::Text(
                    "b backs up, c checks, w backs up every day and W every week (either stops the timer), s looks for a file's history, F restores a snapshot you name.".into(),
                ));
            }
        }
        if let Some(reading) = &self.reading
            && matches!(reading.found, restic::Found::Ready(_))
        {
            match &reading.snapshots {
                Ok(snapshots) => {
                    rows.push(ResticRow::Heading(format!(
                        "Snapshots ({})",
                        snapshots.len()
                    )));
                    rows.extend((0..snapshots.len()).map(ResticRow::Snapshot));
                }
                Err(why) => rows.push(ResticRow::Text(format!(
                    "restic could not read the repository: {why}"
                ))),
            }
        }
        if self.browsing.busy() {
            rows.push(ResticRow::Heading("Listing the folder…".into()));
        } else if let Some(browse) = &self.browse {
            rows.push(ResticRow::Heading(format!(
                "In {} at {}",
                browse.snapshot.short_id,
                helix_toolbox_core::printable(&browse.folder)
            )));
            if browse.folder != "/" {
                rows.push(ResticRow::Up);
            }
            match &browse.nodes {
                Ok(nodes) if nodes.is_empty() => rows.push(ResticRow::Text("Nothing here.".into())),
                Ok(nodes) => rows.extend((0..nodes.len()).map(ResticRow::Node)),
                Err(why) => rows.push(ResticRow::Text(why.clone())),
            }
        }
        if self.searching.busy() {
            rows.push(ResticRow::Heading("Looking through the snapshots…".into()));
        } else if let Some((pattern, found)) = &self.search {
            rows.push(ResticRow::Heading(format!(
                "History of {}{}",
                helix_toolbox_core::printable(pattern),
                if self.ignore_case {
                    ", ignoring upper and lower case (i)"
                } else {
                    ""
                }
            )));
            match found {
                Ok(files) if files.is_empty() => rows.push(ResticRow::Text(
                    "No snapshot has a file that matches.".into(),
                )),
                Ok(files) => {
                    for (index, file) in files.iter().enumerate() {
                        rows.push(ResticRow::Text(helix_toolbox_core::printable(&file.path)));
                        rows.extend(
                            (0..file.versions.len())
                                .map(|version| ResticRow::Version(index, version)),
                        );
                        if file.missing_from > 0 {
                            rows.push(ResticRow::Text(format!(
                                "Not in the {} newer snapshot(s) of the same folder: deleted or moved since.",
                                file.missing_from
                            )));
                        }
                    }
                }
                Err(why) => rows.push(ResticRow::Text(why.clone())),
            }
        }
        if let Some(reading) = &self.reading {
            rows.push(ResticRow::Heading(format!(
                "Recovery tests ({} baselines)",
                reading.baselines.len()
            )));
            rows.push(ResticRow::Text(
                "n records a file's SHA-256; v on one restores it from a snapshot and compares."
                    .into(),
            ));
            rows.extend((0..reading.baselines.len()).map(ResticRow::Baseline));
        }
        self.rows = rows;
        cx.clamp();
    }
}

/// Say that no repository is set up, and where to name one.
fn no_repository(cx: &mut Cx) {
    *cx.popup = Some(Popup::Text(TextPopup::new(
        "No repository",
        "Name the repository and its password under [restic] in settings.toml (helix-toolbox restic says how), or with restic's RESTIC_REPOSITORY.",
        true,
    )));
}

impl ToolScreen for Restic {
    fn open(&mut self, cx: &mut Cx) {
        if self.reading.is_none() {
            self.read(cx);
        } else {
            self.build_rows(cx);
        }
    }

    fn len(&self, _view: &View) -> usize {
        self.rows.len()
    }

    fn key(&mut self, cx: &mut Cx, code: KeyCode) {
        match code {
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => self.open_row(cx),
            KeyCode::Char('R') => self.ask_restore(cx),
            KeyCode::Char('F') => self.ask_restore_from(cx),
            KeyCode::Char('i') => {
                self.ignore_case = !self.ignore_case;
                match self.search.as_ref().map(|(pattern, _)| pattern.clone()) {
                    Some(pattern) => self.search(cx, pattern),
                    None => self.build_rows(cx),
                }
            }
            KeyCode::Char('d') => self.describe(cx),
            KeyCode::Char('b') => self.plan(cx, restic::plan_backup),
            KeyCode::Char('c') => self.plan(cx, restic::plan_check),
            KeyCode::Char('s') if self.config().is_some() => {
                let mut ask = InputPopup::new(
                    "A file's history",
                    "A file name or path to look for in every snapshot, with * wildcards, such as notes.txt:",
                )
                .answering("Look");
                if let Some((pattern, _)) = &self.search {
                    ask = ask.with_value(pattern.clone());
                }
                *cx.popup = Some(Popup::Ask(ask, Asked::ResticHistory));
            }
            KeyCode::Char('n') => {
                *cx.popup = Some(Popup::Ask(
                    InputPopup::new(
                        "Record a baseline",
                        "A file of your home folder whose SHA-256 to record, to test restores of it against:",
                    )
                    .path()
                    .answering("Record"),
                    Asked::ResticBaseline,
                ));
            }
            KeyCode::Char('v') => self.ask_test(cx),
            KeyCode::Char('w') => self.toggle_timer(cx, restic::Every::Day),
            KeyCode::Char('W') => self.toggle_timer(cx, restic::Every::Week),
            KeyCode::Char('r') => self.read(cx),
            KeyCode::Char('h') | KeyCode::Left => cx.ask(Ask::Sidebar),
            _ => {}
        }
    }

    fn hints(&self, _view: &View, at: Option<usize>, list: &mut Vec<Hint>) {
        list.push(Hint::new(&["↑↓"], "Navigate"));
        if self.config().is_some() {
            match self.rows.get(at.unwrap_or(0)) {
                Some(ResticRow::Snapshot(_)) => {
                    list.push(Hint::new(&["Enter"], "Browse"));
                    list.push(Hint::new(&["R"], "Restore"));
                }
                Some(ResticRow::Node(_) | ResticRow::Version(..)) => {
                    list.push(Hint::new(&["R"], "Restore"));
                }
                Some(ResticRow::Baseline(_)) => list.push(Hint::new(&["v"], "Test")),
                _ => list.push(Hint::new(&["F"], "Restore…")),
            }
            list.push(Hint::new(&["b"], "Back up"));
            list.push(Hint::new(&["s"], "History"));
            if self.search.is_some() {
                list.push(Hint::new(
                    &["i"],
                    if self.ignore_case {
                        "Match case"
                    } else {
                        "Ignore case"
                    },
                ));
            }
            list.push(Hint::new(&["n"], "Baseline"));
        }
        list.push(Hint::new(&["r"], "Read again"));
    }

    fn poll(&mut self, cx: &mut Cx) -> bool {
        if let Some(found) = self.loading.take() {
            self.reading = Some(found);
        } else if let Some(found) = self.browsing.take() {
            self.browse = Some(found);
        } else if let Some(found) = self.searching.take() {
            self.search = Some(found);
        } else {
            return false;
        }
        self.build_rows(cx);
        true
    }

    fn after_run(&mut self, cx: &mut Cx, reading: Reading) {
        if reading == Reading::Tools {
            self.read(cx);
        }
    }

    fn draw(&self, view: &View, frame: &mut Frame, area: Rect, state: &mut ListState) {
        let inner = view.page(
            frame,
            area,
            "Restic",
            "Backups of your own files: snapshots, a file's history, restores into a new folder and recovery tests.",
        );
        view.fill(frame, inner);
        let focused = view.focused;
        let muted = Style::default().fg(view.theme.muted());
        let printable = helix_toolbox_core::printable;
        let size = |bytes: Option<u64>| bytes.map(storage::shown).unwrap_or_default();
        let snapshots = self.snapshots();
        let mut items: Vec<Line> = Vec::new();
        for row in &self.rows {
            items.push(match row {
                ResticRow::Heading(text) => {
                    Line::styled(text.clone(), Style::default().fg(view.theme.group()).bold())
                }
                ResticRow::Text(text) => Line::styled(format!("  {text}"), muted),
                ResticRow::Snapshot(index) => {
                    let snapshot = &snapshots[*index];
                    Line::from(vec![
                        Span::styled(
                            format!("  {}  {}  ", snapshot.short_id, snapshot.when()),
                            Style::default().fg(view.theme.entry()),
                        ),
                        Span::styled(
                            format!(
                                "{:<16} {}",
                                printable(&snapshot.hostname),
                                printable(&snapshot.paths.join(", "))
                            ),
                            muted,
                        ),
                    ])
                }
                ResticRow::Up => Line::styled("  ..", Style::default().fg(view.theme.entry())),
                ResticRow::Node(index) => {
                    let node = &self.browse.as_ref().unwrap().nodes.as_ref().unwrap()[*index];
                    if node.is_dir() {
                        Line::styled(
                            format!("  {}/", printable(&node.name)),
                            Style::default().fg(view.theme.group()),
                        )
                    } else {
                        Line::from(vec![
                            Span::styled(
                                format!("  {:<40} ", printable(&node.name)),
                                Style::default().fg(view.theme.entry()),
                            ),
                            Span::styled(size(node.size), muted),
                        ])
                    }
                }
                ResticRow::Version(file, version) => {
                    let found = &self.search.as_ref().unwrap().1.as_ref().unwrap()[*file];
                    let version = &found.versions[*version];
                    Line::from(vec![
                        Span::styled(
                            format!(
                                "    {}  {}  ",
                                version.short_id,
                                version
                                    .time
                                    .replace('T', " ")
                                    .get(..16)
                                    .unwrap_or(&version.time)
                            ),
                            Style::default().fg(view.theme.entry()),
                        ),
                        Span::styled(
                            format!(
                                "{:<10} {}",
                                size(version.size),
                                match version.change {
                                    restic::Change::First => "first",
                                    restic::Change::Changed => "changed",
                                    restic::Change::Same => "same",
                                }
                            ),
                            muted,
                        ),
                    ])
                }
                ResticRow::Baseline(index) => {
                    let baseline = &self.reading.as_ref().unwrap().baselines[*index];
                    Line::from(vec![
                        Span::styled(
                            format!("  {}  ", &baseline.sha256[..baseline.sha256.len().min(12)]),
                            Style::default().fg(view.theme.entry()),
                        ),
                        Span::styled(
                            format!(
                                "{}  {}",
                                timestamp(baseline.recorded),
                                printable(&baseline.path)
                            ),
                            muted,
                        ),
                    ])
                }
            });
        }
        let list = List::new(view.measure(items))
            .block(view.card("Restic"))
            .highlight_style(view.highlight(focused))
            .highlight_symbol(view.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, inner, state, view.hits);
    }
}
