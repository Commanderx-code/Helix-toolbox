//! The Services screen.

use super::super::*;

impl App {
    /// Read the units and the autostart entries again; quick, so not in the
    /// background. Tests keep what they put there.
    pub(in crate::tui::app) fn reload_services(&mut self) {
        if self.examine_here {
            let desktop = std::env::var("XDG_CURRENT_DESKTOP").ok();
            self.services_found = Some(services::gather(&doctor::Here, desktop.as_deref()));
        }
        self.build_service_rows();
    }

    pub(in crate::tui::app) fn unit(&self, index: usize) -> Option<&services::Unit> {
        self.services_found.as_ref()?.units.get(index)
    }

    pub(in crate::tui::app) fn selected_service_row(&self) -> Option<ServiceRow> {
        self.service_rows.get(self.main_state.selected()?).cloned()
    }

    /// The rows of the chosen view, those the filter lets through.
    pub(in crate::tui::app) fn build_service_rows(&mut self) {
        let mut rows = Vec::new();
        if let Some(found) = &self.services_found {
            let filter = self.service_filter.to_lowercase();
            let wanted = |text: &str, more: &str| {
                filter.is_empty()
                    || text.to_lowercase().contains(&filter)
                    || more.to_lowercase().contains(&filter)
            };
            if self.service_view == ServiceView::Login {
                rows.push(ServiceRow::Heading(format!(
                    "Started at login ({})",
                    found.autostart.len()
                )));
                rows.extend(
                    found
                        .autostart
                        .iter()
                        .enumerate()
                        .filter(|(_, item)| wanted(&item.name, &item.file))
                        .map(|(index, _)| ServiceRow::Autostart(index)),
                );
            } else {
                let units: Vec<usize> = found
                    .units
                    .iter()
                    .enumerate()
                    .filter(|(_, unit)| match self.service_view {
                        ServiceView::System => !unit.user && unit.kind() == "service",
                        ServiceView::Timers => unit.kind() != "service",
                        ServiceView::User => unit.user && unit.kind() == "service",
                        ServiceView::Failed => unit.failed(),
                        ServiceView::Login => false,
                    })
                    .filter(|(_, unit)| wanted(&unit.name, &unit.description))
                    .map(|(index, _)| index)
                    .collect();
                rows.push(ServiceRow::Heading(format!(
                    "{} ({})",
                    match self.service_view {
                        ServiceView::System => "The machine's services",
                        ServiceView::Timers => "Timers and sockets",
                        ServiceView::User => "Your session's services",
                        ServiceView::Failed => "Failed units",
                        ServiceView::Login => "",
                    },
                    units.len()
                )));
                rows.extend(units.into_iter().map(ServiceRow::Unit));
            }
        }
        self.service_rows = rows;
        self.clamp_main();
    }

    pub(in crate::tui::app) fn describe_unit(&mut self, index: usize) {
        let Some(unit) = self.unit(index).cloned() else {
            return;
        };
        let needed = if self.examine_here {
            services::needed_by(&doctor::Here, &unit.name, unit.user)
        } else {
            Vec::new()
        };
        let mut text = format!(
            "{}\n\nEnabled: {}\nRunning: {}{}\n",
            if unit.description.is_empty() {
                "No description."
            } else {
                unit.description.as_str()
            },
            unit.enablement
                .as_deref()
                .unwrap_or("no unit file of its own"),
            unit.active.as_deref().unwrap_or("not loaded"),
            unit.sub
                .as_deref()
                .filter(|sub| Some(*sub) != unit.active.as_deref())
                .map(|sub| format!(" ({sub})"))
                .unwrap_or_default()
        );
        if !needed.is_empty() {
            text.push_str(&format!("Needed by: {}\n", needed.join(", ")));
        }
        if let Some(cost) = services::consequence(
            &unit.name,
            unit.user,
            std::env::var_os("SSH_CONNECTION").is_some(),
        ) {
            text.push_str(&format!("\nBefore stopping or disabling it: {cost}\n"));
        }
        if !unit.switchable() {
            text.push_str(
                "\nIt is not enabled or disabled itself: another unit or a request starts it.\n",
            );
        }
        text.push_str(
            "\ns starts or stops it, e enables or disables it, R restarts it; each asks first.",
        );
        self.popup = Some(Popup::Text(TextPopup::new(
            format!("{}{}", if unit.user { "Your " } else { "" }, unit.name),
            text,
            true,
        )));
    }

    /// Plan starting, stopping, restarting, enabling or disabling a unit.
    pub(in crate::tui::app) fn change_unit(&mut self, index: usize, verb: Verb) {
        let Some(unit) = self.unit(index).cloned() else {
            return;
        };
        let needed = if self.examine_here {
            services::needed_by(&doctor::Here, &unit.name, unit.user)
        } else {
            Vec::new()
        };
        match services::plan(
            &unit.name,
            unit.user,
            verb,
            &needed,
            std::env::var_os("SSH_CONNECTION").is_some(),
        ) {
            Ok(planned) => self.confirm_plan(Ok(planned)),
            Err(why) => {
                self.popup = Some(Popup::Text(TextPopup::new("Cannot plan", why, true)));
            }
        }
    }

