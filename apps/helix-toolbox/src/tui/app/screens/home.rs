//! The Home Manager screen.

use super::super::*;
use super::{Ask, Cx, Read, ToolScreen, View};

/// Home Manager's generations, building and switching.
#[derive(Default)]
pub(in crate::tui::app) struct Home {
    /// Home Manager as last read.
    pub(in crate::tui::app) status: Option<home_manager::Status>,
    reading: Read<home_manager::Status>,
    /// A run while another screen was shown left what it read out of
    /// date.
    stale: bool,
    pub(in crate::tui::app) rows: Vec<HomeRow>,
}

/// A line of the Home Manager screen.
#[derive(Clone, Debug)]
pub(in crate::tui::app) enum HomeRow {
    Heading(String),
    Text(String),
    /// A generation, by its place in the status.
    Generation(usize),
    /// A linked configuration file, by its place in the status.
    Link(usize),
}

impl Home {
    /// Read Home Manager in the background: its generations, its
    /// configuration's repository and the files it links.
    fn read(&mut self, cx: &mut Cx) {
        self.reading
            .start(cx.host, |host| home_manager::status(&host, &repos::Here));
        self.build_rows(cx);
    }

    fn selected_generation(&self, at: Option<usize>) -> Option<&home_manager::Generation> {
        let HomeRow::Generation(index) = self.rows.get(at?)? else {
            return None;
        };
        self.status.as_ref()?.generations.get(*index)
    }

    fn selected_link(&self, at: Option<usize>) -> Option<&home_manager::Link> {
        let HomeRow::Link(index) = self.rows.get(at?)? else {
            return None;
        };
        self.status.as_ref()?.links.get(*index)
    }

