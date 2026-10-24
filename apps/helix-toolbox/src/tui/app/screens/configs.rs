//! The Configs screen.

use super::super::*;

impl App {
    /// Where a configuration is: in the tests' folders, else this machine's.
    fn locate_config(&self, kind: configs::Kind) -> Result<configs::Located, String> {
        match &self.config_places {
            Some((home, config)) => configs::locate(
                kind,
                &configs::Facts {
                    home,
                    config_home: config,
                    settings: None,
                    command_center: None,
                    store: Path::new("/nix/store"),
                },
            ),
            None if self.examine_here => configs::here(kind),
            None => Err("configurations are not read here".to_string()),
        }
    }

    /// The private folder for copies: the tests' own, else beside the
    /// history; never a shared one.
    fn copies_folder(&self) -> Option<PathBuf> {
        self.config_folder.clone().or_else(configs::private_folder)
    }

    fn no_copies_folder(&mut self) {
        self.cannot_config(
            "There is no state folder for Helix Toolbox's private copies; nothing is saved.".into(),
        );
    }

    /// Ghostty's own check on this machine; none in tests.
    fn config_check(&self) -> fn(&Path) -> Option<(bool, String)> {
        if self.examine_here {
            configs::ghostty_here
        } else {
            |_| None
        }
    }

    /// Read both configurations, their checks and their backups.
    pub(in crate::tui::app) fn read_configs(&mut self) {
        let check = self.config_check();
        self.config_parts = configs::Kind::ALL
            .into_iter()
            .map(|kind| {
                let found = self.locate_config(kind);
                let (loaded, checked, history) = match &found {
                    Ok(located) => {
                        let loaded = configs::read(located);
                        let checked = loaded.as_ref().ok().map(|loaded| {
                            configs::validate(
                                kind,
                                &loaded.content,
                                &check,
                                self.copies_folder().as_deref(),
                            )
                        });
                        (Some(loaded), checked, configs::history(located))
                    }
                    Err(_) => (None, None, Vec::new()),
                };
                ConfigPart {
                    kind,
                    found,
                    loaded,
                    checked,
                    history,
                    draft: None,
                    draft_checked: None,
                }
            })
            .collect();
        self.build_config_rows();
    }

    fn config_part(&self, kind: configs::Kind) -> Option<&ConfigPart> {
        self.config_parts.iter().find(|part| part.kind == kind)
    }

    /// The configuration ready to change: where it is and its text.
    fn config_ready(
        &self,
        kind: configs::Kind,
    ) -> Result<(configs::Located, configs::Loaded), String> {
        let part = self.config_part(kind).ok_or("it has not been read")?;
        let located = part.found.clone()?;
        let loaded = part.loaded.clone().ok_or("it has not been read")??;
        Ok((located, loaded))
    }

    fn selected_config_row(&self) -> Option<ConfigRow> {
        self.config_rows.get(self.main_state.selected()?).cloned()
    }

    /// The configuration the selected row belongs to.
    fn selected_config_kind(&self) -> Option<configs::Kind> {
        let at = self.main_state.selected()?;
        self.config_rows[..=at.min(self.config_rows.len().saturating_sub(1))]
            .iter()
            .rev()
            .find_map(|row| match row {
                ConfigRow::Field(kind, _) | ConfigRow::Edit(kind) | ConfigRow::Backup(kind, _) => {
                    Some(*kind)
                }
                ConfigRow::Heading(text) => configs::Kind::ALL
                    .into_iter()
                    .find(|kind| text.starts_with(kind.label())),
                ConfigRow::Text(_) => None,
            })
    }

