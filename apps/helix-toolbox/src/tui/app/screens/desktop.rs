//! The Desktop screen.

use super::super::*;
use super::{Ask, Cx, ToolScreen, View};
use helix_toolbox_engine::desktop;

/// The desktop environment, its look and the entries for it.
#[derive(Default)]
pub(in crate::tui::app) struct Desktop {
    pub(in crate::tui::app) rows: Vec<DesktopRow>,
    /// Where looks are saved; the user's configuration unless a test says.
    pub(in crate::tui::app) looks_dir: Option<PathBuf>,
}

/// A line of the Desktop screen.
#[derive(Clone, Debug)]
pub(in crate::tui::app) enum DesktopRow {
    Heading(String),
    Info {
        text: String,
        details: String,
    },
    /// A suggested entry, with what is worth knowing about it here.
    Entry {
        id: String,
        text: String,
        note: Option<String>,
    },
    Look {
        slug: String,
        text: String,
    },
}

impl Desktop {
    fn selected(&self, at: Option<usize>) -> Option<&DesktopRow> {
        self.rows.get(at?)
    }

    fn looks_directory(&self) -> Option<PathBuf> {
        self.looks_dir.clone().or_else(desktop::dir)
    }

    pub(in crate::tui::app) fn reload(&mut self, cx: &mut Cx) {
        let found = desktop::detect_on(cx.host);
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
            .map(|directory| desktop::load_all(&directory).0)
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
        let offered = helix_toolbox_engine::setup::load(cx.catalog.root())
            .map(|setup| setup.for_desktop(found.kind))
            .unwrap_or_default();
        let offered: Vec<_> =
            helix_toolbox_engine::setup::suggestions(&offered, cx.catalog, cx.platform)
                .into_iter()
                .filter(|suggestion| suggestion.available)
                .collect();
        if !offered.is_empty() {
            rows.push(DesktopRow::Heading("For this desktop".to_string()));
        }
        for suggestion in offered {
            let queued = cx.queue.iter().any(|entry| entry.id == suggestion.id);
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
        self.rows = rows;
        cx.clamp();
    }

    /// Save the desktop's look under `name`.
    pub(in crate::tui::app) fn save_look_as(&mut self, cx: &mut Cx, name: &str) {
        let found = desktop::detect_on(cx.host);
        let saved = desktop::capture(name, &found)
            .ok_or_else(|| {
                "no desktop whose appearance Helix Toolbox can read is running here".to_string()
            })
            .and_then(|look| {
                let directory = self
                    .looks_directory()
                    .ok_or_else(|| "cannot find the configuration folder".to_string())?;
                desktop::save(&directory, &look)
                    .map(|path| (look, path))
                    .map_err(|error| error.to_string())
            });
        *cx.popup = Some(Popup::Text(match saved {
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
        self.reload(cx);
    }

    fn apply_look(&self, cx: &mut Cx, slug: &str) {
        let Some(directory) = self.looks_directory() else {
            return;
        };
        let Some((_, look)) = desktop::load_all(&directory)
            .0
            .into_iter()
            .find(|(known, _)| known == slug)
        else {
            return;
        };
        let found = desktop::detect_on(cx.host);
        match desktop::plan_apply(&look, &found) {
            Ok(planned) => cx.confirm(planned),
            Err(why) => {
                *cx.popup = Some(Popup::Text(TextPopup::new(
                    "Cannot apply this look",
                    why,
                    true,
                )));
            }
        }
    }
}

impl ToolScreen for Desktop {
    fn open(&mut self, cx: &mut Cx) {
        self.reload(cx);
    }

    fn len(&self, _view: &View) -> usize {
        self.rows.len()
    }

    fn key(&mut self, cx: &mut Cx, code: KeyCode) {
        match code {
            KeyCode::Enter | KeyCode::Char('d') => match self.selected(cx.at()).cloned() {
                Some(DesktopRow::Info { text, details }) => {
                    *cx.popup = Some(Popup::Text(TextPopup::new(text, details, true)));
                }
                Some(DesktopRow::Entry { id, .. }) => {
                    if let Some(entry) = cx.catalog.find(&id).cloned() {
                        cx.ask(Ask::Run(vec![entry]));
                    }
                }
                Some(DesktopRow::Look { slug, .. }) => self.apply_look(cx, &slug),
                _ => {}
            },
            KeyCode::Char(' ') => {
                if let Some(DesktopRow::Entry { id, .. }) = self.selected(cx.at()).cloned()
                    && let Some(entry) = cx.catalog.find(&id).cloned()
                {
                    cx.toggle_queued(entry);
                    self.reload(cx);
                }
            }
            KeyCode::Char('s') => {
                *cx.popup = Some(Popup::LookName(InputPopup::new(
                    "Save this look",
                    "A name for the desktop's look (it is saved in ~/.config/helix-toolbox/looks):",
                )));
            }
            KeyCode::Char('r') => self.reload(cx),
            KeyCode::Char('h') | KeyCode::Left => cx.ask(Ask::Sidebar),
            _ => {}
        }
    }

    fn hints(&self, _view: &View, at: Option<usize>, list: &mut Vec<Hint>) {
        list.push(Hint::new(&["↑↓"], "Navigate"));
        match self.selected(at) {
            Some(DesktopRow::Entry { .. }) => {
                list.push(Hint::new(&["Enter"], "Run"));
                list.push(Hint::new(&["Space"], "Plan"));
            }
            Some(DesktopRow::Look { .. }) => list.push(Hint::new(&["Enter"], "Apply")),
            Some(DesktopRow::Info { .. }) => list.push(Hint::new(&["Enter"], "Details")),
            _ => {}
        }
        list.push(Hint::new(&["s"], "Save this look"));
        list.push(Hint::new(&["r"], "Read again"));
    }

    fn draw(&self, view: &View, frame: &mut Frame, area: Rect, state: &mut ListState) {
        let inner = view.page(
            frame,
            area,
            "Desktop",
            "The desktop, its login screen and look, and the entries for it; who else sets the look.",
        );
        view.fill(frame, inner);
        let focused = view.focused;
        let items: Vec<Line> = self
            .rows
            .iter()
            .map(|row| match row {
                DesktopRow::Heading(title) => Line::styled(
                    title.clone(),
                    Style::default().fg(view.theme.group()).bold(),
                ),
                DesktopRow::Info { text, .. } => Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        text.clone(),
                        Style::default().fg(
                            if text.starts_with("Note:") || text.starts_with("Not changed") {
                                view.theme.warning()
                            } else {
                                view.theme.entry()
                            },
                        ),
                    ),
                ]),
                DesktopRow::Entry { text, note, .. } => Line::from(vec![
                    Span::styled(
                        format!("  {} ", view.glyph(Icon::Entry)),
                        Style::default().fg(view.theme.muted()),
                    ),
                    Span::styled(text.clone(), Style::default().fg(view.theme.entry())),
                    Span::styled(
                        note.as_ref()
                            .map(|note| format!("  {note}"))
                            .unwrap_or_default(),
                        Style::default().fg(view.theme.faint()),
                    ),
                ]),
                DesktopRow::Look { text, .. } => Line::from(vec![
                    Span::styled(
                        format!("  {} ", view.glyph(Icon::Entry)),
                        Style::default().fg(view.theme.muted()),
                    ),
                    Span::styled(text.clone(), Style::default().fg(view.theme.entry())),
                ]),
            })
            .collect();
        let list = List::new(view.measure(items))
            .block(view.card("This machine"))
            .highlight_style(view.highlight(focused))
            .highlight_symbol(view.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, inner, state, view.hits);
    }
}
