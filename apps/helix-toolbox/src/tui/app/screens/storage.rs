//! The Storage screen.

use super::super::*;

impl App {
    /// Read the storage in the background: sizing caches takes a moment.
    pub(in crate::tui::app) fn read_storage(&mut self) {
        if !self.examine_here {
            self.build_storage_rows();
            return;
        }
        let manager = self.platform.manager;
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(storage::gather(&doctor::Here, manager));
        });
        self.reading_storage = Some(receiver);
    }

    pub(in crate::tui::app) fn build_storage_rows(&mut self) {
        let mut list = Vec::new();
        if let Some(found) = &self.storage_found {
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
        self.storage_rows = list;
        self.clamp_main();
    }

    pub(in crate::tui::app) fn selected_storage_row(&self) -> Option<StorageRow> {
        self.storage_rows.get(self.main_state.selected()?).cloned()
    }

    pub(in crate::tui::app) fn describe_storage(&mut self) {
        let Some(found) = &self.storage_found else {
            return;
        };
        let (title, text) = match self.selected_storage_row() {
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
        self.popup = Some(Popup::Text(TextPopup::new(title, text, true)));
    }

    pub(in crate::tui::app) fn clean_selected(&mut self) {
        let Some(StorageRow::Item(index)) = self.selected_storage_row() else {
            if matches!(self.selected_storage_row(), Some(StorageRow::Kept(_))) {
                self.describe_storage();
            }
            return;
        };
        let Some(item) = self
            .storage_found
            .as_ref()
            .and_then(|found| found.items.get(index))
            .cloned()
        else {
            return;
        };
        match storage::plan(&item, self.platform.manager, &doctor::Here) {
            Ok(planned) => self.confirm_plan(Ok(planned)),
            Err(why) => {
                self.popup = Some(Popup::Text(TextPopup::new(item.title, why, true)));
            }
        }
    }

    pub(in crate::tui::app) fn draw_storage(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Storage",
            "Where the space goes and what can be cleaned; each cleanup asks first.",
        );
        self.fill(frame, inner);
        let focused = self.focus == Focus::Main;
        let mut items: Vec<Line> = Vec::new();
        if self.storage_rows.is_empty() {
            items.push(Line::styled(
                if self.reading_storage.is_some() {
                    "Reading the storage…"
                } else {
                    "Press r to read this machine's storage."
                },
                Style::default().fg(self.theme.muted()),
            ));
        }
        if let Some(found) = &self.storage_found {
            for row in &self.storage_rows {
                items.push(match row {
                    StorageRow::Heading(text) => {
                        Line::styled(text.clone(), Style::default().fg(self.theme.group()).bold())
                    }
                    StorageRow::Filesystem(text) => {
                        Line::styled(format!("  {text}"), Style::default().fg(self.theme.entry()))
                    }
                    StorageRow::Item(index) => {
                        let item = &found.items[*index];
                        let rows::CleanRow {
                            size,
                            said,
                            offered,
                        } = rows::clean(item);
                        let color = if offered {
                            self.theme.success()
                        } else {
                            self.theme.muted()
                        };
                        Line::from(vec![
                            Span::styled(
                                format!("  {:<26} ", item.title),
                                Style::default().fg(self.theme.entry()),
                            ),
                            Span::styled(
                                format!("{size:>9}  "),
                                Style::default().fg(self.theme.muted()),
                            ),
                            Span::styled(said, Style::default().fg(color)),
                        ])
                    }
                    StorageRow::Kept(index) => {
                        let kept = &found.kept[*index];
                        Line::from(vec![
                            Span::styled(
                                format!("  {:<26} ", kept.title),
                                Style::default().fg(self.theme.entry()),
                            ),
                            Span::styled(
                                rows::kept_size(kept).unwrap_or_default(),
                                Style::default().fg(self.theme.muted()),
                            ),
                        ])
                    }
                });
            }
        }
        let list = List::new(self.measure(items))
            .block(self.card("Storage"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, inner, &mut self.main_state, &self.hits);
    }
}