    pub(in crate::tui::app) fn config_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                match self.selected_config_row() {
                    Some(ConfigRow::Field(kind, index)) => self.ask_config_field(kind, index),
                    Some(ConfigRow::Edit(kind)) => self.start_edit(kind),
                    Some(ConfigRow::Backup(kind, index)) => self.restore_config(kind, index),
                    _ => self.describe_config(),
                }
            }
            KeyCode::Char('E') => {
                if let Some(kind) = self.selected_config_kind() {
                    self.start_edit(kind);
                }
            }
            KeyCode::Char('d') => self.describe_config(),
            KeyCode::Char('c') => {
                if let Some(kind) = self.selected_config_kind() {
                    self.check_config(kind);
                }
            }
            KeyCode::Char('s') => {
                if let Some(kind) = self.selected_config_kind() {
                    self.save_draft(kind);
                }
            }
            KeyCode::Char('x') => {
                if let Some(kind) = self.selected_config_kind() {
                    self.discard_draft(kind);
                }
            }
            KeyCode::Char('v') | KeyCode::Char('r') => self.read_configs(),
            KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
            _ => {}
        }
    }

    pub(in crate::tui::app) fn config_hints(&self, list: &mut Vec<Hint>) {
        list.push(Hint::new(&["↑↓"], "Navigate"));
        match self.selected_config_row() {
            Some(ConfigRow::Field(..)) => list.push(Hint::new(&["Enter"], "Change")),
            Some(ConfigRow::Backup(..)) => list.push(Hint::new(&["Enter"], "Put back")),
            _ => {}
        }
        if let Some(kind) = self.selected_config_kind() {
            list.push(Hint::new(&["c"], "Check"));
            if self
                .config_part(kind)
                .is_some_and(|part| part.draft.is_some())
            {
                list.push(Hint::new(&["s"], "Save"));
                list.push(Hint::new(&["x"], "Discard changes"));
            }
            list.push(Hint::new(&["E"], "Edit in your editor"));
        }
        list.push(Hint::new(&["d"], "Details"));
        list.push(Hint::new(&["r"], "Read again"));
    }

    fn cannot_config(&mut self, why: String) {
        self.popup = Some(Popup::Text(TextPopup::new("Cannot change it", why, true)));
    }

    fn ask_config_field(&mut self, kind: configs::Kind, index: usize) {
        let Some(field) = kind.fields().get(index) else {
            return;
        };
        let (_, loaded) = match self.config_ready(kind) {
            Ok(ready) => ready,
            Err(why) => return self.cannot_config(why),
        };
        let text = self
            .config_part(kind)
            .and_then(|part| part.draft.clone())
            .unwrap_or(loaded.content);
        let now = configs::values(kind, &text)
            .into_iter()
            .find(|(key, _)| *key == field.key)
            .and_then(|(_, value)| value)
            .unwrap_or_default();
        self.popup = Some(Popup::Ask(
            InputPopup::new(
                format!("{}: {}", kind.label(), field.label),
                format!("{} ({}):", field.key, field.hint),
            )
            .with_value(now)
            .answering("Change"),
            Asked::ConfigField(kind, field.key),
        ));
    }

    /// Change a field in the draft; s saves the draft, after review.
    pub(in crate::tui::app) fn set_config(&mut self, kind: configs::Kind, key: &str, value: &str) {
        let (_, loaded) = match self.config_ready(kind) {
            Ok(ready) => ready,
            Err(why) => return self.cannot_config(why),
        };
        let text = self
            .config_part(kind)
            .and_then(|part| part.draft.clone())
            .unwrap_or_else(|| loaded.content.clone());
        match configs::set(kind, &text, key, value) {
            Ok(after) => self.set_draft(kind, &loaded, after),
            Err(why) => self.cannot_config(why),
        }
    }

    /// Keep text as the draft, or none when it is the file as it is.
    fn set_draft(&mut self, kind: configs::Kind, loaded: &configs::Loaded, after: String) {
        if let Some(part) = self.config_parts.iter_mut().find(|part| part.kind == kind) {
            part.draft = (after != loaded.content).then_some(after);
            part.draft_checked = None;
        }
        self.build_config_rows();
    }

    /// Check the draft, or the file when there is none, as the window's Check.
    fn check_config(&mut self, kind: configs::Kind) {
        let (_, loaded) = match self.config_ready(kind) {
            Ok(ready) => ready,
            Err(why) => return self.cannot_config(why),
        };
        let check = self.config_check();
        let folder = self.copies_folder();
        if let Some(part) = self.config_parts.iter_mut().find(|part| part.kind == kind) {
            let checked = configs::validate(
                kind,
                part.draft.as_deref().unwrap_or(&loaded.content),
                &check,
                folder.as_deref(),
            );
            if part.draft.is_some() {
                part.draft_checked = Some(checked);
            } else {
                part.checked = Some(checked);
            }
        }
        self.build_config_rows();
    }

    /// Review saving the draft.
    fn save_draft(&mut self, kind: configs::Kind) {
        let (located, loaded) = match self.config_ready(kind) {
            Ok(ready) => ready,
            Err(why) => return self.cannot_config(why),
        };
        let draft = self
            .config_part(kind)
            .and_then(|part| part.draft.clone())
            .unwrap_or_else(|| loaded.content.clone());
        self.save_config(&located, &loaded, &draft);
    }

    /// Go back to the file as it was read.
    fn discard_draft(&mut self, kind: configs::Kind) {
        if let Some(part) = self.config_parts.iter_mut().find(|part| part.kind == kind) {
            part.draft = None;
            part.draft_checked = None;
        }
        self.build_config_rows();
    }

    /// Check new text and review the plan that saves it.
    fn save_config(&mut self, located: &configs::Located, loaded: &configs::Loaded, after: &str) {
        let Some(folder) = self.copies_folder() else {
            return self.no_copies_folder();
        };
        let checked = configs::validate(located.kind, after, &self.config_check(), Some(&folder));
        if let Checked::Invalid(why) = &checked {
            return self.cannot_config(format!("Not saved: {why}."));
        }
        if loaded.content == after {
            self.popup = Some(Popup::Text(TextPopup::new(
                "Nothing changed",
                "The file is as it was; nothing is saved.",
                true,
            )));
            return;
        }
        match configs::plan_save(located, &loaded.revision, after, &checked, &folder) {
            Ok(planned) => self.confirm_plan(Ok(planned)),
            Err(why) => self.cannot_config(why),
        }
    }

    /// A private copy of the file for the user's editor, opened when the
    /// key has been handled.
    fn start_edit(&mut self, kind: configs::Kind) {
        let (located, loaded) = match self.config_ready(kind) {
            Ok(ready) => ready,
            Err(why) => return self.cannot_config(why),
        };
        let Some(folder) = self.copies_folder() else {
            return self.no_copies_folder();
        };
        // The draft, when there is one, is what the editor starts from.
        let text = self
            .config_part(kind)
            .and_then(|part| part.draft.clone())
            .unwrap_or_else(|| loaded.content.clone());
        let copy = match configs::private_copy(kind, "edit", &text, &folder) {
            Ok(copy) => copy,
            Err(why) => return self.cannot_config(why),
        };
        let editor = ["VISUAL", "EDITOR"]
            .iter()
            .filter_map(|name| std::env::var(name).ok())
            .find(|editor| !editor.trim().is_empty())
            .unwrap_or_else(|| "nano".to_string());
        self.edit_request = Some(EditRequest {
            copy,
            editor,
            located,
            loaded,
        });
    }

    /// The copy to open in the user's editor, once.
    pub fn take_edit(&mut self) -> Option<EditRequest> {
        self.edit_request.take()
    }

    /// The editor closed: check the copy, and review saving it.
    pub fn edited(&mut self, request: EditRequest, closed: Result<(), String>) {
        let after = std::fs::read_to_string(&request.copy);
        let _ = std::fs::remove_file(&request.copy);
        if let Err(why) = closed {
            return self.cannot_config(format!("{why}; nothing is saved."));
        }
        match after {
            Ok(after) => {
                // What came back is the draft, and its save is reviewed.
                self.set_draft(request.located.kind, &request.loaded, after.clone());
                self.save_config(&request.located, &request.loaded, &after)
            }
            Err(error) => self.cannot_config(format!(
                "The edited copy cannot be read: {error}; nothing is saved."
            )),
        }
    }

    /// Put a backup back, after review: the file there now is kept too.
    fn restore_config(&mut self, kind: configs::Kind, index: usize) {
        let Some(backup) = self
            .config_part(kind)
            .and_then(|part| part.history.get(index))
            .cloned()
        else {
            return;
        };
        self.confirm_plan(Ok(backups::plan_restore(&backup)));
    }

    fn describe_config(&mut self) {
        let Some(kind) = self.selected_config_kind() else {
            return;
        };
        let Some(part) = self.config_part(kind) else {
            return;
        };
        let text = match (&part.found, &part.loaded) {
            (Err(why), _) => format!("{why}."),
            (Ok(located), loaded) => {
                let mut text = format!(
                    "{}, named by {}.\n\n{} reads {}{}.",
                    helix_toolbox_core::printable(&located.path.display().to_string()),
                    located.origin.label(),
                    kind.label(),
                    helix_toolbox_core::printable(&located.live.display().to_string()),
                    if located.managed {
                        ", a Home Manager link into the Nix store"
                    } else {
                        ""
                    }
                );
                if let Some(Ok(loaded)) = loaded {
                    text.push_str(&format!(
                        "\n\n{} bytes; SHA-256 {}.",
                        loaded.content.len(),
                        loaded.revision
                    ));
                }
                if let Some(flake) = &located.flake {
                    text.push_str(&format!(
                        "\n\nIt is part of the Home Manager flake in {}: run Home Manager after a change to make it live.",
                        helix_toolbox_core::printable(&flake.display().to_string())
                    ));
                }
                text.push_str("\n\nA save keeps the file there now as a backup beside it, which Backups and helix-toolbox restore can put back.");
                text
            }
        };
        self.popup = Some(Popup::Text(TextPopup::new(
            format!("{}'s configuration", kind.label()),
            text,
            true,
        )));
    }

    fn build_config_rows(&mut self) {
        let mut rows = Vec::new();
        for part in &self.config_parts {
            let kind = part.kind;
            let located = match &part.found {
                Ok(located) => located,
                Err(why) => {
                    rows.push(ConfigRow::Heading(kind.label().to_string()));
                    rows.push(ConfigRow::Text(format!("{why}.")));
                    continue;
                }
            };
            rows.push(ConfigRow::Heading(format!(
                "{}  {}",
                kind.label(),
                located.path.display()
            )));
            let mut about = format!("named by {}", located.origin.label());
            if located.managed {
                about.push_str("; the live file is a Home Manager link");
            }
            if located.flake.is_some() {
                about.push_str("; run Home Manager after a change");
            }
            rows.push(ConfigRow::Text(about));
            match &part.loaded {
                Some(Err(why)) => {
                    rows.push(ConfigRow::Text(format!("{why}.")));
                    continue;
                }
                Some(Ok(loaded)) if !loaded.exists => {
                    rows.push(ConfigRow::Text(
                        "not there yet; a change makes it".to_string(),
                    ));
                }
                _ => {}
            }
            if let Some(checked) = &part.checked {
                rows.push(ConfigRow::Text(match checked {
                    Checked::Valid => "Checks: valid".to_string(),
                    Checked::Unchecked(why) | Checked::Invalid(why) => {
                        format!("Checks: {}: {why}", checked.label())
                    }
                }));
            }
            if part.draft.is_some() {
                rows.push(ConfigRow::Text(
                    "Draft: changed, not saved yet. c checks it, s saves it after review, x discards it."
                        .to_string(),
                ));
                rows.push(ConfigRow::Text(match &part.draft_checked {
                    None => "Draft checks: not checked yet".to_string(),
                    Some(Checked::Valid) => "Draft checks: valid".to_string(),
                    Some(checked @ (Checked::Unchecked(why) | Checked::Invalid(why))) => {
                        format!("Draft checks: {}: {why}", checked.label())
                    }
                }));
            }
            rows.extend((0..kind.fields().len()).map(|index| ConfigRow::Field(kind, index)));
            rows.push(ConfigRow::Edit(kind));
            if !part.history.is_empty() {
                rows.push(ConfigRow::Text(format!(
                    "backups kept ({}), newest first:",
                    part.history.len()
                )));
                rows.extend((0..part.history.len()).map(|index| ConfigRow::Backup(kind, index)));
            }
        }
        self.config_rows = rows;
        self.clamp_main();
    }

    pub(in crate::tui::app) fn draw_configs(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Configs",
            "Ghostty's and Fastfetch's configuration. Each change is checked and shown before it is saved, and the file there now is kept as a backup.",
        );
        self.fill(frame, inner);
        let focused = self.focus == Focus::Main;
        let muted = Style::default().fg(self.theme.muted());
        let mut items: Vec<Line> = Vec::new();
        for row in &self.config_rows {
            items.push(match row {
                ConfigRow::Heading(text) => Line::styled(
                    helix_toolbox_core::printable(text),
                    Style::default().fg(self.theme.group()).bold(),
                ),
                ConfigRow::Text(text) => {
                    let color = if text.starts_with("Checks: Invalid")
                        || text.starts_with("Draft checks: Invalid")
                    {
                        self.theme.failure()
                    } else {
                        self.theme.muted()
                    };
                    Line::styled(
                        format!("  {}", helix_toolbox_core::printable(text)),
                        Style::default().fg(color),
                    )
                }
                ConfigRow::Field(kind, index) => {
                    let field = &kind.fields()[*index];
                    // The draft's value, when there is a draft.
                    let value = self.config_part(*kind).and_then(|part| {
                        let text = match &part.draft {
                            Some(draft) => draft.as_str(),
                            None => part.loaded.as_ref()?.as_ref().ok()?.content.as_str(),
                        };
                        configs::values(*kind, text)
                            .into_iter()
                            .find(|(key, _)| *key == field.key)?
                            .1
                    });
                    Line::from(vec![
                        Span::styled(
                            format!("  {:<26}", field.label),
                            Style::default().fg(self.theme.entry()),
                        ),
                        Span::styled(
                            value
                                .as_deref()
                                .map(helix_toolbox_core::printable)
                                .unwrap_or_else(|| "(not set)".to_string()),
                            muted,
                        ),
                    ])
                }
                ConfigRow::Edit(_) => Line::styled(
                    "  Edit the whole file in your editor…".to_string(),
                    Style::default().fg(self.theme.entry()),
                ),
                ConfigRow::Backup(kind, index) => {
                    let backup = self
                        .config_part(*kind)
                        .and_then(|part| part.history.get(*index));
                    Line::from(vec![
                        Span::styled(
                            format!(
                                "    {}  ",
                                backup
                                    .and_then(|backup| backup.taken.clone())
                                    .unwrap_or_default()
                            ),
                            Style::default().fg(self.theme.entry()),
                        ),
                        Span::styled(
                            backup
                                .map(|backup| {
                                    helix_toolbox_core::printable(
                                        &backup.backup.display().to_string(),
                                    )
                                })
                                .unwrap_or_default(),
                            muted,
                        ),
                    ])
                }
            });
        }
        let list = List::new(self.measure(items))
            .block(self.card("Configs"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, inner, &mut self.main_state, &self.hits);
    }
}
