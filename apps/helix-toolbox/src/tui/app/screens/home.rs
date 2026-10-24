//! The Home Manager screen.

use super::super::*;

impl App {
    /// Read Home Manager in the background: its generations, its
    /// configuration's repository and the files it links.
    pub(in crate::tui::app) fn read_home(&mut self) {
        if !self.examine_here {
            self.build_home_rows();
            return;
        }
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(home_manager::status(&doctor::Here, &repos::Here));
        });
        self.reading_home = Some(receiver);
        self.build_home_rows();
    }

    fn selected_generation(&self) -> Option<&home_manager::Generation> {
        let HomeRow::Generation(index) = self.home_rows.get(self.main_state.selected()?)? else {
            return None;
        };
        self.home_status.as_ref()?.generations.get(*index)
    }

    fn selected_link(&self) -> Option<&home_manager::Link> {
        let HomeRow::Link(index) = self.home_rows.get(self.main_state.selected()?)? else {
            return None;
        };
        self.home_status.as_ref()?.links.get(*index)
    }

    /// The configuration to build, or a popup saying why there is none.
    fn home_config(&mut self) -> Option<home_manager::Config> {
        let why = match self.home_status.as_ref() {
            None => "Home Manager has not been read yet; r reads it.".to_string(),
            Some(status) if !status.installed => "home-manager is not installed here.".to_string(),
            Some(home_manager::Status {
                config: Some(config),
                ..
            }) => return Some(config.clone()),
            Some(home_manager::Status {
                problem: Some(problem),
                ..
            }) => problem.clone(),
            Some(_) => "No Home Manager configuration was found: name its folder as flake under [home_manager] in settings.toml.".to_string(),
        };
        self.popup = Some(Popup::Text(TextPopup::new("Home Manager", why, true)));
        None
    }

    fn confirm_home(&mut self, planned: Result<Plan, String>) {
        match planned {
            Ok(planned) => self.confirm_plan(Ok(planned)),
            Err(why) => {
                self.popup = Some(Popup::Text(TextPopup::new("Cannot plan", why, true)));
            }
        }
    }

    pub(in crate::tui::app) fn home_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('b') => {
                if let Some(config) = self.home_config() {
                    self.confirm_home(Ok(home_manager::plan_build(&config)));
                }
            }
            KeyCode::Char('s') => {
                if let Some(config) = self.home_config() {
                    self.confirm_home(Ok(home_manager::plan_switch(&config)));
                }
            }
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => self.home_enter(),
            KeyCode::Char('p') => self.pull_home(),
            KeyCode::Char('d') => self.describe_home(),
            KeyCode::Char('r') => self.read_home(),
            KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
            _ => {}
        }
    }

    /// Enter on an earlier generation plans going back to it; on anything
    /// else it tells of it.
    fn home_enter(&mut self) {
        match self.selected_generation().cloned() {
            Some(generation) if !generation.current => {
                let planned = home_manager::plan_activate(&doctor::Here, &generation);
                self.confirm_home(planned);
            }
            _ => self.describe_home(),
        }
    }

    /// Pull the configuration's repository, fast-forward only.
    fn pull_home(&mut self) {
        if self.home_config().is_none() {
            return;
        }
        let Some(repository) = self
            .home_status
            .as_ref()
            .and_then(|status| status.repository.clone())
        else {
            self.popup = Some(Popup::Text(TextPopup::new(
                "Home Manager",
                "The configuration is not in a Git repository.",
                true,
            )));
            return;
        };
        if !self.examine_here {
            return;
        }
        let planned = repos::details(&repos::Here, &repository.path)
            .and_then(|details| repos::plan(&repos::Here, &details, &Change::Pull));
        self.confirm_home(planned);
    }

    fn describe_home(&mut self) {
        let (title, text) = if let Some(generation) = self.selected_generation() {
            (
                format!("Generation {}", generation.id),
                format!(
                    "Built {}.\n{}\n\n{}",
                    generation.built,
                    generation.path.display(),
                    if generation.current {
                        "It is the active one."
                    } else {
                        "Enter goes back to it through its own activate script, after you review the plan."
                    }
                ),
            )
        } else if let Some(link) = self.selected_link() {
            (
                link.name.clone(),
                match (&link.target, link.managed) {
                    (Some(target), true) => format!(
                        "{} links to {}.\n\nHome Manager manages it: change it in the configuration, then switch.",
                        link.path.display(),
                        target.display()
                    ),
                    (Some(target), false) => {
                        format!("{} links to {}.", link.path.display(), target.display())
                    }
                    (None, _) => format!(
                        "{} is a file of its own; Home Manager does not manage it.",
                        link.path.display()
                    ),
                },
            )
        } else {
            return;
        };
        self.popup = Some(Popup::Text(TextPopup::new(
            helix_toolbox_core::printable(&title),
            helix_toolbox_core::printable(&text),
            true,
        )));
    }

    pub(in crate::tui::app) fn home_hints(&self, list: &mut Vec<Hint>) {
        list.push(Hint::new(&["↑↓"], "Navigate"));
        if self
            .selected_generation()
            .is_some_and(|generation| !generation.current)
        {
            list.push(Hint::new(&["Enter"], "Go back to it"));
        }
        if let Some(status) = &self.home_status
            && status.installed
            && status.config.is_some()
        {
            list.push(Hint::new(&["b"], "Build"));
            list.push(Hint::new(&["s"], "Switch"));
            if status.repository.is_some() {
                list.push(Hint::new(&["p"], "Pull"));
            }
        }
        if self.selected_generation().is_some() || self.selected_link().is_some() {
            list.push(Hint::new(&["d"], "Details"));
        }
        list.push(Hint::new(&["r"], "Read again"));
    }

    pub(in crate::tui::app) fn build_home_rows(&mut self) {
        let mut rows = vec![HomeRow::Heading("Home Manager".into())];
        let Some(status) = &self.home_status else {
            rows.push(HomeRow::Text(if self.reading_home.is_some() {
                "Reading Home Manager…".into()
            } else {
                "Press r to read Home Manager.".into()
            }));
            self.home_rows = rows;
            self.clamp_main();
            return;
        };
        rows.push(HomeRow::Text(match (&status.version, status.installed) {
            (Some(version), _) => format!("Home Manager {version}"),
            (None, true) => "Home Manager is installed".into(),
            (None, false) => "home-manager is not installed here".into(),
        }));
        rows.push(HomeRow::Text(match (&status.config, &status.problem) {
            (Some(config), _) => format!(
                "Configuration: {} ({}, from {})",
                config
                    .flake_reference()
                    .unwrap_or_else(|| config.folder.join("home.nix").display().to_string()),
                if config.flake { "a flake" } else { "home.nix" },
                config.origin.label()
            ),
            (None, Some(problem)) => format!("Configuration: {problem}"),
            (None, None) => {
                "No configuration found: name its folder as flake under [home_manager] in settings.toml".into()
            }
        }));
        if let Some(repository) = &status.repository {
            rows.push(HomeRow::Text(match &repository.problem {
                Some(problem) => format!("Repository {}: {problem}", repository.path.display()),
                None => format!(
                    "Repository {}: {}, {} changed{}",
                    repository.path.display(),
                    repository.branch.as_deref().unwrap_or("detached"),
                    repository.changed,
                    match (repository.ahead, repository.behind) {
                        (Some(ahead), Some(behind)) if ahead + behind > 0 => {
                            format!(", {ahead} ahead and {behind} behind")
                        }
                        (Some(_), Some(_)) => ", up to date".to_string(),
                        _ => String::new(),
                    }
                ),
            }));
        }
        if !status.generations.is_empty() {
            rows.push(HomeRow::Heading(format!(
                "Generations ({})",
                status.generations.len()
            )));
            rows.extend((0..status.generations.len()).map(HomeRow::Generation));
        }
        if !status.links.is_empty() {
            rows.push(HomeRow::Heading("Configuration files".into()));
            rows.extend((0..status.links.len()).map(HomeRow::Link));
        }
        self.home_rows = rows;
        self.clamp_main();
    }

    pub(in crate::tui::app) fn draw_home(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Home Manager",
            "Its generations and configuration; building, switching and going back are plans you review.",
        );
        self.fill(frame, inner);
        let focused = self.focus == Focus::Main;
        let muted = Style::default().fg(self.theme.muted());
        let mut items: Vec<Line> = Vec::new();
        for row in &self.home_rows {
            items.push(match row {
                HomeRow::Heading(text) => {
                    Line::styled(text.clone(), Style::default().fg(self.theme.group()).bold())
                }
                HomeRow::Text(text) => {
                    Line::styled(format!("  {}", helix_toolbox_core::printable(text)), muted)
                }
                HomeRow::Generation(index) => {
                    let generation = &self.home_status.as_ref().unwrap().generations[*index];
                    Line::from(vec![
                        Span::styled(
                            format!("  {:>5}  {}", generation.id, generation.built),
                            Style::default().fg(if generation.current {
                                self.theme.success()
                            } else {
                                self.theme.entry()
                            }),
                        ),
                        Span::styled(
                            if generation.current {
                                "  active".to_string()
                            } else {
                                String::new()
                            },
                            Style::default().fg(self.theme.success()),
                        ),
                    ])
                }
                HomeRow::Link(index) => {
                    let link = &self.home_status.as_ref().unwrap().links[*index];
                    Line::from(vec![
                        Span::styled(
                            format!("  {:<12}", link.name),
                            Style::default().fg(self.theme.entry()),
                        ),
                        Span::styled(
                            if link.managed {
                                "linked by Home Manager".to_string()
                            } else if link.target.is_some() {
                                "a link of its own".to_string()
                            } else {
                                "a file of its own".to_string()
                            },
                            muted,
                        ),
                    ])
                }
            });
        }
        let list = List::new(self.measure(items))
            .block(self.card("Home Manager"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, inner, &mut self.main_state, &self.hits);
    }
}