    /// An application started at login, as the window's side pane shows it.
    pub(in crate::tui::app) fn describe_autostart(&mut self, index: usize) {
        let Some(item) = self
            .services_found
            .as_ref()
            .and_then(|found| found.autostart.get(index))
        else {
            return;
        };
        let mut text = String::new();
        if !item.comment.is_empty() {
            text.push_str(&format!("{}\n\n", item.comment));
        }
        text.push_str(&format!(
            "Runs: {}\nFrom: {}{}\nAt login: {}\n\n",
            if item.exec.is_empty() {
                "nothing named"
            } else {
                &item.exec
            },
            if item.system {
                "the system"
            } else {
                "your own"
            },
            if item.own.is_some() && item.system {
                " · your file overrides it"
            } else {
                ""
            },
            if !item.here {
                "not in this desktop"
            } else if item.enabled {
                "started"
            } else {
                "not started"
            }
        ));
        text.push_str(&format!(
            "Enter plans {}, reviewed before it changes anything. For you only: the system's file stays as it is.",
            if item.enabled {
                "not starting it at login"
            } else {
                "starting it at login"
            }
        ));
        self.popup = Some(Popup::Text(TextPopup::new(item.name.clone(), text, true)));
    }

    /// Plan turning an autostart entry off, or on again.
    pub(in crate::tui::app) fn switch_autostart(&mut self, index: usize) {
        let Some(item) = self
            .services_found
            .as_ref()
            .and_then(|found| found.autostart.get(index))
            .cloned()
        else {
            return;
        };
        let dirs = services::AutostartDirs::here(&doctor::Here);
        match services::plan_autostart(&item, !item.enabled, &dirs, &doctor::Here) {
            Ok(planned) => self.confirm_plan(Ok(planned)),
            Err(why) => {
                self.popup = Some(Popup::Text(TextPopup::new("Cannot plan", why, true)));
            }
        }
    }

    pub(in crate::tui::app) fn draw_services(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Services",
            "systemd's units and what starts at login; each change asks first and says what it costs.",
        );
        self.fill(frame, inner);
        let focused = self.focus == Focus::Main;
        let mut items: Vec<Line> = Vec::new();
        let mut title = format!("Showing {}", self.service_view.label());
        if !self.service_filter.is_empty() {
            title.push_str(&format!(", with \"{}\"", self.service_filter));
        }
        match &self.services_found {
            None => items.push(Line::styled(
                "Press r to read this machine's services.",
                Style::default().fg(self.theme.muted()),
            )),
            Some(found) if !found.systemd && self.service_view != ServiceView::Login => {
                items.push(Line::styled(
                    "systemd does not run this machine, so there are no units; i shows what starts at login.",
                    Style::default().fg(self.theme.muted()),
                ))
            }
            Some(found) => {
                let width = inner.width.saturating_sub(8) as usize;
                for row in &self.service_rows {
                    items.push(match row {
                        ServiceRow::Heading(text) => Line::styled(
                            text.clone(),
                            Style::default().fg(self.theme.group()).bold(),
                        ),
                        ServiceRow::Unit(index) => {
                            let unit = &found.units[*index];
                            let state = format!(
                                "{:<9} {:<9}",
                                unit.enablement.as_deref().unwrap_or("-"),
                                unit.active.as_deref().unwrap_or("-")
                            );
                            let color = if unit.failed() {
                                self.theme.failure()
                            } else if unit.running() {
                                self.theme.success()
                            } else {
                                self.theme.muted()
                            };
                            let name: String = unit.name.chars().take(40).collect();
                            let room = width.saturating_sub(42 + state.len());
                            Line::from(vec![
                                Span::styled(format!("  {name:<40} "), Style::default().fg(self.theme.entry())),
                                Span::styled(state, Style::default().fg(color)),
                                Span::styled(
                                    format!("  {}", unit.description.chars().take(room).collect::<String>()),
                                    Style::default().fg(self.theme.muted()),
                                ),
                            ])
                        }
                        ServiceRow::Autostart(index) => {
                            let item = &found.autostart[*index];
                            let (word, color) = match (item.here, item.enabled) {
                                (false, _) => ("not here", self.theme.muted()),
                                (true, true) => ("on", self.theme.success()),
                                (true, false) => ("off", self.theme.warning()),
                            };
                            Line::from(vec![
                                Span::styled(format!("  {:<36} ", item.name), Style::default().fg(self.theme.entry())),
                                Span::styled(format!("{word:<9}"), Style::default().fg(color)),
                                Span::styled(
                                    format!("  {}", item.exec.chars().take(width.saturating_sub(48)).collect::<String>()),
                                    Style::default().fg(self.theme.muted()),
                                ),
                            ])
                        }
                    });
                }
            }
        }
        let list = List::new(self.measure(items))
            .block(self.card(&title))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, inner, &mut self.main_state, &self.hits);
    }
}