    /// The configuration to build, or a popup saying why there is none.
    fn config(&self, cx: &mut Cx) -> Option<home_manager::Config> {
        let why = match self.status.as_ref() {
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
        *cx.popup = Some(Popup::Text(TextPopup::new("Home Manager", why, true)));
        None
    }

    /// Enter on an earlier generation plans going back to it; on anything
    /// else it tells of it.
    fn enter(&self, cx: &mut Cx) {
        match self.selected_generation(cx.at()).cloned() {
            Some(generation) if !generation.current => {
                let planned = home_manager::plan_activate(cx.host, &generation);
                cx.confirm_or_say(planned);
            }
            _ => self.describe(cx),
        }
    }

    /// Pull the configuration's repository, fast-forward only.
    fn pull(&self, cx: &mut Cx) {
        if self.config(cx).is_none() {
            return;
        }
        let Some(repository) = self
            .status
            .as_ref()
            .and_then(|status| status.repository.clone())
        else {
            *cx.popup = Some(Popup::Text(TextPopup::new(
                "Home Manager",
                "The configuration is not in a Git repository.",
                true,
            )));
            return;
        };
        // Git is asked through its own seam, which a made-up host does not
        // stand in for.
        if !cx.host.is_here() {
            return;
        }
        let planned = repos::details(&repos::Here, &repository.path)
            .and_then(|details| repos::plan(&repos::Here, &details, &Change::Pull));
        cx.confirm_or_say(planned);
    }

    fn describe(&self, cx: &mut Cx) {
        let (title, text) = if let Some(generation) = self.selected_generation(cx.at()) {
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
        } else if let Some(link) = self.selected_link(cx.at()) {
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
        *cx.popup = Some(Popup::Text(TextPopup::new(
            helix_toolbox_core::printable(&title),
            helix_toolbox_core::printable(&text),
            true,
        )));
    }

    pub(in crate::tui::app) fn build_rows(&mut self, cx: &mut Cx) {
        let mut rows = vec![HomeRow::Heading("Home Manager".into())];
        let Some(status) = &self.status else {
            rows.push(HomeRow::Text(if self.reading.busy() {
                "Reading Home Manager…".into()
            } else if self.reading.failed() {
                "Could not read Home Manager. r reads again.".into()
            } else {
                "Press r to read Home Manager.".into()
            }));
            self.rows = rows;
            cx.clamp();
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
        self.rows = rows;
        cx.clamp();
    }
}

impl ToolScreen for Home {
    fn open(&mut self, cx: &mut Cx) {
        if std::mem::take(&mut self.stale) || self.status.is_none() {
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
            KeyCode::Char('b') => {
                if let Some(config) = self.config(cx) {
                    cx.confirm(home_manager::plan_build(&config));
                }
            }
            KeyCode::Char('s') => {
                if let Some(config) = self.config(cx) {
                    cx.confirm(home_manager::plan_switch(&config));
                }
            }
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => self.enter(cx),
            KeyCode::Char('p') => self.pull(cx),
            KeyCode::Char('d') => self.describe(cx),
            KeyCode::Char('r') => self.read(cx),
            KeyCode::Char('h') | KeyCode::Left => cx.ask(Ask::Sidebar),
            _ => {}
        }
    }

    fn hints(&self, _view: &View, at: Option<usize>, list: &mut Vec<Hint>) {
        list.push(Hint::new(&["↑↓"], "Navigate"));
        if self
            .selected_generation(at)
            .is_some_and(|generation| !generation.current)
        {
            list.push(Hint::new(&["Enter"], "Go back to it"));
        }
        if let Some(status) = &self.status
            && status.installed
            && status.config.is_some()
        {
            list.push(Hint::new(&["b"], "Build"));
            list.push(Hint::new(&["s"], "Switch"));
            if status.repository.is_some() {
                list.push(Hint::new(&["p"], "Pull"));
            }
        }
        if self.selected_generation(at).is_some() || self.selected_link(at).is_some() {
            list.push(Hint::new(&["d"], "Details"));
        }
        list.push(Hint::new(&["r"], "Read again"));
    }

    fn poll(&mut self, cx: &mut Cx) -> bool {
        let Some(answer) = self.reading.take() else {
            return false;
        };
        if let Ok(found) = answer {
            self.status = Some(found);
        }
        self.build_rows(cx);
        true
    }

    fn after_run(&mut self, cx: &mut Cx, reading: Reading) {
        if reading == Reading::Tools {
            self.read(cx);
        }
    }

    fn stale(&mut self, reading: Reading) {
        if reading == Reading::Tools {
            self.stale = true;
        }
    }

    fn draw(&self, view: &View, frame: &mut Frame, area: Rect, state: &mut ListState) {
        let inner = view.page(
            frame,
            area,
            "Home Manager",
            "Its generations and configuration; building, switching and going back are plans you review.",
        );
        view.fill(frame, inner);
        let focused = view.focused;
        let muted = Style::default().fg(view.theme.muted());
        let mut items: Vec<Line> = Vec::new();
        for row in &self.rows {
            items.push(match row {
                HomeRow::Heading(text) => {
                    Line::styled(text.clone(), Style::default().fg(view.theme.group()).bold())
                }
                HomeRow::Text(text) => {
                    Line::styled(format!("  {}", helix_toolbox_core::printable(text)), muted)
                }
                HomeRow::Generation(index) => {
                    let generation = &self.status.as_ref().unwrap().generations[*index];
                    Line::from(vec![
                        Span::styled(
                            format!("  {:>5}  {}", generation.id, generation.built),
                            Style::default().fg(if generation.current {
                                view.theme.success()
                            } else {
                                view.theme.entry()
                            }),
                        ),
                        Span::styled(
                            if generation.current {
                                "  active".to_string()
                            } else {
                                String::new()
                            },
                            Style::default().fg(view.theme.success()),
                        ),
                    ])
                }
                HomeRow::Link(index) => {
                    let link = &self.status.as_ref().unwrap().links[*index];
                    Line::from(vec![
                        Span::styled(
                            format!("  {:<12}", link.name),
                            Style::default().fg(view.theme.entry()),
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
        let list = List::new(view.measure(items))
            .block(view.card("Home Manager"))
            .highlight_style(view.highlight(focused))
            .highlight_symbol(view.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, inner, state, view.hits);
    }
}
