//! The Settings screen and the scheduled checks.

use super::super::*;

impl App {
    pub(in crate::tui::app) fn settings(&self) -> Vec<(String, String)> {
        vec![
            ("Theme".to_string(), self.theme.label().to_string()),
            (
                "Icons".to_string(),
                if self.icons {
                    "Nerd Font".to_string()
                } else {
                    "plain".to_string()
                },
            ),
            (
                "Ask before running".to_string(),
                if self.skip_confirmation {
                    "no".to_string()
                } else {
                    "yes".to_string()
                },
            ),
            (
                "Show entries hidden on this machine".to_string(),
                if self.show_hidden {
                    "yes".to_string()
                } else {
                    "no".to_string()
                },
            ),
            (
                "After a failed step".to_string(),
                if self.keep_going {
                    "continue".to_string()
                } else {
                    "stop".to_string()
                },
            ),
            (
                "Snapshot before system changes".to_string(),
                match (self.snapshot_tool, self.snapshots) {
                    (None, _) => "not set up here".to_string(),
                    (Some(tool), true) => format!("yes ({})", tool.label()),
                    (Some(_), false) => "no".to_string(),
                },
            ),
            (
                "Tell me when updates are waiting".to_string(),
                match self.watching() {
                    None => "not available here".to_string(),
                    Some(watch) if watch.updates => format!("yes, {}", notifications::EVERY),
                    Some(_) => "no".to_string(),
                },
            ),
            (
                "Tell me of new health problems".to_string(),
                match self.watching() {
                    None => "not available here".to_string(),
                    Some(watch) if watch.health => "yes".to_string(),
                    Some(_) => "no".to_string(),
                },
            ),
            (
                "Tell me when it drifts from a baseline".to_string(),
                match self.watching() {
                    None => "not available here".to_string(),
                    Some(Watch {
                        drift: Some(slug), ..
                    }) => format!("from {slug}"),
                    Some(_) if self.baseline_slugs().is_empty() => {
                        "no (save one in Drift)".to_string()
                    }
                    Some(_) => "no".to_string(),
                },
            ),
            (
                "Back up with restic on a timer".to_string(),
                self.restic_timer_label(),
            ),
            ("Setup wizard".to_string(), "Enter opens it".to_string()),
            (
                "Bug report, to paste into an issue".to_string(),
                "Enter makes one".to_string(),
            ),
            (
                "Log level (HELIX_TOOLBOX_LOG)".to_string(),
                crate::logging::requested().unwrap_or_else(|| "info".to_string()),
            ),
            (
                "Mouse".to_string(),
                if self.mouse {
                    "on (Shift+drag selects text)".to_string()
                } else {
                    "off".to_string()
                },
            ),
        ]
    }

