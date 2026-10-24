//! The Storage screen.

use super::super::*;
use super::{Ask, Cx, Read, ToolScreen, View};

/// Where the space goes, and what can be cleaned.
#[derive(Default)]
pub(in crate::tui::app) struct Storage {
    /// The storage, as last read.
    pub(in crate::tui::app) found: Option<storage::Storage>,
    pub(in crate::tui::app) rows: Vec<StorageRow>,
    /// A look at the storage running in the background.
    reading: Read<storage::Storage>,
}

/// A line of the Storage screen.
#[derive(Clone, Debug)]
pub(in crate::tui::app) enum StorageRow {
    Heading(String),
    Filesystem(String),
    Item(usize),
    Kept(usize),
}

impl Storage {
    /// Read the storage in the background: sizing caches takes a moment.
    fn read(&mut self, cx: &Cx) {
        let manager = cx.platform.manager;
        self.reading
            .start(cx.host, move |host| storage::gather_on(&host, manager));
    }

    fn build_rows(&mut self, cx: &mut Cx) {
        let mut list = Vec::new();
        if let Some(found) = &self.found {
            list.push(StorageRow::Heading("Space".to_string()));
            for filesystem in &found.filesystems {
                let space = rows::space(filesystem);
                list.push(StorageRow::Filesystem(format!(
                    "{}: {}",
                    space.path, space.text
                )));
            }
            list.push(StorageRow::Heading("What can be cleaned".to_string()));
            list.extend((0..found.items.len()).map(StorageRow::Item));
            if !found.kept.is_empty() {
                list.push(StorageRow::Heading("Kept, never cleaned here".to_string()));
                list.extend((0..found.kept.len()).map(StorageRow::Kept));
            }
        }
        self.rows = list;
        cx.clamp();
    }

    fn selected(&self, at: Option<usize>) -> Option<StorageRow> {
        self.rows.get(at?).cloned()
    }

    fn describe(&self, cx: &mut Cx) {
        let Some(found) = &self.found else {
            return;
        };
        let (title, text) = match self.selected(cx.at()) {
            Some(StorageRow::Item(index)) => {
                let item = &found.items[index];
                let mut text = item.detail.clone();
                if let Some(size) = item.size {
                    text.push_str(&format!("\n\nIt takes {}.", storage::shown(size)));
                }
                match (item.frees, &item.not_offered) {
                    (_, Some(why)) => text.push_str(&format!("\nNot offered: {why}.")),
                    (Some(frees), None) => text.push_str(&format!(
                        "\nCleaning it frees about {}. Enter shows the plan, then asks.",
                        storage::shown(frees)
                    )),
                    (None, None) => text.push_str(
                        "\nWhat cleaning it frees is known only when it runs. Enter shows the plan, then asks.",
                    ),
                }
                if !item.packages.is_empty() {
                    text.push_str(&format!("\n\nPackages: {}", item.packages.join(", ")));
                }
                (item.title.clone(), text)
            }
            Some(StorageRow::Kept(index)) => {
                let kept = &found.kept[index];
                (kept.title.clone(), kept.detail.clone())
            }
            _ => return,
        };
        *cx.popup = Some(Popup::Text(TextPopup::new(title, text, true)));
    }

    fn clean_selected(&self, cx: &mut Cx) {
        let Some(StorageRow::Item(index)) = self.selected(cx.at()) else {
            if matches!(self.selected(cx.at()), Some(StorageRow::Kept(_))) {
                self.describe(cx);
            }
            return;
        };
        let Some(item) = self
            .found
            .as_ref()
            .and_then(|found| found.items.get(index))
            .cloned()
        else {
            return;
        };
        match storage::plan(&item, cx.platform.manager, cx.host) {
            Ok(planned) => cx.confirm(planned),
            Err(why) => {
                *cx.popup = Some(Popup::Text(TextPopup::new(item.title, why, true)));
            }
        }
    }
}

impl ToolScreen for Storage {
    fn open(&mut self, cx: &mut Cx) {
        if self.found.is_none() {
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
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('l') | KeyCode::Right => {
                self.clean_selected(cx)
            }
            KeyCode::Char('d') => self.describe(cx),
            KeyCode::Char('r') => self.read(cx),
            KeyCode::Char('h') | KeyCode::Left => cx.ask(Ask::Sidebar),
            _ => {}
        }
    }

    fn hints(&self, _view: &View, at: Option<usize>, list: &mut Vec<Hint>) {
        list.push(Hint::new(&["↑↓"], "Navigate"));
        if let Some(StorageRow::Item(index)) = self.selected(at)
            && self
                .found
                .as_ref()
                .and_then(|found| found.items.get(index))
                .is_some_and(|item| item.not_offered.is_none())
        {
            list.push(Hint::new(&["Enter"], "Clean"));
        }
        list.push(Hint::new(&["d"], "Details"));
        list.push(Hint::new(&["r"], "Read again"));
    }

    fn poll(&mut self, cx: &mut Cx) -> bool {
        let Some(found) = self.reading.take() else {
            return false;
        };
        self.found = Some(found);
        self.build_rows(cx);
        true
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
            "Storage",
            "Where the space goes and what can be cleaned; each cleanup asks first.",
        );
        view.fill(frame, inner);
        let focused = view.focused;
        let mut items: Vec<Line> = Vec::new();
        if self.rows.is_empty() {
            items.push(Line::styled(
                if self.reading.busy() {
                    "Reading the storage…"
                } else {
                    "Press r to read this machine's storage."
                },
                Style::default().fg(view.theme.muted()),
            ));
        }
        if let Some(found) = &self.found {
            for row in &self.rows {
                items.push(match row {
                    StorageRow::Heading(text) => {
                        Line::styled(text.clone(), Style::default().fg(view.theme.group()).bold())
                    }
                    StorageRow::Filesystem(text) => {
                        Line::styled(format!("  {text}"), Style::default().fg(view.theme.entry()))
                    }
                    StorageRow::Item(index) => {
                        let item = &found.items[*index];
                        let rows::CleanRow {
                            size,
                            said,
                            offered,
                        } = rows::clean(item);
                        let color = if offered {
                            view.theme.success()
                        } else {
                            view.theme.muted()
                        };
                        Line::from(vec![
                            Span::styled(
                                format!("  {:<26} ", item.title),
                                Style::default().fg(view.theme.entry()),
                            ),
                            Span::styled(
                                format!("{size:>9}  "),
                                Style::default().fg(view.theme.muted()),
                            ),
                            Span::styled(said, Style::default().fg(color)),
                        ])
                    }
                    StorageRow::Kept(index) => {
                        let kept = &found.kept[*index];
                        Line::from(vec![
                            Span::styled(
                                format!("  {:<26} ", kept.title),
                                Style::default().fg(view.theme.entry()),
                            ),
                            Span::styled(
                                rows::kept_size(kept).unwrap_or_default(),
                                Style::default().fg(view.theme.muted()),
                            ),
                        ])
                    }
                });
            }
        }
        let list = List::new(view.measure(items))
            .block(view.card("Storage"))
            .highlight_style(view.highlight(focused))
            .highlight_symbol(view.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, inner, state, view.hits);
    }
}
