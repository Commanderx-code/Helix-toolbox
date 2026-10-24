//! The Desktop screen.

use super::super::*;

impl App {
    pub(in crate::tui::app) fn desktop_here(&self) -> helix_toolbox_engine::desktop::Desktop {
        if self.examine_here {
            crate::setup::desktop_here()
        } else {
            self.desktop.clone().unwrap_or_default()
        }
    }

    pub(in crate::tui::app) fn selected_desktop_row(&self) -> Option<&DesktopRow> {
        self.desktop_rows.get(self.main_state.selected()?)
    }

    fn looks_directory(&self) -> Option<std::path::PathBuf> {
        self.looks_dir
            .clone()
            .or_else(helix_toolbox_engine::desktop::dir)
    }

    pub(in crate::tui::app) fn reload_desktop(&mut self) {
        let found = self.desktop_here();
        let mut rows = vec![DesktopRow::Heading("This desktop".to_string())];
        let name = match (&found.name, &found.session) {
            (Some(name), Some(session)) => format!("{name}, {session} session"),
            (Some(name), None) => name.clone(),
            (None, _) => "No desktop environment found".to_string(),
        };
        rows.push(DesktopRow::Info {
            details: crate::setup::desktop_text(&found),
            text: name,
        });
        if let Some(manager) = &found.display_manager {
            rows.push(DesktopRow::Info {
                text: format!(
                    "Login screen: {}{}",
                    manager.name,
                    manager
                        .theme
                        .as_deref()
                        .map(|theme| format!(", theme {theme}"))
                        .unwrap_or_default()
                ),
                details: format!(
                    "The display manager {} runs the login screen.\n\nIts configuration:\n{}",
                    manager.name,
                    if manager.files.is_empty() {
                        "  none found".to_string()
                    } else {
                        manager
                            .files
                            .iter()
                            .map(|file| format!("  {file}"))
                            .collect::<Vec<_>>()
                            .join("\n")
                    }
                ),
            });
        }
        if !found.appearance.is_empty() {
            rows.push(DesktopRow::Heading("Appearance".to_string()));
        }
        for value in &found.appearance {
            rows.push(DesktopRow::Info {
                text: format!(
                    "{:<16}{}",
                    value.label,
                    value.value.as_deref().unwrap_or("not set")
                ),
                details: format!(
                    "{}: {}\n\ns saves the whole look, to put back later from this screen.",
                    value.label,
                    value.value.as_deref().unwrap_or("not set")
                ),
            });
        }
        for owner in &found.owners {
            rows.push(DesktopRow::Info {
                text: format!("Note: {owner}"),
                details: owner.clone(),
            });
        }
        if let Some(blocked) = &found.blocked {
            rows.push(DesktopRow::Info {
                text: format!("Not changed here: {blocked}"),
                details: blocked.clone(),
            });
        }
        let looks = self
            .looks_directory()
            .map(|directory| helix_toolbox_engine::desktop::load_all(&directory).0)
            .unwrap_or_default();
        if !looks.is_empty() {
            rows.push(DesktopRow::Heading("Saved looks".to_string()));
        }
        for (slug, look) in looks {
            let other = found.kind.map(|kind| kind.id()) != Some(look.desktop.as_str());
            rows.push(DesktopRow::Look {
                text: format!(
                    "{}{}",
                    helix_toolbox_core::printable(&look.name),
                    if other {
                        format!(
                            "  (saved on {})",
                            helix_toolbox_core::printable(&look.desktop)
                        )
                    } else {
                        format!("  ({} settings)", look.settings.len())
                    }
                ),
                slug,
            });
        }
        let offered = helix_toolbox_engine::setup::load(self.catalog.root())
            .map(|setup| setup.for_desktop(found.kind))
            .unwrap_or_default();
        let offered: Vec<_> =
            helix_toolbox_engine::setup::suggestions(&offered, &self.catalog, &self.platform)
                .into_iter()
                .filter(|suggestion| suggestion.available)
                .collect();
        if !offered.is_empty() {
            rows.push(DesktopRow::Heading("For this desktop".to_string()));
        }
        for suggestion in offered {
            let queued = self.queue.iter().any(|entry| entry.id == suggestion.id);
            rows.push(DesktopRow::Entry {
                text: format!(
                    "{}{}",
                    suggestion.name,
                    if queued { "  · planned" } else { "" }
                ),
                id: suggestion.id,
                note: suggestion.note,
            });
        }
        self.desktop = Some(found);
        self.desktop_rows = rows;
        self.clamp_main();
    }

