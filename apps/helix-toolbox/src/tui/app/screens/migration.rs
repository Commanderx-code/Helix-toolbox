//! The Migration screen.

use super::super::*;

impl App {
    /// Read in the background what a migration of this machine would hold.
    pub(in crate::tui::app) fn read_migration(&mut self) {
        if !self.examine_here {
            self.build_migration_rows();
            return;
        }
        let entries: Vec<Entry> = self.catalog.entries().into_iter().cloned().collect();
        let platform = self.platform.clone();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let refs: Vec<&Entry> = entries.iter().collect();
            // As migration::gather, keeping whether the packages chosen by
            // hand could be read.
            let inventory = Inventory::gather(&doctor::Here, &platform);
            let chosen = migration::chosen(&doctor::Here, platform.manager);
            let found = migration::export(
                &refs,
                &platform,
                &inventory,
                chosen.as_deref(),
                history::now(),
            );
            let _ = sender.send((found, chosen.is_some()));
        });
        self.reading_migration = Some(receiver);
        self.build_migration_rows();
    }

    /// Read this machine again, and the opened migration with it.
    pub(in crate::tui::app) fn reload_migration(&mut self) {
        self.read_migration();
        if let Some((path, _)) = self.migration_opened.clone() {
            self.open_migration(path);
        }
    }

    /// Read a migration file and what it means here, in the background.
    pub(in crate::tui::app) fn open_migration(&mut self, path: PathBuf) {
        if !self.examine_here {
            let read = migration::read(&path)
                .map(|read| {
                    migration::translate(
                        read,
                        &self.catalog.entries(),
                        &self.platform,
                        Self::test_machine(),
                        None,
                    )
                })
                .map_err(|error| error.to_string());
            self.opened_migration(path, read);
            return;
        }
        let entries: Vec<Entry> = self.catalog.entries().into_iter().cloned().collect();
        let platform = self.platform.clone();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let refs: Vec<&Entry> = entries.iter().collect();
            let read = migration::read_here(&doctor::Here, &path, &refs, &platform)
                .map_err(|error| error.to_string());
            let _ = sender.send((path, read));
        });
        self.opening_migration = Some(receiver);
        self.build_migration_rows();
    }

    pub(in crate::tui::app) fn opened_migration(
        &mut self,
        path: PathBuf,
        read: Result<Translation, String>,
    ) {
        match read {
            Ok(translation) => {
                self.migration_opened = Some((path, translation));
                self.build_migration_rows();
            }
            Err(error) => {
                self.build_migration_rows();
                self.popup = Some(Popup::Text(TextPopup::new(
                    "Cannot open the migration",
                    error,
                    true,
                )));
            }
        }
    }

    /// Write what this machine has to a file, as `helix-toolbox migrate
    /// export` does.
    pub(in crate::tui::app) fn export_migration(&mut self, text: &str) -> Result<String, String> {
        let path = typed_path(text)?;
        if path.extension().is_none_or(|extension| extension != "toml") {
            return Err("A migration is written to a .toml file.".to_string());
        }
        let found = match &self.migration_here {
            Some(found) => found.clone(),
            None if self.examine_here => migration::gather(
                &doctor::Here,
                &self.catalog.entries(),
                &self.platform,
                history::now(),
            ),
            None => return Err("What this machine has is still being read.".to_string()),
        };
        migration::write(&path, &found).map_err(|error| error.to_string())?;
        Ok(format!(
            "{} entries, {} Flatpaks and {} packages of yours are written to {}.\n\nOn the other machine: o in Migration opens it, or helix-toolbox migrate show.",
            found.entries.len(),
            found.flatpaks.len(),
            found.packages.len(),
            path.display()
        ))
    }

    /// Plan what the opened migration marks Install, to review and run.
    pub(in crate::tui::app) fn apply_migration(&mut self) {
        let Some((_, translation)) = &self.migration_opened else {
            self.popup = Some(Popup::Text(TextPopup::new(
                "No migration open",
                "o opens a migration file made on the other machine; then a installs what it can here.",
                true,
            )));
            return;
        };
        if translation.count(Standing::Install) == 0 {
            self.popup = Some(Popup::Text(TextPopup::new(
                "Nothing to install",
                "Everything the migration names is here already, cannot be installed here, or is left to you.",
                true,
            )));
            return;
        }
        let entries = self.catalog.entries();
        let planned = if self.examine_here {
            migration::plan_here(&doctor::Here, translation, &entries, &self.platform)
        } else {
            migration::plan(translation, &entries, &self.platform, Self::test_machine())
        };
        self.confirm_plan(planned);
    }

    pub(in crate::tui::app) fn selected_migration_item(&self) -> Option<&migration::Item> {
        let MigrationRow::Item(index) = self.migration_rows.get(self.main_state.selected()?)?
        else {
            return None;
        };
        self.migration_opened.as_ref()?.1.items.get(*index)
    }

    pub(in crate::tui::app) fn describe_migration(&mut self) {
        let Some(item) = self.selected_migration_item() else {
            return;
        };
        let kind = migration_kind(item.kind);
        let next = match item.standing {
            Standing::Install => "a installs it with the rest, after you review the plan.",
            Standing::Here => "Nothing to do: it is here already.",
            Standing::Unavailable => "Helix Toolbox has no way to install it here.",
            Standing::Yours => {
                "It is left to you: install what this distribution calls it, if you want it."
            }
        };
        let text = format!(
            "{} ({kind}): {}.\n\n{next}",
            helix_toolbox_core::printable(&item.id),
            helix_toolbox_core::printable(&item.detail)
        );
        self.popup = Some(Popup::Text(TextPopup::new(
            helix_toolbox_core::printable(&item.name),
            text,
            true,
        )));
    }

    pub(in crate::tui::app) fn build_migration_rows(&mut self) {
        let mut rows = vec![MigrationRow::Heading("This machine".into())];
        rows.push(MigrationRow::Text(match &self.migration_here {
            Some(found) => format!(
                "{} entries, {} Flatpaks and {} packages of yours, from {}",
                found.entries.len(),
                found.flatpaks.len(),
                found.packages.len(),
                found.from
            ),
            None if self.reading_migration.is_some() => "Reading what this machine has…".into(),
            None => "Press r to read what this machine has.".into(),
        }));
        if self.migration_here.is_some() && !self.migration_packages_known {
            rows.push(MigrationRow::Text(
                "The packages you installed yourself cannot be told apart here, so only the entries and Flatpaks go in the file.".into(),
            ));
        }
        rows.push(MigrationRow::Text(
            "e writes it to a file to take to another machine; o opens one made elsewhere.".into(),
        ));
        if self.opening_migration.is_some() {
            rows.push(MigrationRow::Heading("Opening the migration…".into()));
        }
        if let Some((path, translation)) = &self.migration_opened {
            let read = &translation.migration;
            rows.push(MigrationRow::Heading(format!(
                "From {} ({}), made {}{}",
                read.from,
                read.architecture,
                timestamp(read.made),
                if translation.same_family {
                    ", the same family"
                } else {
                    ""
                }
            )));
            rows.push(MigrationRow::Text(path.display().to_string()));
            for standing in [
                Standing::Install,
                Standing::Here,
                Standing::Unavailable,
                Standing::Yours,
            ] {
                let count = translation.count(standing);
                if count == 0 {
                    continue;
                }
                rows.push(MigrationRow::Heading(format!(
                    "{} ({count})",
                    match standing {
                        Standing::Install => "To install",
                        Standing::Here => "Here already",
                        Standing::Unavailable => "Cannot be installed here",
                        Standing::Yours => "Left to you",
                    }
                )));
                rows.extend(
                    translation
                        .items
                        .iter()
                        .enumerate()
                        .filter(|(_, item)| item.standing == standing)
                        .map(|(index, _)| MigrationRow::Item(index)),
                );
            }
        }
        self.migration_rows = rows;
        self.clamp_main();
    }

    pub(in crate::tui::app) fn draw_migration(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Migration",
            "Take this machine's applications to another machine or distribution. Exporting only reads.",
        );
        self.fill(frame, inner);
        let focused = self.focus == Focus::Main;
        let items_of = self
            .migration_opened
            .as_ref()
            .map(|(_, translation)| translation.items.as_slice())
            .unwrap_or_default();
        let name_width = items_of
            .iter()
            .map(|item| item.name.chars().count())
            .max()
            .unwrap_or(0)
            .min(36)
            + 2;
        let mut items: Vec<Line> = Vec::new();
        for row in &self.migration_rows {
            items.push(match row {
                MigrationRow::Heading(text) => {
                    Line::styled(text.clone(), Style::default().fg(self.theme.group()).bold())
                }
                MigrationRow::Text(text) => {
                    Line::styled(format!("  {text}"), Style::default().fg(self.theme.muted()))
                }
                MigrationRow::Item(index) => {
                    let item = &items_of[*index];
                    let color = match item.standing {
                        Standing::Install => self.theme.entry(),
                        Standing::Here => self.theme.success(),
                        Standing::Unavailable => self.theme.failure(),
                        Standing::Yours => self.theme.warning(),
                    };
                    Line::from(vec![
                        Span::styled(
                            format!(
                                "  {:<name_width$}",
                                helix_toolbox_core::printable(&item.name)
                            ),
                            Style::default().fg(color),
                        ),
                        Span::styled(
                            format!("{:<9}", migration_kind(item.kind)),
                            Style::default().fg(self.theme.muted()),
                        ),
                        Span::styled(
                            helix_toolbox_core::printable(&item.detail),
                            Style::default().fg(self.theme.muted()),
                        ),
                    ])
                }
            });
        }
        let list = List::new(self.measure(items))
            .block(self.card("Migration"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, inner, &mut self.main_state, &self.hits);
    }
}
