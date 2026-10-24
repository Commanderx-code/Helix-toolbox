//! The Repositories and Workspaces screens.

use super::super::*;
use super::{Ask, Cx, Read, ToolScreen, View};

/// The Git repositories under the project folders, and one of them opened.
#[derive(Default)]
pub(in crate::tui::app) struct Repos {
    /// Where to look for repositories, in place of the settings' places.
    pub(in crate::tui::app) places: Option<repos::Places>,
    /// The workspaces file, in place of the one beside the settings.
    pub(in crate::tui::app) workspaces_file: Option<PathBuf>,
    /// Where they were looked for, and what was found there.
    found: Option<(repos::Places, Vec<Summary>)>,
    reading: Read<(repos::Places, Vec<Summary>)>,
    /// The opened repository, read in full, and its tasks.
    pub(in crate::tui::app) open: Option<(Details, Vec<Task>)>,
    opening: Read<RepoOpened>,
    pub(in crate::tui::app) rows: Vec<RepoRow>,
}

/// A line of the Repositories screen: in the list, a repository; in one
/// repository, a file, branch, stash, commit or task, by its place.
#[derive(Clone, Debug)]
pub(in crate::tui::app) enum RepoRow {
    Heading(String),
    Text(String),
    Repository(usize),
    File(usize),
    Branch(usize),
    Stash(usize),
    Commit(usize),
    Task(usize),
}

/// One repository read, with its tasks, or why it could not be.
type RepoOpened = Result<(Details, Vec<Task>), String>;

/// A task's command as typed, and the shell a "fish:" or "bash:" in front
/// of it names.
pub(in crate::tui::app) fn task_shell(text: &str) -> (workspaces::Shell, String) {
    let text = text.trim();
    for (prefix, shell) in [
        ("fish:", workspaces::Shell::Fish),
        ("bash:", workspaces::Shell::Bash),
    ] {
        if let Some(rest) = text.strip_prefix(prefix)
            && !rest.trim().is_empty()
        {
            return (shell, rest.trim().to_string());
        }
    }
    (workspaces::Shell::Direct, text.to_string())
}

impl Repos {
    /// Where repositories are looked for: the tests' places, else those the
    /// settings name; none in a test that sets none.
    fn places_here(&self, cx: &Cx) -> Option<repos::Places> {
        if let Some(places) = &self.places {
            return Some(places.clone());
        }
        let settings = cx.settings_path?;
        Some(repos::places(settings, cx.host.home()))
    }

    fn workspaces_path(&self, settings_path: Option<&Path>) -> Option<PathBuf> {
        self.workspaces_file
            .clone()
            .or_else(|| settings_path.map(|settings| settings.with_file_name("workspaces.toml")))
    }

    fn kept(&self, settings_path: Option<&Path>) -> Workspaces {
        self.workspaces_path(settings_path)
            .and_then(|path| workspaces::load(&path).ok())
            .unwrap_or_default()
    }

    /// Find the repositories and read each, in the background on this
    /// machine; favorites first.
    fn read(&mut self, cx: &mut Cx) {
        let Some(places) = self.places_here(cx) else {
            self.found = Some((
                repos::Places {
                    roots: Vec::new(),
                    depth: repos::DEFAULT_DEPTH,
                    origin: "default".to_string(),
                },
                Vec::new(),
            ));
            self.build_rows(cx);
            return;
        };
        let kept = self.kept(cx.settings_path);
        self.reading.start(cx.host, move |_| {
            let found = repos::discover(&places);
            let mut summaries = repos::summaries(&repos::Here, &found);
            summaries.sort_by_key(|summary| !kept.favorite(&summary.path));
            (places, summaries)
        });
        self.build_rows(cx);
    }

    /// Read the list again, and the open repository with it.
    fn reload(&mut self, cx: &mut Cx) {
        self.read(cx);
        if let Some((found, _)) = &self.open {
            let path = found.summary.path.clone();
            self.open_repo(cx, path);
        }
    }