    pub(in crate::tui::app) fn save_look_as(&mut self, name: &str) {
        let found = self.desktop_here();
        let saved = helix_toolbox_engine::desktop::capture(name, &found)
            .ok_or_else(|| {
                "no desktop whose appearance Helix Toolbox can read is running here".to_string()
            })
            .and_then(|look| {
                let directory = self
                    .looks_directory()
                    .ok_or_else(|| "cannot find the configuration folder".to_string())?;
                helix_toolbox_engine::desktop::save(&directory, &look)
                    .map(|path| (look, path))
                    .map_err(|error| error.to_string())
            });
        self.popup = Some(Popup::Text(match saved {
            Ok((look, path)) => TextPopup::new(
                "Look saved",
                format!(
                    "{} ({} settings) is saved as {}. Enter on it under Saved looks puts it back.",
                    look.name,
                    look.settings.len(),
                    path.display()
                ),
                true,
            ),
            Err(error) => TextPopup::new("Look not saved", error, true),
        }));
        self.reload_desktop();
    }

    pub(in crate::tui::app) fn apply_look(&mut self, slug: &str) {
        let Some(directory) = self.looks_directory() else {
            return;
        };
        let Some((_, look)) = helix_toolbox_engine::desktop::load_all(&directory)
            .0
            .into_iter()
            .find(|(known, _)| known == slug)
        else {
            return;
        };
        let found = self.desktop_here();
        match helix_toolbox_engine::desktop::plan_apply(&look, &found) {
            Ok(planned) => self.confirm_plan(Ok(planned)),
            Err(why) => {
                self.popup = Some(Popup::Text(TextPopup::new(
                    "Cannot apply this look",
                    why,
                    true,
                )));
            }
        }
    }

    pub(in crate::tui::app) fn draw_desktop(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Desktop",
            "The desktop, its login screen and look, and the entries for it; who else sets the look.",
        );
        self.fill(frame, inner);
        let focused = self.focus == Focus::Main;
        let items: Vec<Line> = self
            .desktop_rows
            .iter()
            .map(|row| match row {
                DesktopRow::Heading(title) => Line::styled(
                    title.clone(),
                    Style::default().fg(self.theme.group()).bold(),
                ),
                DesktopRow::Info { text, .. } => Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        text.clone(),
                        Style::default().fg(
                            if text.starts_with("Note:") || text.starts_with("Not changed") {
                                self.theme.warning()
                            } else {
                                self.theme.entry()
                            },
                        ),
                    ),
                ]),
                DesktopRow::Entry { text, note, .. } => Line::from(vec![
                    Span::styled(
                        format!("  {} ", self.glyph(Icon::Entry)),
                        Style::default().fg(self.theme.muted()),
                    ),
                    Span::styled(text.clone(), Style::default().fg(self.theme.entry())),
                    Span::styled(
                        note.as_ref()
                            .map(|note| format!("  {note}"))
                            .unwrap_or_default(),
                        Style::default().fg(self.theme.faint()),
                    ),
                ]),
                DesktopRow::Look { text, .. } => Line::from(vec![
                    Span::styled(
                        format!("  {} ", self.glyph(Icon::Entry)),
                        Style::default().fg(self.theme.muted()),
                    ),
                    Span::styled(text.clone(), Style::default().fg(self.theme.entry())),
                ]),
            })
            .collect();
        let list = List::new(self.measure(items))
            .block(self.card("This machine"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, inner, &mut self.main_state, &self.hits);
    }
}
