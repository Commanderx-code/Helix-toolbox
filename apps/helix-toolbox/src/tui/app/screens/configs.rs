//! The Configs screen.

use super::super::*;
use super::{Ask, Cx, ToolScreen, View};

/// Ghostty's and Fastfetch's configuration, changed after review.
#[derive(Default)]
pub(in crate::tui::app) struct Configs {
    parts: Vec<ConfigPart>,
    pub(in crate::tui::app) rows: Vec<ConfigRow>,
    /// A copy waiting for the user's editor.
    edit: Option<EditRequest>,
}

/// A line of the Configs screen.
#[derive(Clone, Debug)]
pub(in crate::tui::app) enum ConfigRow {
    Heading(String),
    Text(String),
    /// A field of a configuration, by its place in the configuration's fields.
    Field(configs::Kind, usize),
    /// The whole file, in the user's editor.
    Edit(configs::Kind),
    /// A backup kept beside the file, by its place in its history.
    Backup(configs::Kind, usize),
}

/// A configuration as read: the file to edit, its text, what its checks
/// say and the backups kept beside it.
struct ConfigPart {
    kind: configs::Kind,
    found: Result<configs::Located, String>,
    loaded: Option<Result<configs::Loaded, String>>,
    checked: Option<Checked>,
    history: Vec<backups::Backup>,
    /// Changes not saved yet, as the window's draft: fields changed one by
    /// one, then checked and saved once.
    draft: Option<String>,
    /// What the checks said of the draft, once asked.
    draft_checked: Option<Checked>,
}

/// A private copy of a configuration for the user's editor, which the
/// event loop opens with the terminal handed back.
pub struct EditRequest {
    pub copy: PathBuf,
    pub editor: String,
    located: configs::Located,
    loaded: configs::Loaded,
}

/// The private folder for copies, beside the host's history; never a
/// shared one.
fn copies_folder(host: &Host) -> Option<PathBuf> {
    host.state().map(|state| state.join("configs"))
}

/// Ghostty's own check, which runs this machine's Ghostty; none on a
/// made-up host.
fn check_on(host: &Host) -> fn(&Path) -> Option<(bool, String)> {
    if host.is_here() {
        configs::ghostty_here
    } else {
        |_| None
    }
}

fn cannot(cx: &mut Cx, why: String) {
    *cx.popup = Some(Popup::Text(TextPopup::new("Cannot change it", why, true)));
}

fn no_copies_folder(cx: &mut Cx) {
    cannot(
        cx,
        "There is no state folder for Helix Toolbox's private copies; nothing is saved.".into(),
    );
}