    /// Read one repository and its tasks, in the background on this machine.
    pub(in crate::tui::app) fn open_repo(&mut self, cx: &mut Cx, path: PathBuf) {
        let kept = self.kept(cx.settings_path);
        self.opening.start(cx.host, move |_| -> RepoOpened {
            let found = repos::details(&repos::Here, &path)?;
            let tasks = workspaces::tasks(&path, &kept);
            Ok((found, tasks))
        });
        self.build_rows(cx);
    }

    fn opened(&mut self, cx: &mut Cx, read: RepoOpened) {
        match read {
            Ok(open) => {
                let same = self.open.as_ref().map(|(found, _)| &found.summary.path)
                    == Some(&open.0.summary.path);
                self.open = Some(open);
                if !same {
                    cx.selected.select(Some(0));
                }
                self.build_rows(cx);
            }
            Err(why) => {
                *cx.popup = Some(Popup::Text(TextPopup::new(
                    "Cannot read the repository",
                    why,
                    true,
                )));
            }
        }
    }

    /// Back from a repository to the list; false when the list is shown.
    fn close(&mut self, cx: &mut Cx) -> bool {
        if self.open.take().is_none() {
            return false;
        }
        cx.selected.select(Some(0));
        self.build_rows(cx);
        true
    }

    fn selected(&self, at: Option<usize>) -> Option<RepoRow> {
        self.rows.get(at?).cloned()
    }

    /// Enter: open a repository; in one, a file's diff, a branch to switch
    /// to, a stash to apply or a task to run.
    fn enter(&mut self, cx: &mut Cx) {
        let Some(row) = self.selected(cx.at()) else {
            return;
        };
        let Some((found, tasks)) = &self.open else {
            if let (RepoRow::Repository(index), Some((_, summaries))) = (row, &self.found)
                && let Some(summary) = summaries.get(index)
            {
                let path = summary.path.clone();
                self.open_repo(cx, path);
            }
            return;
        };
        match row {
            RepoRow::File(index) => {
                let Some(file) = found.files.get(index) else {
                    return;
                };
                let path = found.summary.path.clone();
                let mut text = String::new();
                for (staged, heading) in [(true, "Staged"), (false, "Not staged")] {
                    if (staged && !file.is_staged()) || (!staged && !file.has_unstaged()) {
                        continue;
                    }
                    match repos::diff(&repos::Here, &path, found, Some(&file.path), staged) {
                        Ok(shown) => {
                            text.push_str(&format!("{heading}:\n{}", shown.text));
                            if shown.truncated {
                                text.push_str("(cut short)\n");
                            }
                        }
                        Err(why) => text.push_str(&format!("{heading}: {why}\n")),
                    }
                }
                let text: String = text
                    .lines()
                    .map(|line| format!("{}\n", helix_toolbox_core::printable(line)))
                    .collect();
                *cx.popup = Some(Popup::Text(TextPopup::new(
                    helix_toolbox_core::printable(&file.path),
                    text,
                    false,
                )));
            }
            RepoRow::Branch(index) => {
                if let Some(branch) = found.branches.get(index).cloned()
                    && Some(&branch) != found.summary.branch.as_ref()
                {
                    self.plan_change(cx, Change::SwitchBranch(branch));
                }
            }
            RepoRow::Stash(index) => {
                if let Some(stash) = found.stashes.get(index) {
                    let id = stash.id.clone();
                    self.plan_change(cx, Change::ApplyStash(id));
                }
            }
            RepoRow::Task(index) => {
                if let Some(task) = tasks.get(index) {
                    cx.confirm_or_say(workspaces::plan_task(&found.summary.path, task));
                }
            }
            _ => {}
        }
    }

    /// Review a change to the open repository.
    pub(in crate::tui::app) fn plan_change(&self, cx: &mut Cx, change: Change) {
        let Some((found, _)) = &self.open else {
            return;
        };
        cx.confirm_or_say(repos::plan(&repos::Here, found, &change));
    }

    /// Space: stage the file's changes, or unstage it when all are staged.
    fn stage_selected(&self, cx: &mut Cx) {
        let (Some(RepoRow::File(index)), Some((found, _))) = (self.selected(cx.at()), &self.open)
        else {
            return;
        };
        let Some(file) = found.files.get(index) else {
            return;
        };
        let paths = vec![file.path.clone()];
        let change = if file.has_unstaged() || file.conflict {
            Change::Stage(paths)
        } else {
            Change::Unstage(paths)
        };
        self.plan_change(cx, change);
    }

