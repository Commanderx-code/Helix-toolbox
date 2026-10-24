//! The Migration screen.

use super::super::*;
use super::{Ask, Cx, Read, ToolScreen, View};

/// What this machine has, to take elsewhere, and a migration brought here.
pub(in crate::tui::app) struct Migration {
    /// What a migration of this machine would hold, as last read.
    pub(in crate::tui::app) here: Option<migration::Migration>,
    reading: Read<(migration::Migration, bool)>,
    /// Whether the packages installed by hand can be told apart here.
    pub(in crate::tui::app) packages_known: bool,
    /// The migration file opened, and what it means here.
    pub(in crate::tui::app) opened: Option<(PathBuf, Translation)>,
    opening: Read<MigrationOpened>,
    /// Why the migration file asked for could not be opened, answered after
    /// the screen was left.
    unopened: Option<String>,
    /// A run while another screen was shown left what it read out of
    /// date.
    stale: bool,
    pub(in crate::tui::app) rows: Vec<MigrationRow>,
}

impl Default for Migration {
    fn default() -> Self {
        Self {
            here: None,
            reading: Read::default(),
            packages_known: true,
            opened: None,
            opening: Read::default(),
            unopened: None,
            stale: false,
            rows: Vec::new(),
        }
    }
}

/// A migration file read here, or why it could not be.
type MigrationOpened = (PathBuf, Result<Translation, String>);

/// A line of the Migration screen.
#[derive(Clone, Debug)]
pub(in crate::tui::app) enum MigrationRow {
    Heading(String),
    Text(String),
    /// A thing the opened migration names, by its place in the translation.
    Item(usize),
}

/// How the Migration screen names a kind of thing.
fn migration_kind(kind: migration::Kind) -> &'static str {
    match kind {
        migration::Kind::Entry => "entry",
        migration::Kind::Flatpak => "Flatpak",
        migration::Kind::Package => "package",
    }
}

impl Migration {
    /// Read in the background what a migration of this machine would hold.
    fn read(&mut self, cx: &mut Cx) {
        let entries: Vec<Entry> = cx.catalog.entries().into_iter().cloned().collect();
        let platform = cx.platform.clone();
        self.reading.start(cx.host, move |host| {
            let refs: Vec<&Entry> = entries.iter().collect();
            // As migration::gather, keeping whether the packages chosen by
            // hand could be read.
            let inventory = Inventory::gather(&host, &platform);
            let chosen = migration::chosen(&host, platform.manager);
            let found =
                migration::export(&refs, &platform, &inventory, chosen.as_deref(), host.now());
            (found, chosen.is_some())
        });
        self.build_rows(cx);
    }

    /// Read this machine again, and the opened migration with it.
    fn reload(&mut self, cx: &mut Cx) {
        self.read(cx);
        if let Some((path, _)) = self.opened.clone() {
            self.open_file(cx, path);
        }
    }

    /// Read a migration file and what it means here, in the background.
    pub(in crate::tui::app) fn open_file(&mut self, cx: &mut Cx, path: PathBuf) {
        let entries: Vec<Entry> = cx.catalog.entries().into_iter().cloned().collect();
        let platform = cx.platform.clone();
        self.opening.start(cx.host, move |host| {
            let refs: Vec<&Entry> = entries.iter().collect();
            let read = migration::read_here(&host, &path, &refs, &platform)
                .map_err(|error| error.to_string());
            (path, read)
        });
        self.build_rows(cx);
    }

    /// The migration file read, or why it could not be.
    fn file_read(&mut self, cx: &mut Cx, path: PathBuf, read: Result<Translation, String>) {
        match read {
            Ok(translation) => {
                self.opened = Some((path, translation));
                self.build_rows(cx);
            }
            Err(error) => self.not_opened(cx, error),
        }
    }

    /// The migration file could not be opened. The popup is the screen
    /// shown: when it was left, why waits until it is shown again.
    fn not_opened(&mut self, cx: &mut Cx, error: String) {
        self.build_rows(cx);
        if cx.screen == Screen::Migration {
            Self::cannot_open(cx, error);
        } else {
            self.unopened = Some(error);
        }
    }

