//! The Repositories and Workspaces screens.

use super::super::*;

impl App {
    /// Where repositories are looked for: the tests' places, else those the
    /// settings name; none in a test that sets none.
    pub(in crate::tui::app) fn places_here(&self) -> Option<repos::Places> {
        if let Some(places) = &self.repo_places {
            return Some(places.clone());
        }
        let settings = self.settings_path.as_ref()?;
        let home = std::env::var_os("HOME").map(PathBuf::from)?;
        Some(repos::places(settings, &home))
    }

    pub(in crate::tui::app) fn workspaces_path(&self) -> Option<PathBuf> {
        self.workspaces_file.clone().or_else(|| {
            self.settings_path
                .as_ref()
                .map(|settings| settings.with_file_name("workspaces.toml"))
        })
    }

    pub(in crate::tui::app) fn kept_workspaces(&self) -> Workspaces {
        self.workspaces_path()
            .and_then(|path| workspaces::load(&path).ok())
            .unwrap_or_default()
    }

    /// Find the repositories and read each, in the background on this
    /// machine; favorites first.
    pub(in crate::tui::app) fn read_repos(&mut self) {
        let Some(places) = self.places_here() else {
            self.repos_found = Some((
                repos::Places {
                    roots: Vec::new(),
                    depth: repos::DEFAULT_DEPTH,
                    origin: "default".to_string(),
                },
                Vec::new(),
            ));
            self.build_repo_rows();
            return;
        };
        let kept = self.kept_workspaces();
        let read = move || {
            let found = repos::discover(&places);
            let mut summaries = repos::summaries(&repos::Here, &found);
            summaries.sort_by_key(|summary| !kept.favorite(&summary.path));
            (places, summaries)
        };
        if self.examine_here {
            let (sender, receiver) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = sender.send(read());
            });
            self.reading_repos = Some(receiver);
        } else {
            self.repos_found = Some(read());
        }
        self.build_repo_rows();
    }

    /// Read the list again, and the open repository with it.
    pub(in crate::tui::app) fn reload_repos(&mut self) {
        self.read_repos();
        if let Some((found, _)) = &self.repo_open {
            let path = found.summary.path.clone();
            self.open_repo(path);
        }
    }

    /// Read one repository and its tasks, in the background on this machine.
    pub(in crate::tui::app) fn open_repo(&mut self, path: PathBuf) {
        let kept = self.kept_workspaces();
        let read = move || -> RepoOpened {
            let found = repos::details(&repos::Here, &path)?;
            let tasks = workspaces::tasks(&path, &kept);
            Ok((found, tasks))
        };
        if self.examine_here {
            let (sender, receiver) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = sender.send(read());
            });
            self.opening_repo = Some(receiver);
            self.build_repo_rows();
        } else {
            self.opened_repo(read());
        }
    }

    pub(in crate::tui::app) fn opened_repo(&mut self, read: RepoOpened) {
        match read {
            Ok(open) => {
                let same = self
                    .repo_open
                    .as_ref()
                    .map(|(found, _)| &found.summary.path)
                    == Some(&open.0.summary.path);
                self.repo_open = Some(open);
                if !same {
                    self.main_state.select(Some(0));
                }
                self.build_repo_rows();
            }
            Err(why) => {
                self.popup = Some(Popup::Text(TextPopup::new(
                    "Cannot read the repository",
                    why,
                    true,
                )));
            }
        }
    }

    /// Back from a repository to the list; false when the list is shown.
    pub(in crate::tui::app) fn close_repo(&mut self) -> bool {
        if self.repo_open.take().is_none() {
            return false;
        }
        self.main_state.select(Some(0));
        self.build_repo_rows();
        true
    }

    pub(in crate::tui::app) fn selected_repo_row(&self) -> Option<RepoRow> {
        self.repo_rows.get(self.main_state.selected()?).cloned()
    }

    pub(in crate::tui::app) fn repo_key(&mut self, code: KeyCode) {
        let open = self.repo_open.is_some();
        match code {
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => self.repo_enter(),
            KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace => {
                if !self.close_repo() {
                    self.focus = Focus::Sidebar;
                }
            }
            KeyCode::Char('r') => self.reload_repos(),
            KeyCode::Char('f') => self.toggle_repo_favorite(),
            _ if !open => {}
            KeyCode::Char(' ') => self.stage_selected(),
            KeyCode::Char('A') => self.plan_repo_change(Change::StageAll),
            KeyCode::Char('c') => {
                self.popup = Some(Popup::Ask(
                    InputPopup::new("Commit", "The commit message:").answering("Review"),
                    Asked::CommitMessage,
                ))
            }
            KeyCode::Char('b') => {
                self.popup = Some(Popup::Ask(
                    InputPopup::new("New branch", "The branch to create and switch to:")
                        .answering("Review"),
                    Asked::NewBranch,
                ))
            }
            KeyCode::Char('s') => {
                self.popup = Some(Popup::Ask(
                    InputPopup::new("Stash", "A message for the stash (new files go in it too):")
                        .answering("Review"),
                    Asked::StashMessage,
                ))
            }
            KeyCode::Char('F') => self.plan_repo_change(Change::Fetch),
            KeyCode::Char('p') => self.plan_repo_change(Change::Pull),
            KeyCode::Char('P') => self.push_repo(),
            KeyCode::Char('n') => {
                self.popup = Some(Popup::Ask(
                    InputPopup::new(
                        "A task of yours",
                        "The command, run in the repository's folder, such as npm run serve; begin with fish: or bash: to run it with that shell:",
                    )
                    .answering("Keep"),
                    Asked::AddTask,
                ))
            }
            KeyCode::Char('x') => self.remove_repo_task(),
            _ => {}
        }
    }

    /// Enter: open a repository; in one, a file's diff, a branch to switch
    /// to, a stash to apply or a task to run.
    pub(in crate::tui::app) fn repo_enter(&mut self) {
        let Some(row) = self.selected_repo_row() else {
            return;
        };
        let Some((found, tasks)) = &self.repo_open else {
            if let (RepoRow::Repository(index), Some((_, summaries))) = (row, &self.repos_found)
                && let Some(summary) = summaries.get(index)
            {
                let path = summary.path.clone();
                self.open_repo(path);
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
                self.popup = Some(Popup::Text(TextPopup::new(
                    helix_toolbox_core::printable(&file.path),
                    text,
                    false,
                )));
            }
            RepoRow::Branch(index) => {
                if let Some(branch) = found.branches.get(index).cloned()
                    && Some(&branch) != found.summary.branch.as_ref()
                {
                    self.plan_repo_change(Change::SwitchBranch(branch));
                }
            }
            RepoRow::Stash(index) => {
                if let Some(stash) = found.stashes.get(index) {
                    let id = stash.id.clone();
                    self.plan_repo_change(Change::ApplyStash(id));
                }
            }
            RepoRow::Task(index) => {
                if let Some(task) = tasks.get(index) {
                    match workspaces::plan_task(&found.summary.path, task) {
                        Ok(planned) => self.confirm_plan(Ok(planned)),
                        Err(why) => {
                            self.popup = Some(Popup::Text(TextPopup::new("Cannot plan", why, true)))
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Review a change to the open repository.
    pub(in crate::tui::app) fn plan_repo_change(&mut self, change: Change) {
        let Some((found, _)) = &self.repo_open else {
            return;
        };
        match repos::plan(&repos::Here, found, &change) {
            Ok(planned) => self.confirm_plan(Ok(planned)),
            Err(why) => self.popup = Some(Popup::Text(TextPopup::new("Cannot plan", why, true))),
        }
    }

    /// Space: stage the file's changes, or unstage it when all are staged.
    pub(in crate::tui::app) fn stage_selected(&mut self) {
        let (Some(RepoRow::File(index)), Some((found, _))) =
            (self.selected_repo_row(), &self.repo_open)
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
        self.plan_repo_change(change);
    }

    /// P: push to the upstream, or publish the branch to origin (or the
    /// only remote) when it has none.
    pub(in crate::tui::app) fn push_repo(&mut self) {
        let Some((found, _)) = &self.repo_open else {
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
        self.plan_repo_change(change);
    }

    pub(in crate::tui::app) fn toggle_repo_favorite(&mut self) {
        let path = match (&self.repo_open, self.selected_repo_row(), &self.repos_found) {
            (Some((found, _)), ..) => found.summary.path.clone(),
            (None, Some(RepoRow::Repository(index)), Some((_, summaries))) => {
                match summaries.get(index) {
                    Some(summary) => summary.path.clone(),
                    None => return,
                }
            }
            _ => return,
        };
        let Some(file) = self.workspaces_path() else {
            return;
        };
        let mut kept = self.kept_workspaces();
        let favorite = !kept.favorite(&path);
        kept.set_favorite(&path, favorite);
        if let Err(why) = workspaces::save(&file, &kept) {
            self.popup = Some(Popup::Text(TextPopup::new("It did not work", why, true)));
            return;
        }
        self.reload_repos();
    }

    /// Keep a command as a task of the open repository.
    pub(in crate::tui::app) fn add_repo_task(&mut self, text: &str) -> Result<String, String> {
        let Some((found, _)) = &self.repo_open else {
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
            .workspaces_path()
            .ok_or("There is no configuration folder to keep it in.")?;
        let mut kept = self.kept_workspaces();
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
        self.open_repo(path);
        Ok(format!(
            "{command} is kept as a task in {}. Enter on it runs it, after you review it.",
            file.display()
        ))
    }

    /// x: remove the selected task, when it is one of the user's.
    pub(in crate::tui::app) fn remove_repo_task(&mut self) {
        let (Some(RepoRow::Task(index)), Some((found, tasks))) =
            (self.selected_repo_row(), &self.repo_open)
        else {
            return;
        };
        let Some(task) = tasks.get(index) else {
            return;
        };
        if task.detected {
            self.popup = Some(Popup::Text(TextPopup::new(
                "Not yours to remove",
                "The repository's manifest names this task; only tasks you added are removed.",
                true,
            )));
            return;
        }
        let (path, id) = (found.summary.path.clone(), task.id.clone());
        let Some(file) = self.workspaces_path() else {
            return;
        };
        let mut kept = self.kept_workspaces();
        let removed = kept
            .remove_task(&path, &id)
            .and_then(|_| workspaces::save(&file, &kept));
        if let Err(why) = removed {
            self.popup = Some(Popup::Text(TextPopup::new("It did not work", why, true)));
            return;
        }
        self.open_repo(path);
    }

    pub(in crate::tui::app) fn repo_hints(&self, list: &mut Vec<Hint>) {
        list.push(Hint::new(&["↑↓"], "Navigate"));
        let Some((found, _)) = &self.repo_open else {
            list.push(Hint::new(&["Enter"], "Open"));
            list.push(Hint::new(&["f"], "Favorite"));
            list.push(Hint::new(&["r"], "Read again"));
            return;
        };
        match self.selected_repo_row() {
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

    pub(in crate::tui::app) fn build_repo_rows(&mut self) {
        let mut rows: Vec<RepoRow> = Vec::new();
        if let Some((found, tasks)) = &self.repo_open {
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
        } else if self.opening_repo.is_some() {
            rows.push(RepoRow::Heading("Reading the repository…".into()));
        } else {
            match &self.repos_found {
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
            if self.reading_repos.is_some() && self.repos_found.is_some() {
                rows.push(RepoRow::Text("Reading them again…".into()));
            }
        }
        self.repo_rows = rows;
        self.clamp_main();
    }

    pub(in crate::tui::app) fn draw_repos(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Repositories",
            "The Git repositories under your project folders. Reading changes nothing; every change is reviewed first.",
        );
        self.fill(frame, inner);
        let focused = self.focus == Focus::Main;
        let printable = helix_toolbox_core::printable;
        let muted = Style::default().fg(self.theme.muted());
        let kept = self.kept_workspaces();
        let mut items: Vec<Line> = Vec::new();
        for row in &self.repo_rows {
            items.push(match row {
                RepoRow::Heading(text) => Line::styled(
                    printable(text),
                    Style::default().fg(self.theme.group()).bold(),
                ),
                RepoRow::Text(text) => Line::styled(format!("  {}", printable(text)), muted),
                RepoRow::Repository(index) => {
                    let Some(summary) = self
                        .repos_found
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
                        self.theme.failure()
                    } else if summary.changed > 0 {
                        self.theme.warning()
                    } else {
                        self.theme.success()
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
                            Style::default().fg(self.theme.entry()),
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
                        .repo_open
                        .as_ref()
                        .and_then(|(found, _)| found.files.get(*index))
                    else {
                        continue;
                    };
                    let color = if file.conflict {
                        self.theme.failure()
                    } else if file.is_staged() && !file.has_unstaged() {
                        self.theme.success()
                    } else {
                        self.theme.warning()
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
                            Style::default().fg(self.theme.entry()),
                        ),
                    ])
                }
                RepoRow::Branch(index) => {
                    let Some((found, _)) = &self.repo_open else {
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
                            self.theme.success()
                        } else {
                            self.theme.entry()
                        }),
                    )
                }
                RepoRow::Stash(index) => {
                    let Some((found, _)) = &self.repo_open else {
                        continue;
                    };
                    let stash = &found.stashes[*index];
                    Line::from(vec![
                        Span::styled(
                            format!("  {:<12}", stash.name),
                            Style::default().fg(self.theme.entry()),
                        ),
                        Span::styled(printable(&stash.subject), muted),
                    ])
                }
                RepoRow::Commit(index) => {
                    let Some((found, _)) = &self.repo_open else {
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
                            Style::default().fg(self.theme.entry()),
                        ),
                    ])
                }
                RepoRow::Task(index) => {
                    let Some((_, tasks)) = &self.repo_open else {
                        continue;
                    };
                    let task = &tasks[*index];
                    Line::from(vec![
                        Span::styled(
                            format!("  {:<28}", printable(&task.name)),
                            Style::default().fg(self.theme.entry()),
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
        let list = List::new(self.measure(items))
            .block(self.card("Repositories"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, inner, &mut self.main_state, &self.hits);
    }
}