    /// P: push to the upstream, or publish the branch to origin (or the
    /// only remote) when it has none.
    fn push(&self, cx: &mut Cx) {
        let Some((found, _)) = &self.open else {
            return;
        };
        let change = if found.push_to.is_some() {
            Change::Push
        } else {
            match found
                .remotes
                .iter()
                .find(|remote| *remote == "origin")
                .or(found.remotes.first())
            {
                Some(remote) => Change::Publish(remote.clone()),
                None => Change::Push,
            }
        };
        self.plan_change(cx, change);
    }

    fn toggle_favorite(&mut self, cx: &mut Cx) {
        let path = match (&self.open, self.selected(cx.at()), &self.found) {
            (Some((found, _)), ..) => found.summary.path.clone(),
            (None, Some(RepoRow::Repository(index)), Some((_, summaries))) => {
                match summaries.get(index) {
                    Some(summary) => summary.path.clone(),
                    None => return,
                }
            }
            _ => return,
        };
        let Some(file) = self.workspaces_path(cx.settings_path) else {
            return;
        };
        let mut kept = self.kept(cx.settings_path);
        let favorite = !kept.favorite(&path);
        kept.set_favorite(&path, favorite);
        if let Err(why) = workspaces::save(&file, &kept) {
            *cx.popup = Some(Popup::Text(TextPopup::new("It did not work", why, true)));
            return;
        }
        self.reload(cx);
    }