    pub(in crate::tui::app) fn draw_settings(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Settings",
            "Enter changes the selected setting; theme and icons are remembered.",
        );
        let focused = self.focus == Focus::Main;
        let width = inner.width.saturating_sub(6) as usize;
        let items: Vec<Line> = self
            .settings()
            .into_iter()
            .map(|(label, value)| {
                let gap = width.saturating_sub(label.len() + value.len());
                Line::from(vec![
                    Span::styled(label, Style::default().fg(self.theme.entry())),
                    Span::raw(" ".repeat(gap)),
                    Span::styled(value, Style::default().fg(self.theme.focused()).bold()),
                ])
            })
            .collect();
        self.fill(frame, inner);
        // Where they are kept, as the window says.
        let mut block = self.card("Settings");
        if let Some(path) = &self.settings_path {
            block = block.title_bottom(Line::styled(
                format!(
                    " Kept in {} ",
                    helix_toolbox_core::printable(&path.display().to_string())
                ),
                Style::default().fg(self.theme.faint()),
            ));
        }
        let list = List::new(self.measure(items))
            .block(block)
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused));
        bordered_list(frame, list, inner, &mut self.main_state, &self.hits);
    }

    pub(in crate::tui::app) fn change_setting(&mut self) {
        match self.main_state.selected().unwrap_or(0) {
            0 => {
                self.theme = self.theme.next();
                self.saved_theme = self.theme;
                self.save_look();
            }
            1 => {
                self.icons = !self.icons;
                self.save_look();
            }
            2 => self.skip_confirmation = !self.skip_confirmation,
            3 => {
                self.show_hidden = !self.show_hidden;
                match Catalog::builtin(!self.show_hidden) {
                    Ok(mut catalog) => {
                        if let Some(directory) = self.profiles_dir.clone().or_else(profiles::dir) {
                            profiles::add_to(&mut catalog, &directory);
                        }
                        self.catalog = catalog;
                        self.menu = build_menu(&self.catalog);
                        self.areas = vec![AreaState::default(); self.catalog.tabs.len()];
                        self.queue.clear();
                        self.update_rows();
                    }
                    Err(error) => {
                        self.popup = Some(Popup::Text(TextPopup::new(
                            "Cannot reload the catalog",
                            error.to_string(),
                            true,
                        )));
                    }
                }
            }
            4 => {
                self.keep_going = !self.keep_going;
                self.save_look();
            }
            5 if self.snapshot_tool.is_some() => {
                self.snapshots = !self.snapshots;
                self.save_look();
            }
            6 => self.change_watch(|watch| watch.updates = !watch.updates),
            7 => self.change_watch(|watch| watch.health = !watch.health),
            8 => {
                // The next saved baseline, then none.
                let slugs = self.baseline_slugs();
                self.change_watch(|watch| {
                    let next = match &watch.drift {
                        None => 0,
                        Some(slug) => slugs
                            .iter()
                            .position(|other| other == slug)
                            .map_or(0, |at| at + 1),
                    };
                    watch.drift = slugs.get(next).cloned();
                })
            }
            // Off, every day, every week, then off again.
            9 if self.examine_here => {
                let units = restic::units_here();
                let next = match units.as_deref().and_then(restic::timer) {
                    None => Some(restic::Every::Day),
                    Some(restic::Every::Day) => Some(restic::Every::Week),
                    Some(restic::Every::Week) => None,
                };
                self.set_restic_timer(next);
            }
            10 => self.open_wizard(false),
            11 => self.make_report(),
            13 => {
                self.mouse = !self.mouse;
                self.save_look();
            }
            _ => {}
        }
    }

    /// What the scheduled check watches; none where it cannot run.
    fn watching(&self) -> Option<Watch> {
        let places = self.notices.as_ref()?;
        self.settings_path.as_ref()?;
        Some(watch::read(places, self.settings_path.as_deref()))
    }

    /// The saved baselines' file names, for the Drift choice.
    fn baseline_slugs(&self) -> Vec<String> {
        self.drift_dir()
            .map(|directory| {
                drift::load_all(&directory)
                    .0
                    .into_iter()
                    .map(|(slug, _)| slug)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Change what the scheduled check watches, and its timer with it.
    fn change_watch(&mut self, how: impl FnOnce(&mut Watch)) {
        let (Some(places), Some(path)) = (self.notices.clone(), self.settings_path.clone()) else {
            return;
        };
        let before = watch::read(&places, Some(&path));
        let mut chosen = before.clone();
        how(&mut chosen);
        if chosen == before {
            return;
        }
        let applied = std::env::current_exe()
            .map_err(|error| format!("cannot tell where this program is: {error}"))
            .and_then(|program| {
                watch::apply(
                    &places,
                    &path,
                    &chosen,
                    &program,
                    notifications::window_here().as_deref(),
                    &self.systemctl,
                )
            });
        let turned_on = (chosen.updates && !before.updates)
            || (chosen.health && !before.health)
            || (chosen.drift.is_some() && chosen.drift != before.drift);
        let what: Vec<String> = [
            chosen.updates.then(|| "updates are waiting".to_string()),
            chosen
                .health
                .then(|| "Helix Doctor finds a new problem".to_string()),
            chosen
                .drift
                .as_ref()
                .map(|slug| format!("the machine changes since the baseline {slug}")),
        ]
        .into_iter()
        .flatten()
        .collect();
        let text = match applied {
            Err(error) => format!("They could not be turned on or off: {error}"),
            Ok(Some(notifications::Started::AtLogin)) => "The checks are set up, but systemd's user manager did not answer, so they start at your next login.".to_string(),
            Ok(_) if !turned_on => return,
            Ok(_) => format!(
                "Helix Toolbox now checks {}, and soon after you log in, and shows a notification when {}. It never updates, repairs or puts back anything itself.\n\nThe same settings turn it off again.",
                notifications::EVERY,
                what.join(", or when ")
            ),
        };
        self.popup = Some(Popup::Text(TextPopup::new("Notifications", text, true)));
    }
}