    fn cannot_open(cx: &mut Cx, error: String) {
        *cx.popup = Some(Popup::Text(TextPopup::new(
            "Cannot open the migration",
            error,
            true,
        )));
    }

    /// Write what this machine has to a file, as `helix-toolbox migrate
    /// export` does.
    pub(in crate::tui::app) fn export(&self, cx: &Cx, text: &str) -> Result<String, String> {
        let path = typed_path(text)?;
        if path.extension().is_none_or(|extension| extension != "toml") {
            return Err("A migration is written to a .toml file.".to_string());
        }
        let found = match &self.here {
            Some(found) => found.clone(),
            None => migration::gather(cx.host, &cx.catalog.entries(), cx.platform, cx.host.now()),
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
    fn apply(&self, cx: &mut Cx) {
        let Some((_, translation)) = &self.opened else {
            *cx.popup = Some(Popup::Text(TextPopup::new(
                "No migration open",
                "o opens a migration file made on the other machine; then a installs what it can here.",
                true,
            )));
            return;
        };
        if translation.count(Standing::Install) == 0 {
            *cx.popup = Some(Popup::Text(TextPopup::new(
                "Nothing to install",
                "Everything the migration names is here already, cannot be installed here, or is left to you.",
                true,
            )));
            return;
        }
        let entries = cx.catalog.entries();
        let planned = migration::plan_here(cx.host, translation, &entries, cx.platform);
        cx.ask(Ask::Plan(planned));
    }

    fn selected(&self, at: Option<usize>) -> Option<&migration::Item> {
        let MigrationRow::Item(index) = self.rows.get(at?)? else {
            return None;
        };
        self.opened.as_ref()?.1.items.get(*index)
    }

    fn describe(&self, cx: &mut Cx) {
        let Some(item) = self.selected(cx.at()) else {
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
        *cx.popup = Some(Popup::Text(TextPopup::new(
            helix_toolbox_core::printable(&item.name),
            text,
            true,
        )));
    }

    fn build_rows(&mut self, cx: &mut Cx) {
        let mut rows = vec![MigrationRow::Heading("This machine".into())];
        rows.push(MigrationRow::Text(match &self.here {
            Some(found) => format!(
                "{} entries, {} Flatpaks and {} packages of yours, from {}",
                found.entries.len(),
                found.flatpaks.len(),
                found.packages.len(),
                found.from
            ),
            None if self.reading.busy() => "Reading what this machine has…".into(),
            None if self.reading.failed() => {
                "Could not read what this machine has. r reads again.".into()
            }
            None => "Press r to read what this machine has.".into(),
        }));
        if self.here.is_some() && !self.packages_known {
            rows.push(MigrationRow::Text(
                "The packages you installed yourself cannot be told apart here, so only the entries and Flatpaks go in the file.".into(),
            ));
        }
        rows.push(MigrationRow::Text(
            "e writes it to a file to take to another machine; o opens one made elsewhere.".into(),
        ));
        if self.opening.busy() {
            rows.push(MigrationRow::Heading("Opening the migration…".into()));
        }
        if let Some((path, translation)) = &self.opened {
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
        self.rows = rows;
        cx.clamp();
    }
}

impl ToolScreen for Migration {
    fn open(&mut self, cx: &mut Cx) {
        if std::mem::take(&mut self.stale) {
            self.reload(cx);
        } else if self.here.is_none() {
            self.read(cx);
        } else {
            self.build_rows(cx);
        }
        if let Some(error) = self.unopened.take() {
            Self::cannot_open(cx, error);
        }
    }

    fn len(&self, _view: &View) -> usize {
        self.rows.len()
    }

    fn key(&mut self, cx: &mut Cx, code: KeyCode) {
        match code {
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => match self.selected(cx.at()) {
                Some(item) if item.standing == Standing::Install => self.apply(cx),
                Some(_) => self.describe(cx),
                None => {}
            },
            KeyCode::Char('d') => self.describe(cx),
            KeyCode::Char('e') => {
                let name = self
                    .here
                    .as_ref()
                    .map(migration::file_name)
                    .unwrap_or_else(|| "helix-toolbox-migration.toml".to_string());
                *cx.popup = Some(Popup::Ask(
                    InputPopup::new(
                        "Export this machine",
                        "The file to write its entries, Flatpaks and packages to, to take to the other machine:",
                    )
                    .with_value(format!("~/{name}"))
                    .path()
                    .answering("Export"),
                    Asked::ExportMigration,
                ));
            }
            KeyCode::Char('o') => {
                let mut ask = InputPopup::new(
                    "Open a migration",
                    "The migration file made on the other machine, such as ~/helix-toolbox-migration.toml:",
                )
                .path()
                .answering("Open");
                if let Some((path, _)) = &self.opened {
                    ask = ask.with_value(path.display().to_string());
                }
                *cx.popup = Some(Popup::Ask(ask, Asked::OpenMigration));
            }
            KeyCode::Char('a') => self.apply(cx),
            KeyCode::Char('r') => self.reload(cx),
            KeyCode::Char('h') | KeyCode::Left => cx.ask(Ask::Sidebar),
            _ => {}
        }
    }

    fn hints(&self, _view: &View, at: Option<usize>, list: &mut Vec<Hint>) {
        list.push(Hint::new(&["↑↓"], "Navigate"));
        if self.selected(at).is_some() {
            list.push(Hint::new(&["d"], "Details"));
        }
        list.push(Hint::new(&["e"], "Export"));
        list.push(Hint::new(&["o"], "Open a file"));
        if self
            .opened
            .as_ref()
            .is_some_and(|(_, translation)| translation.count(Standing::Install) > 0)
        {
            list.push(Hint::new(&["a"], "Install"));
        }
        list.push(Hint::new(&["r"], "Read again"));
    }

    fn poll(&mut self, cx: &mut Cx) -> bool {
        if let Some(answer) = self.reading.take() {
            if let Ok((found, known)) = answer {
                self.here = Some(found);
                self.packages_known = known;
            }
            self.build_rows(cx);
            return true;
        }
        if let Some(answer) = self.opening.take() {
            match answer {
                Ok((path, read)) => self.file_read(cx, path, read),
                Err(_) => self.not_opened(cx, "Reading the file stopped without an answer.".into()),
            }
            return true;
        }
        false
    }

    fn after_run(&mut self, cx: &mut Cx, reading: Reading) {
        if reading == Reading::Tools {
            self.reload(cx);
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
            "Migration",
            "Take this machine's applications to another machine or distribution. Exporting only reads.",
        );
        view.fill(frame, inner);
        let focused = view.focused;
        let items_of = self
            .opened
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
        for row in &self.rows {
            items.push(match row {
                MigrationRow::Heading(text) => {
                    Line::styled(text.clone(), Style::default().fg(view.theme.group()).bold())
                }
                MigrationRow::Text(text) => {
                    Line::styled(format!("  {text}"), Style::default().fg(view.theme.muted()))
                }
                MigrationRow::Item(index) => {
                    let item = &items_of[*index];
                    let color = match item.standing {
                        Standing::Install => view.theme.entry(),
                        Standing::Here => view.theme.success(),
                        Standing::Unavailable => view.theme.failure(),
                        Standing::Yours => view.theme.warning(),
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
                            Style::default().fg(view.theme.muted()),
                        ),
                        Span::styled(
                            helix_toolbox_core::printable(&item.detail),
                            Style::default().fg(view.theme.muted()),
                        ),
                    ])
                }
            });
        }
        let list = List::new(view.measure(items))
            .block(view.card("Migration"))
            .highlight_style(view.highlight(focused))
            .highlight_symbol(view.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, inner, state, view.hits);
    }
}