    /// Keep a command as a task of the open repository.
    pub(in crate::tui::app) fn add_task(
        &mut self,
        cx: &mut Cx,
        text: &str,
    ) -> Result<String, String> {
        let Some((found, _)) = &self.open else {
            return Err("No repository is open.".to_string());
        };
        // "fish: …" or "bash: …" runs it with that shell, as the window's
        // Run with choice; otherwise it runs as it is.
        let (shell, command) = task_shell(text);
        let mut id: String = command
            .to_lowercase()
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() {
                    character
                } else {
                    '-'
                }
            })
            .collect();
        while id.contains("--") {
            id = id.replace("--", "-");
        }
        let id: String = id.trim_matches('-').chars().take(40).collect();
        let path = found.summary.path.clone();
        let file = self
            .workspaces_path(cx.settings_path)
            .ok_or("There is no configuration folder to keep it in.")?;
        let mut kept = self.kept(cx.settings_path);
        kept.add_task(
            &path,
            Task {
                id: if id.is_empty() {
                    "task".to_string()
                } else {
                    id
                },
                name: command.chars().take(80).collect(),
                command: command.clone(),
                shell,
                detected: false,
            },
        )?;
        workspaces::save(&file, &kept)?;
        self.open_repo(cx, path);
        Ok(format!(
            "{command} is kept as a task in {}. Enter on it runs it, after you review it.",
            file.display()
        ))
    }

    /// x: remove the selected task, when it is one of the user's.
    fn remove_task(&mut self, cx: &mut Cx) {
        let (Some(RepoRow::Task(index)), Some((found, tasks))) =
            (self.selected(cx.at()), &self.open)
        else {
            return;
        };
        let Some(task) = tasks.get(index) else {
            return;
        };
        if task.detected {
            *cx.popup = Some(Popup::Text(TextPopup::new(
                "Not yours to remove",
                "The repository's manifest names this task; only tasks you added are removed.",
                true,
            )));
            return;
        }
        let (path, id) = (found.summary.path.clone(), task.id.clone());
        let Some(file) = self.workspaces_path(cx.settings_path) else {
            return;
        };
        let mut kept = self.kept(cx.settings_path);
        let removed = kept
            .remove_task(&path, &id)
            .and_then(|_| workspaces::save(&file, &kept));
        if let Err(why) = removed {
            *cx.popup = Some(Popup::Text(TextPopup::new("It did not work", why, true)));
            return;
        }
        self.open_repo(cx, path);
    }

    fn build_rows(&mut self, cx: &mut Cx) {
        let mut rows: Vec<RepoRow> = Vec::new();
        if let Some((found, tasks)) = &self.open {
            let summary = &found.summary;
            rows.push(RepoRow::Heading(format!(
                "{} on {}: {}",
                summary.name,
                summary.branch.as_deref().unwrap_or("a detached HEAD"),
                crate::repos::sync_words(summary)
            )));
            rows.push(RepoRow::Text(summary.path.display().to_string()));
            if let Some(origin) = &summary.origin {
                rows.push(RepoRow::Text(format!("origin {origin}")));
            }
            if let Some(busy) = &found.busy {
                rows.push(RepoRow::Text(format!(
                    "In the middle of {busy}: finish it with git."
                )));
            }
            if found.files.is_empty() {
                rows.push(RepoRow::Heading("No changes".into()));
            } else {
                rows.push(RepoRow::Heading(format!(
                    "Changes ({}, {} staged)",
                    found.files.len(),
                    found.staged()
                )));
                rows.extend((0..found.files.len()).map(RepoRow::File));
            }
            if !found.branches.is_empty() {
                rows.push(RepoRow::Heading("Branches".into()));
                rows.extend((0..found.branches.len()).map(RepoRow::Branch));
            }
            if !found.stashes.is_empty() {
                rows.push(RepoRow::Heading("Stashes".into()));
                rows.extend((0..found.stashes.len()).map(RepoRow::Stash));
            }
            rows.push(RepoRow::Heading("Tasks".into()));
            if tasks.is_empty() {
                rows.push(RepoRow::Text(
                    "None found in its manifests; n keeps a command of yours.".into(),
                ));
            }
            rows.extend((0..tasks.len()).map(RepoRow::Task));
            if !found.log.is_empty() {
                rows.push(RepoRow::Heading("Recent commits".into()));
                rows.extend((0..found.log.len()).map(RepoRow::Commit));
            }
        } else if self.opening.busy() {
            rows.push(RepoRow::Heading("Reading the repository…".into()));
        } else {
            match &self.found {
                None => rows.push(RepoRow::Heading("Looking for repositories…".into())),
                Some((places, summaries)) => {
                    let roots = if places.roots.is_empty() {
                        "no folders".to_string()
                    } else {
                        places
                            .roots
                            .iter()
                            .map(|root| root.display().to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    if summaries.is_empty() {
                        rows.push(RepoRow::Heading("No repositories found".into()));
                    } else {
                        rows.push(RepoRow::Heading(format!(
                            "{} repositories",
                            summaries.len()
                        )));
                        rows.extend((0..summaries.len()).map(RepoRow::Repository));
                    }
                    rows.push(RepoRow::Text(format!(
                        "Under {roots}, {} folders deep (from the {}); [repositories] roots in settings.toml changes it.",
                        places.depth, places.origin
                    )));
                }
            }
            if self.reading.busy() && self.found.is_some() {
                rows.push(RepoRow::Text("Reading them again…".into()));
            }
        }
        self.rows = rows;
        cx.clamp();
    }
}

impl ToolScreen for Repos {
    fn open(&mut self, cx: &mut Cx) {
        if self.found.is_none() {
            self.read(cx);
        } else {
            self.build_rows(cx);
        }
    }

    fn len(&self, _view: &View) -> usize {
        self.rows.len()
    }

    fn key(&mut self, cx: &mut Cx, code: KeyCode) {
        let open = self.open.is_some();
        match code {
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => self.enter(cx),
            KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace => {
                if !self.close(cx) {
                    cx.ask(Ask::Sidebar);
                }
            }
            KeyCode::Char('r') => self.reload(cx),
            KeyCode::Char('f') => self.toggle_favorite(cx),
            _ if !open => {}
            KeyCode::Char(' ') => self.stage_selected(cx),
            KeyCode::Char('A') => self.plan_change(cx, Change::StageAll),
            KeyCode::Char('c') => {
                *cx.popup = Some(Popup::Ask(
                    InputPopup::new("Commit", "The commit message:").answering("Review"),
                    Asked::CommitMessage,
                ))
            }
            KeyCode::Char('b') => {
                *cx.popup = Some(Popup::Ask(
                    InputPopup::new("New branch", "The branch to create and switch to:")
                        .answering("Review"),
                    Asked::NewBranch,
                ))
            }
            KeyCode::Char('s') => {
                *cx.popup = Some(Popup::Ask(
                    InputPopup::new("Stash", "A message for the stash (new files go in it too):")
                        .answering("Review"),
                    Asked::StashMessage,
                ))
            }
            KeyCode::Char('F') => self.plan_change(cx, Change::Fetch),
            KeyCode::Char('p') => self.plan_change(cx, Change::Pull),
            KeyCode::Char('P') => self.push(cx),
            KeyCode::Char('n') => {
                *cx.popup = Some(Popup::Ask(
                    InputPopup::new(
                        "A task of yours",
                        "The command, run in the repository's folder, such as npm run serve; begin with fish: or bash: to run it with that shell:",
                    )
                    .answering("Keep"),
                    Asked::AddTask,
                ))
            }
            KeyCode::Char('x') => self.remove_task(cx),
            _ => {}
        }
    }

    fn hints(&self, _view: &View, at: Option<usize>, list: &mut Vec<Hint>) {
        list.push(Hint::new(&["↑↓"], "Navigate"));
        let Some((found, _)) = &self.open else {
            list.push(Hint::new(&["Enter"], "Open"));
            list.push(Hint::new(&["f"], "Favorite"));
            list.push(Hint::new(&["r"], "Read again"));
            return;
        };
        match self.selected(at) {
            Some(RepoRow::File(_)) => {
                list.push(Hint::new(&["Enter"], "Diff"));
                list.push(Hint::new(&["Space"], "Stage"));
            }
            Some(RepoRow::Branch(_)) => list.push(Hint::new(&["Enter"], "Switch")),
            Some(RepoRow::Stash(_)) => list.push(Hint::new(&["Enter"], "Apply")),
            Some(RepoRow::Task(_)) => {
                list.push(Hint::new(&["Enter"], "Run"));
                list.push(Hint::new(&["x"], "Remove"));
            }
            _ => {}
        }
        if found.staged() > 0 {
            list.push(Hint::new(&["c"], "Commit"));
        }
        list.push(Hint::new(&["b"], "Branch"));
        if !found.clean() {
            list.push(Hint::new(&["s"], "Stash"));
        }
        if !found.remotes.is_empty() {
            list.push(Hint::new(&["F", "p", "P"], "Fetch, pull, push"));
        }
        list.push(Hint::new(&["n"], "Task"));
        list.push(Hint::new(&["h"], "Back"));
    }

    fn poll(&mut self, cx: &mut Cx) -> bool {
        if let Some(found) = self.reading.take() {
            self.found = Some(found);
            self.build_rows(cx);
            return true;
        }
        if let Some(read) = self.opening.take() {
            self.opened(cx, read);
            return true;
        }
        false
    }

    fn after_run(&mut self, cx: &mut Cx, reading: Reading) {
        if reading == Reading::Tools {
            self.reload(cx);
        }
    }

    fn draw(&self, view: &View, frame: &mut Frame, area: Rect, state: &mut ListState) {
        let inner = view.page(
            frame,
            area,
            "Repositories",
            "The Git repositories under your project folders. Reading changes nothing; every change is reviewed first.",
        );
        view.fill(frame, inner);
        let focused = view.focused;
        let printable = helix_toolbox_core::printable;
        let muted = Style::default().fg(view.theme.muted());
        let kept = self.kept(view.settings_path);
        let mut items: Vec<Line> = Vec::new();
        for row in &self.rows {
            items.push(match row {
                RepoRow::Heading(text) => Line::styled(
                    printable(text),
                    Style::default().fg(view.theme.group()).bold(),
                ),
                RepoRow::Text(text) => Line::styled(format!("  {}", printable(text)), muted),
                RepoRow::Repository(index) => {
                    let Some(summary) = self
                        .found
                        .as_ref()
                        .and_then(|(_, summaries)| summaries.get(*index))
                    else {
                        continue;
                    };
                    let changes = match (&summary.problem, summary.changed) {
                        (Some(problem), _) => problem.clone(),
                        (None, 0) => "clean".to_string(),
                        (None, count) => format!("{count} changed"),
                    };
                    let color = if summary.problem.is_some() {
                        view.theme.failure()
                    } else if summary.changed > 0 {
                        view.theme.warning()
                    } else {
                        view.theme.success()
                    };
                    Line::from(vec![
                        Span::styled(
                            format!(
                                "{}{:<26}",
                                if kept.favorite(&summary.path) {
                                    "★ "
                                } else {
                                    "  "
                                },
                                printable(&summary.name)
                            ),
                            Style::default().fg(view.theme.entry()),
                        ),
                        Span::styled(
                            format!(
                                "{:<20}",
                                printable(summary.branch.as_deref().unwrap_or("(detached)"))
                            ),
                            muted,
                        ),
                        Span::styled(format!("{changes:<14}"), Style::default().fg(color)),
                        Span::styled(format!("{:<18}", crate::repos::sync_words(summary)), muted),
                        Span::styled(
                            printable(summary.last.as_deref().unwrap_or_default()),
                            muted,
                        ),
                    ])
                }
                RepoRow::File(index) => {
                    let Some(file) = self
                        .open
                        .as_ref()
                        .and_then(|(found, _)| found.files.get(*index))
                    else {
                        continue;
                    };
                    let color = if file.conflict {
                        view.theme.failure()
                    } else if file.is_staged() && !file.has_unstaged() {
                        view.theme.success()
                    } else {
                        view.theme.warning()
                    };
                    Line::from(vec![
                        Span::styled(
                            format!(
                                "  {} {:<13}",
                                if file.is_staged() { "●" } else { "○" },
                                file.state()
                            ),
                            Style::default().fg(color),
                        ),
                        Span::styled(
                            format!(
                                "{}{}",
                                printable(&file.path),
                                file.original
                                    .as_deref()
                                    .map(|original| format!(" (was {})", printable(original)))
                                    .unwrap_or_default()
                            ),
                            Style::default().fg(view.theme.entry()),
                        ),
                    ])
                }
                RepoRow::Branch(index) => {
                    let Some((found, _)) = &self.open else {
                        continue;
                    };
                    let branch = &found.branches[*index];
                    let current = found.summary.branch.as_ref() == Some(branch);
                    Line::styled(
                        format!(
                            "  {} {}",
                            if current { "●" } else { " " },
                            printable(branch)
                        ),
                        Style::default().fg(if current {
                            view.theme.success()
                        } else {
                            view.theme.entry()
                        }),
                    )
                }
                RepoRow::Stash(index) => {
                    let Some((found, _)) = &self.open else {
                        continue;
                    };
                    let stash = &found.stashes[*index];
                    Line::from(vec![
                        Span::styled(
                            format!("  {:<12}", stash.name),
                            Style::default().fg(view.theme.entry()),
                        ),
                        Span::styled(printable(&stash.subject), muted),
                    ])
                }
                RepoRow::Commit(index) => {
                    let Some((found, _)) = &self.open else {
                        continue;
                    };
                    let commit = &found.log[*index];
                    Line::from(vec![
                        Span::styled(format!("  {:<10}", commit.hash), muted),
                        Span::styled(
                            format!(
                                "{:<12}",
                                commit
                                    .at
                                    .map(helix_toolbox_engine::date)
                                    .unwrap_or_default()
                            ),
                            muted,
                        ),
                        Span::styled(
                            printable(&commit.subject),
                            Style::default().fg(view.theme.entry()),
                        ),
                    ])
                }
                RepoRow::Task(index) => {
                    let Some((_, tasks)) = &self.open else {
                        continue;
                    };
                    let task = &tasks[*index];
                    Line::from(vec![
                        Span::styled(
                            format!("  {:<28}", printable(&task.name)),
                            Style::default().fg(view.theme.entry()),
                        ),
                        Span::styled(
                            format!(
                                "{}{}{}",
                                if task.detected { "" } else { "yours: " },
                                match task.shell {
                                    workspaces::Shell::Direct => "",
                                    workspaces::Shell::Fish => "fish: ",
                                    workspaces::Shell::Bash => "bash: ",
                                },
                                printable(&task.command)
                            ),
                            muted,
                        ),
                    ])
                }
            });
        }
        let list = List::new(view.measure(items))
            .block(view.card("Repositories"))
            .highlight_style(view.highlight(focused))
            .highlight_symbol(view.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, inner, state, view.hits);
    }
}