impl Configs {
    /// Read both configurations, their checks and their backups.
    fn read(&mut self, cx: &mut Cx) {
        let check = check_on(cx.host);
        self.parts = configs::Kind::ALL
            .into_iter()
            .map(|kind| {
                let found = configs::locate_on(cx.host, kind);
                let (loaded, checked, history) = match &found {
                    Ok(located) => {
                        let loaded = configs::read(located);
                        let checked = loaded.as_ref().ok().map(|loaded| {
                            configs::validate(
                                kind,
                                &loaded.content,
                                &check,
                                copies_folder(cx.host).as_deref(),
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
        self.build_rows(cx);
    }

    fn part(&self, kind: configs::Kind) -> Option<&ConfigPart> {
        self.parts.iter().find(|part| part.kind == kind)
    }

    /// The configuration ready to change: where it is and its text.
    fn ready(&self, kind: configs::Kind) -> Result<(configs::Located, configs::Loaded), String> {
        let part = self.part(kind).ok_or("it has not been read")?;
        let located = part.found.clone()?;
        let loaded = part.loaded.clone().ok_or("it has not been read")??;
        Ok((located, loaded))
    }

    fn selected(&self, at: Option<usize>) -> Option<ConfigRow> {
        self.rows.get(at?).cloned()
    }

    /// The configuration the selected row belongs to.
    fn selected_kind(&self, at: Option<usize>) -> Option<configs::Kind> {
        let at = at?;
        self.rows[..=at.min(self.rows.len().saturating_sub(1))]
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

    fn ask_field(&self, cx: &mut Cx, kind: configs::Kind, index: usize) {
        let Some(field) = kind.fields().get(index) else {
            return;
        };
        let (_, loaded) = match self.ready(kind) {
            Ok(ready) => ready,
            Err(why) => return cannot(cx, why),
        };
        let text = self
            .part(kind)
            .and_then(|part| part.draft.clone())
            .unwrap_or(loaded.content);
        let now = configs::values(kind, &text)
            .into_iter()
            .find(|(key, _)| *key == field.key)
            .and_then(|(_, value)| value)
            .unwrap_or_default();
        *cx.popup = Some(Popup::Ask(
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
    pub(in crate::tui::app) fn set_config(
        &mut self,
        cx: &mut Cx,
        kind: configs::Kind,
        key: &str,
        value: &str,
    ) {
        let (_, loaded) = match self.ready(kind) {
            Ok(ready) => ready,
            Err(why) => return cannot(cx, why),
        };
        let text = self
            .part(kind)
            .and_then(|part| part.draft.clone())
            .unwrap_or_else(|| loaded.content.clone());
        match configs::set(kind, &text, key, value) {
            Ok(after) => self.set_draft(cx, kind, &loaded, after),
            Err(why) => cannot(cx, why),
        }
    }

    /// Keep text as the draft, or none when it is the file as it is.
    fn set_draft(
        &mut self,
        cx: &mut Cx,
        kind: configs::Kind,
        loaded: &configs::Loaded,
        after: String,
    ) {
        if let Some(part) = self.parts.iter_mut().find(|part| part.kind == kind) {
            part.draft = (after != loaded.content).then_some(after);
            part.draft_checked = None;
        }
        self.build_rows(cx);
    }

    /// Check the draft, or the file when there is none, as the window's Check.
    fn check(&mut self, cx: &mut Cx, kind: configs::Kind) {
        let (_, loaded) = match self.ready(kind) {
            Ok(ready) => ready,
            Err(why) => return cannot(cx, why),
        };
        let check = check_on(cx.host);
        let folder = copies_folder(cx.host);
        if let Some(part) = self.parts.iter_mut().find(|part| part.kind == kind) {
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
        self.build_rows(cx);
    }

    /// Review saving the draft.
    fn save_draft(&self, cx: &mut Cx, kind: configs::Kind) {
        let (located, loaded) = match self.ready(kind) {
            Ok(ready) => ready,
            Err(why) => return cannot(cx, why),
        };
        let draft = self
            .part(kind)
            .and_then(|part| part.draft.clone())
            .unwrap_or_else(|| loaded.content.clone());
        save(cx, &located, &loaded, &draft);
    }

    /// Go back to the file as it was read.
    fn discard_draft(&mut self, cx: &mut Cx, kind: configs::Kind) {
        if let Some(part) = self.parts.iter_mut().find(|part| part.kind == kind) {
            part.draft = None;
            part.draft_checked = None;
        }
        self.build_rows(cx);
    }

    /// A private copy of the file for the user's editor, opened when the
    /// key has been handled.
    fn start_edit(&mut self, cx: &mut Cx, kind: configs::Kind) {
        let (located, loaded) = match self.ready(kind) {
            Ok(ready) => ready,
            Err(why) => return cannot(cx, why),
        };
        let Some(folder) = copies_folder(cx.host) else {
            return no_copies_folder(cx);
        };
        // The draft, when there is one, is what the editor starts from.
        let text = self
            .part(kind)
            .and_then(|part| part.draft.clone())
            .unwrap_or_else(|| loaded.content.clone());
        let copy = match configs::private_copy(kind, "edit", &text, &folder) {
            Ok(copy) => copy,
            Err(why) => return cannot(cx, why),
        };
        let editor = ["VISUAL", "EDITOR"]
            .iter()
            .filter_map(|name| std::env::var(name).ok())
            .find(|editor| !editor.trim().is_empty())
            .unwrap_or_else(|| "nano".to_string());
        self.edit = Some(EditRequest {
            copy,
            editor,
            located,
            loaded,
        });
    }

    /// The copy to open in the user's editor, once.
    pub(in crate::tui::app) fn take_edit(&mut self) -> Option<EditRequest> {
        self.edit.take()
    }

    /// The editor closed: check the copy, and review saving it.
    pub(in crate::tui::app) fn edited(
        &mut self,
        cx: &mut Cx,
        request: EditRequest,
        closed: Result<(), String>,
    ) {
        let after = std::fs::read_to_string(&request.copy);
        let _ = std::fs::remove_file(&request.copy);
        if let Err(why) = closed {
            return cannot(cx, format!("{why}; nothing is saved."));
        }
        match after {
            Ok(after) => {
                // What came back is the draft, and its save is reviewed.
                self.set_draft(cx, request.located.kind, &request.loaded, after.clone());
                save(cx, &request.located, &request.loaded, &after)
            }
            Err(error) => cannot(
                cx,
                format!("The edited copy cannot be read: {error}; nothing is saved."),
            ),
        }
    }

    /// Put a backup back, after review: the file there now is kept too.
    fn restore(&self, cx: &mut Cx, kind: configs::Kind, index: usize) {
        let Some(backup) = self
            .part(kind)
            .and_then(|part| part.history.get(index))
            .cloned()
        else {
            return;
        };
        cx.confirm(backups::plan_restore(&backup));
    }

    fn describe(&self, cx: &mut Cx) {
        let Some(kind) = self.selected_kind(cx.at()) else {
            return;
        };
        let Some(part) = self.part(kind) else {
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
        *cx.popup = Some(Popup::Text(TextPopup::new(
            format!("{}'s configuration", kind.label()),
            text,
            true,
        )));
    }

    fn build_rows(&mut self, cx: &mut Cx) {
        let mut rows = Vec::new();
        for part in &self.parts {
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
        self.rows = rows;
        cx.clamp();
    }
}

/// Check new text and review the plan that saves it.
fn save(cx: &mut Cx, located: &configs::Located, loaded: &configs::Loaded, after: &str) {
    let Some(folder) = copies_folder(cx.host) else {
        return no_copies_folder(cx);
    };
    let checked = configs::validate(located.kind, after, &check_on(cx.host), Some(&folder));
    if let Checked::Invalid(why) = &checked {
        return cannot(cx, format!("Not saved: {why}."));
    }
    if loaded.content == after {
        *cx.popup = Some(Popup::Text(TextPopup::new(
            "Nothing changed",
            "The file is as it was; nothing is saved.",
            true,
        )));
        return;
    }
    match configs::plan_save(located, &loaded.revision, after, &checked, &folder) {
        Ok(planned) => cx.confirm(planned),
        Err(why) => cannot(cx, why),
    }
}

impl ToolScreen for Configs {
    fn open(&mut self, cx: &mut Cx) {
        self.read(cx);
    }

    fn len(&self, _view: &View) -> usize {
        self.rows.len()
    }

    fn key(&mut self, cx: &mut Cx, code: KeyCode) {
        let kind = self.selected_kind(cx.at());
        match code {
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => match self.selected(cx.at()) {
                Some(ConfigRow::Field(kind, index)) => self.ask_field(cx, kind, index),
                Some(ConfigRow::Edit(kind)) => self.start_edit(cx, kind),
                Some(ConfigRow::Backup(kind, index)) => self.restore(cx, kind, index),
                _ => self.describe(cx),
            },
            KeyCode::Char('E') => {
                if let Some(kind) = kind {
                    self.start_edit(cx, kind);
                }
            }
            KeyCode::Char('d') => self.describe(cx),
            KeyCode::Char('c') => {
                if let Some(kind) = kind {
                    self.check(cx, kind);
                }
            }
            KeyCode::Char('s') => {
                if let Some(kind) = kind {
                    self.save_draft(cx, kind);
                }
            }
            KeyCode::Char('x') => {
                if let Some(kind) = kind {
                    self.discard_draft(cx, kind);
                }
            }
            KeyCode::Char('v') | KeyCode::Char('r') => self.read(cx),
            KeyCode::Char('h') | KeyCode::Left => cx.ask(Ask::Sidebar),
            _ => {}
        }
    }

    fn hints(&self, _view: &View, at: Option<usize>, list: &mut Vec<Hint>) {
        list.push(Hint::new(&["↑↓"], "Navigate"));
        match self.selected(at) {
            Some(ConfigRow::Field(..)) => list.push(Hint::new(&["Enter"], "Change")),
            Some(ConfigRow::Backup(..)) => list.push(Hint::new(&["Enter"], "Put back")),
            _ => {}
        }
        if let Some(kind) = self.selected_kind(at) {
            list.push(Hint::new(&["c"], "Check"));
            if self.part(kind).is_some_and(|part| part.draft.is_some()) {
                list.push(Hint::new(&["s"], "Save"));
                list.push(Hint::new(&["x"], "Discard changes"));
            }
            list.push(Hint::new(&["E"], "Edit in your editor"));
        }
        list.push(Hint::new(&["d"], "Details"));
        list.push(Hint::new(&["r"], "Read again"));
    }

    fn after_run(&mut self, cx: &mut Cx, reading: Reading) {
        if reading == Reading::Tools {
            self.read(cx);
        }
    }

    fn draw(&self, view: &View, frame: &mut Frame, area: Rect, state: &mut ListState) {
        let inner = view.page(
            frame,
            area,
            "Configs",
            "Ghostty's and Fastfetch's configuration. Each change is checked and shown before it is saved, and the file there now is kept as a backup.",
        );
        view.fill(frame, inner);
        let focused = view.focused;
        let muted = Style::default().fg(view.theme.muted());
        let mut items: Vec<Line> = Vec::new();
        for row in &self.rows {
            items.push(match row {
                ConfigRow::Heading(text) => Line::styled(
                    helix_toolbox_core::printable(text),
                    Style::default().fg(view.theme.group()).bold(),
                ),
                ConfigRow::Text(text) => {
                    let color = if text.starts_with("Checks: Invalid")
                        || text.starts_with("Draft checks: Invalid")
                    {
                        view.theme.failure()
                    } else {
                        view.theme.muted()
                    };
                    Line::styled(
                        format!("  {}", helix_toolbox_core::printable(text)),
                        Style::default().fg(color),
                    )
                }
                ConfigRow::Field(kind, index) => {
                    let field = &kind.fields()[*index];
                    // The draft's value, when there is a draft.
                    let value = self.part(*kind).and_then(|part| {
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
                            Style::default().fg(view.theme.entry()),
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
                    Style::default().fg(view.theme.entry()),
                ),
                ConfigRow::Backup(kind, index) => {
                    let backup = self.part(*kind).and_then(|part| part.history.get(*index));
                    Line::from(vec![
                        Span::styled(
                            format!(
                                "    {}  ",
                                backup
                                    .and_then(|backup| backup.taken.clone())
                                    .unwrap_or_default()
                            ),
                            Style::default().fg(view.theme.entry()),
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
        let list = List::new(view.measure(items))
            .block(view.card("Configs"))
            .highlight_style(view.highlight(focused))
            .highlight_symbol(view.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, inner, state, view.hits);
    }
}
