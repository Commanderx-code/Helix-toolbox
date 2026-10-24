//! The Restic screen.

use super::super::*;

impl App {
    /// The repository, when one is set up and usable.
    pub(in crate::tui::app) fn restic_config(&self) -> Option<&restic::Config> {
        match &self.restic_reading.as_ref()?.found {
            restic::Found::Ready(config) => Some(config),
            _ => None,
        }
    }

    /// Read the repository and its snapshots in the background. Tests set
    /// what was read themselves, so nothing of this machine's is asked.
    pub(in crate::tui::app) fn read_restic(&mut self) {
        if !self.examine_here {
            self.build_restic_rows();
            return;
        }
        let runner = self.restic.clone();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let found = restic::here();
            let snapshots = match &found {
                restic::Found::Ready(config) => restic::snapshots(runner.as_ref(), config),
                _ => Ok(Vec::new()),
            };
            let _ = sender.send(ResticReading {
                restores: matches!(found, restic::Found::Ready(_))
                    && restic::restores_safely(runner.as_ref()),
                found,
                snapshots,
                timer: restic::units_here().and_then(|units| restic::timer(&units)),
                baselines: history::state_dir()
                    .map(|state| restic::baselines(&state))
                    .unwrap_or_default(),
            });
        });
        self.reading_restic = Some(receiver);
        self.build_restic_rows();
    }

    /// List a folder of a snapshot: in the background, or at once in tests.
    fn browse_restic(&mut self, snapshot: restic::Snapshot, folder: String) {
        let Some(config) = self.restic_config().cloned() else {
            return;
        };
        let runner = self.restic.clone();
        let work = move || {
            let nodes = restic::files(runner.as_ref(), &config, &snapshot.id, &folder);
            ResticBrowse {
                snapshot,
                folder,
                nodes,
            }
        };
        if self.examine_here {
            let (sender, receiver) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = sender.send(work());
            });
            self.browsing_restic = Some(receiver);
        } else {
            self.restic_browse = Some(work());
        }
        self.build_restic_rows();
    }

    /// Look for a file's history across the snapshots.
    pub(in crate::tui::app) fn search_restic(&mut self, pattern: String) {
        let Some(config) = self.restic_config().cloned() else {
            return;
        };
        let ignore_case = self.restic_ignore_case;
        let runner = self.restic.clone();
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let work = move || {
            let found = restic::history(runner.as_ref(), &config, &home, &pattern, ignore_case);
            (pattern, found)
        };
        if self.examine_here {
            let (sender, receiver) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = sender.send(work());
            });
            self.searching_restic = Some(receiver);
        } else {
            self.restic_search = Some(work());
        }
        self.build_restic_rows();
    }

    fn restic_snapshots(&self) -> &[restic::Snapshot] {
        self.restic_reading
            .as_ref()
            .and_then(|reading| reading.snapshots.as_ref().ok())
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    fn selected_restic_row(&self) -> Option<&ResticRow> {
        self.restic_rows.get(self.main_state.selected()?)
    }

    /// Enter: browse a snapshot or folder, go up, restore a file, test a
    /// baseline.
    pub(in crate::tui::app) fn open_restic_row(&mut self) {
        match self.selected_restic_row().cloned() {
            Some(ResticRow::Snapshot(index)) => {
                let snapshot = self.restic_snapshots()[index].clone();
                let folder = snapshot
                    .paths
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "/".into());
                self.browse_restic(snapshot, folder);
            }
            Some(ResticRow::Up) => {
                if let Some(browse) = &self.restic_browse {
                    let parent = Path::new(&browse.folder)
                        .parent()
                        .map(|parent| parent.display().to_string())
                        .unwrap_or_else(|| "/".into());
                    let snapshot = browse.snapshot.clone();
                    self.browse_restic(snapshot, parent);
                }
            }
            Some(ResticRow::Node(index)) => {
                let Some(browse) = &self.restic_browse else {
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
                    self.browse_restic(snapshot, folder);
                } else {
                    self.ask_restore();
                }
            }
            Some(ResticRow::Version(..)) => self.ask_restore(),
            Some(ResticRow::Baseline(_)) => self.ask_test(),
            _ => {}
        }
    }

    /// The snapshot and path the selected row would restore.
    fn restic_restore_target(&self) -> Option<(String, Option<String>)> {
        match self.selected_restic_row()? {
            ResticRow::Snapshot(index) => {
                Some((self.restic_snapshots().get(*index)?.id.clone(), None))
            }
            ResticRow::Node(index) => {
                let browse = self.restic_browse.as_ref()?;
                let node = browse.nodes.as_ref().ok()?.get(*index)?;
                Some((browse.snapshot.id.clone(), Some(node.path.clone())))
            }
            ResticRow::Version(file, version) => {
                let found = self.restic_search.as_ref()?.1.as_ref().ok()?.get(*file)?;
                Some((
                    found.versions.get(*version)?.snapshot.clone(),
                    Some(found.path.clone()),
                ))
            }
            _ => None,
        }
    }

    /// Restore what the selected row names, or, on another row, what is typed.
    pub(in crate::tui::app) fn ask_restore(&mut self) {
        match self.restic_restore_target() {
            Some((snapshot, include)) => self.ask_restore_into(snapshot, include),
            None => self.ask_restore_from(),
        }
    }

    /// Whether this restic restores without overwriting; says why not when not.
    fn restic_restores(&mut self) -> bool {
        if self
            .restic_reading
            .as_ref()
            .is_some_and(|reading| reading.restores)
        {
            return true;
        }
        self.popup = Some(Popup::Text(TextPopup::new(
            "Cannot restore here",
            "This restic cannot restore without overwriting and check what it wrote; restic 0.17 or later can.",
            true,
        )));
        false
    }

    /// A restore typed in full: the snapshot (latest unless another is
    /// named) and, after it, only this path.
    pub(in crate::tui::app) fn ask_restore_from(&mut self) {
        if self.restic_config().is_none() || !self.restic_restores() {
            return;
        }
        self.popup = Some(Popup::Ask(
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
        &mut self,
        snapshot: String,
        include: Option<String>,
    ) {
        if !self.restic_restores() {
            return;
        }
        let what = include
            .as_deref()
            .map(helix_toolbox_core::printable)
            .unwrap_or_else(|| "the whole snapshot".to_string());
        self.popup = Some(Popup::Ask(
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

    pub(in crate::tui::app) fn restore_restic(
        &mut self,
        snapshot: &str,
        include: Option<&str>,
        text: &str,
    ) {
        let planned = (|| -> Result<Plan, String> {
            let config = self
                .restic_config()
                .ok_or("No restic repository is set up.")?;
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .ok_or("there is no home folder")?;
            let target = restic::check_target(&home, &typed_path(text)?)?;
            restic::plan_restore(config, snapshot, &target, include)
        })();
        self.confirm_restic(planned);
    }

    pub(in crate::tui::app) fn ask_test(&mut self) {
        let Some(ResticRow::Baseline(index)) = self.selected_restic_row() else {
            return;
        };
        let Some(baseline) = self
            .restic_reading
            .as_ref()
            .and_then(|reading| reading.baselines.get(*index))
        else {
            return;
        };
        self.popup = Some(Popup::Ask(
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

    pub(in crate::tui::app) fn test_restic(&mut self, baseline: &str, snapshot: &str) {
        let planned = (|| -> Result<Plan, String> {
            let config = self
                .restic_config()
                .ok_or("No restic repository is set up.")?;
            let reading = self.restic_reading.as_ref().ok_or("Nothing read yet.")?;
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
        self.confirm_restic(planned);
    }

    /// A backup or check of the repository, to review.
    pub(in crate::tui::app) fn plan_restic(&mut self, make: fn(&restic::Config) -> Plan) {
        match self.restic_config() {
            Some(config) => {
                let planned = make(config);
                self.confirm_plan(Ok(planned));
            }
            None => self.no_restic(),
        }
    }

    fn confirm_restic(&mut self, planned: Result<Plan, String>) {
        match planned {
            Ok(planned) => self.confirm_plan(Ok(planned)),
            Err(why) => {
                self.popup = Some(Popup::Text(TextPopup::new("Cannot plan", why, true)));
            }
        }
    }

    fn no_restic(&mut self) {
        self.popup = Some(Popup::Text(TextPopup::new(
            "No repository",
            "Name the repository and its password under [restic] in settings.toml (helix-toolbox restic says how), or with restic's RESTIC_REPOSITORY.",
            true,
        )));
    }

    pub(in crate::tui::app) fn record_baseline(&mut self, text: &str) -> Result<String, String> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or("there is no home folder")?;
        let state = history::state_dir().ok_or("cannot find the state folder")?;
        let recorded = restic::record(&state, &home, &typed_path(text)?, history::now())?;
        if let Some(reading) = &mut self.restic_reading {
            reading.baselines = restic::baselines(&state);
        }
        self.build_restic_rows();
        Ok(format!(
            "{}: SHA-256 {}.\n\nBack it up, then v on it restores it from a snapshot and compares.",
            recorded.path, recorded.sha256
        ))
    }

    /// The timed backup on, every day or every week, or off when it is on.
    pub(in crate::tui::app) fn toggle_restic_timer(&mut self, every: restic::Every) {
        if !self.examine_here {
            return;
        }
        let on = restic::units_here().and_then(|units| restic::timer(&units));
        self.set_restic_timer(if on.is_some() { None } else { Some(every) });
    }

    /// The timed backup's state, for Settings.
    pub(in crate::tui::app) fn restic_timer_label(&self) -> String {
        if !self.examine_here {
            return "not available here".to_string();
        }
        match restic::units_here().and_then(|units| restic::timer(&units)) {
            Some(every) => format!("yes, {}", every.label()),
            None if self.restic_config().is_some() || restic::repository_here().is_some() => {
                "no".to_string()
            }
            None => "no (name a repository in Restic first)".to_string(),
        }
    }

    /// Turn the timed backup off, or on to run that often.
    pub(in crate::tui::app) fn set_restic_timer(&mut self, every: Option<restic::Every>) {
        let config = self
            .restic_config()
            .cloned()
            .or_else(|| match restic::here() {
                restic::Found::Ready(config) => Some(config),
                _ => None,
            });
        let Some(config) = config else {
            self.no_restic();
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
        if let Some(reading) = &mut self.restic_reading {
            reading.timer = restic::units_here().and_then(|units| restic::timer(&units));
        }
        self.build_restic_rows();
        self.popup = Some(Popup::Text(match said {
            Ok(text) => TextPopup::new("Timed backup", text, true),
            Err(why) => TextPopup::new("The timer did not change", why, true),
        }));
    }

    pub(in crate::tui::app) fn describe_restic(&mut self) {
        let text = match self.selected_restic_row() {
            Some(ResticRow::Snapshot(index)) => {
                let Some(snapshot) = self.restic_snapshots().get(*index) else {
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
                    .restic_reading
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
        self.popup = Some(Popup::Text(TextPopup::new("Restic", text, true)));
    }

    pub(in crate::tui::app) fn build_restic_rows(&mut self) {
        let mut rows = vec![ResticRow::Heading("The repository".into())];
        match self.restic_reading.as_ref().map(|reading| &reading.found) {
            None if self.reading_restic.is_some() => {
                rows.push(ResticRow::Text("Asking restic…".into()))
            }
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
                    self.restic_reading
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
        if let Some(reading) = &self.restic_reading
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
        if self.browsing_restic.is_some() {
            rows.push(ResticRow::Heading("Listing the folder…".into()));
        } else if let Some(browse) = &self.restic_browse {
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
        if self.searching_restic.is_some() {
            rows.push(ResticRow::Heading("Looking through the snapshots…".into()));
        } else if let Some((pattern, found)) = &self.restic_search {
            rows.push(ResticRow::Heading(format!(
                "History of {}{}",
                helix_toolbox_core::printable(pattern),
                if self.restic_ignore_case {
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
        if let Some(reading) = &self.restic_reading {
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
        self.restic_rows = rows;
        self.clamp_main();
    }

    pub(in crate::tui::app) fn draw_restic(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Restic",
            "Backups of your own files: snapshots, a file's history, restores into a new folder and recovery tests.",
        );
        self.fill(frame, inner);
        let focused = self.focus == Focus::Main;
        let muted = Style::default().fg(self.theme.muted());
        let printable = helix_toolbox_core::printable;
        let size = |bytes: Option<u64>| bytes.map(storage::shown).unwrap_or_default();
        let snapshots = self.restic_snapshots();
        let mut items: Vec<Line> = Vec::new();
        for row in &self.restic_rows {
            items.push(match row {
                ResticRow::Heading(text) => {
                    Line::styled(text.clone(), Style::default().fg(self.theme.group()).bold())
                }
                ResticRow::Text(text) => Line::styled(format!("  {text}"), muted),
                ResticRow::Snapshot(index) => {
                    let snapshot = &snapshots[*index];
                    Line::from(vec![
                        Span::styled(
                            format!("  {}  {}  ", snapshot.short_id, snapshot.when()),
                            Style::default().fg(self.theme.entry()),
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
                ResticRow::Up => Line::styled("  ..", Style::default().fg(self.theme.entry())),
                ResticRow::Node(index) => {
                    let node =
                        &self.restic_browse.as_ref().unwrap().nodes.as_ref().unwrap()[*index];
                    if node.is_dir() {
                        Line::styled(
                            format!("  {}/", printable(&node.name)),
                            Style::default().fg(self.theme.group()),
                        )
                    } else {
                        Line::from(vec![
                            Span::styled(
                                format!("  {:<40} ", printable(&node.name)),
                                Style::default().fg(self.theme.entry()),
                            ),
                            Span::styled(size(node.size), muted),
                        ])
                    }
                }
                ResticRow::Version(file, version) => {
                    let found = &self.restic_search.as_ref().unwrap().1.as_ref().unwrap()[*file];
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
                            Style::default().fg(self.theme.entry()),
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
                    let baseline = &self.restic_reading.as_ref().unwrap().baselines[*index];
                    Line::from(vec![
                        Span::styled(
                            format!("  {}  ", &baseline.sha256[..baseline.sha256.len().min(12)]),
                            Style::default().fg(self.theme.entry()),
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
        let list = List::new(self.measure(items))
            .block(self.card("Restic"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, inner, &mut self.main_state, &self.hits);
    }
}
