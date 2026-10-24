//! The Bundles screen.

use super::super::*;

impl App {
    pub(in crate::tui::app) fn selected_bundle_row(&self) -> Option<BundleRow> {
        BUNDLE_ROWS.get(self.main_state.selected()?).copied()
    }

    /// A row's label and value; headings have no value.
    fn bundle_row(&self, row: BundleRow) -> (String, String) {
        let yes_no = |on: bool| if on { "yes" } else { "no" }.to_string();
        match row {
            BundleRow::MakeHeading => ("Make a bundle".to_string(), String::new()),
            BundleRow::Entries => (
                "Entries".to_string(),
                match self.queue.len() {
                    0 => "none: the Install Plan is empty".to_string(),
                    1 => "1 from the Install Plan".to_string(),
                    count => format!("{count} from the Install Plan"),
                },
            ),
            BundleRow::Folder => (
                "Folder, new or empty".to_string(),
                self.bundle_folder.clone(),
            ),
            BundleRow::Complete => ("Every dependency".to_string(), yes_no(self.bundle_complete)),
            BundleRow::Contents => ("Check what it needs".to_string(), String::new()),
            BundleRow::Make => ("Make the bundle".to_string(), String::new()),
            BundleRow::InstallHeading => ("Install from a bundle".to_string(), String::new()),
            BundleRow::Source => (
                "The bundle's folder".to_string(),
                if self.bundle_source.is_empty() {
                    "not chosen".to_string()
                } else {
                    self.bundle_source.clone()
                },
            ),
            BundleRow::Trust => (
                "Take the bundle's word for what it holds".to_string(),
                yes_no(self.bundle_trust),
            ),
            BundleRow::Check => ("Check it".to_string(), String::new()),
            BundleRow::Install => ("Install from it".to_string(), String::new()),
        }
    }

    /// What a row is for.
    fn bundle_help(&self, row: BundleRow) -> String {
        match row {
            BundleRow::MakeHeading => format!(
                "The Install Plan's entries and the packages they need for {}, downloaded with Helix Toolbox into a folder. Nothing is installed here.",
                self.platform.pretty_name
            ),
            BundleRow::Entries if self.queue.is_empty() => {
                "Queue the entries to bundle first: Space on an entry puts it in the Install Plan. Enter opens the Install Plan.".to_string()
            }
            BundleRow::Entries => format!(
                "{}. Enter opens the Install Plan.",
                self.queue
                    .iter()
                    .map(|entry| entry.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            BundleRow::Folder => "Where to download to: a new or empty folder. Enter changes it.".to_string(),
            BundleRow::Complete => "Every dependency makes a larger bundle, for a machine set up less than this one. Off, only what this machine lacks.".to_string(),
            BundleRow::Contents => "Asks the package manager what the bundle would download, and downloads nothing.".to_string(),
            BundleRow::Make => "Downloads into the folder once you confirm, with Helix Toolbox itself. Copy the folder over; on the other machine, inside it: ./helix-toolbox bundle install .".to_string(),
            BundleRow::InstallHeading => "A bundle is checked first: made for this distribution and architecture, and every file matching its checksum. Then its packages install from its files, without downloading, all together.".to_string(),
            BundleRow::Source => "The bundle's folder, such as /run/media/you/USB/helix-toolbox-bundle. Enter changes it.".to_string(),
            BundleRow::Trust => "Install Debian packages this machine's repository index does not know, and add a Flatpak remote this machine lacks with the bundle's copy of its key. Only for a bundle you made yourself.".to_string(),
            BundleRow::Check => "Checks what the bundle was made for and every file's checksum, and installs nothing.".to_string(),
            BundleRow::Install => "Checks the bundle, then shows the plan to confirm.".to_string(),
        }
    }

    pub(in crate::tui::app) fn bundle_enter(&mut self) {
        let Some(row) = self.selected_bundle_row() else {
            return;
        };
        match row {
            BundleRow::Entries => {
                self.open_screen(Screen::Plan);
                self.focus = Focus::Main;
            }
            BundleRow::Folder => {
                self.popup = Some(Popup::Ask(
                    InputPopup::new(
                        "Make a bundle",
                        "The folder to download into, new or empty:",
                    )
                    .with_value(self.bundle_folder.clone())
                    .path()
                    .answering("Keep"),
                    Asked::MakeFolder,
                ));
            }
            BundleRow::Complete => self.bundle_complete = !self.bundle_complete,
            BundleRow::Contents | BundleRow::Make if self.queue.is_empty() => {
                self.popup = Some(Popup::Text(TextPopup::new(
                    "The Install Plan is empty",
                    "Queue the entries to bundle in the Install Plan first: Space on an entry puts it there.",
                    true,
                )));
            }
            BundleRow::Contents => self.start_bundle_task(
                BundleTask::Contents,
                "Asking the package manager what the bundle needs…",
            ),
            BundleRow::Make => self.make_bundle(),
            BundleRow::Source => {
                self.popup = Some(Popup::Ask(
                    InputPopup::new("Install from a bundle", "The bundle's folder:")
                        .with_value(self.bundle_source.clone())
                        .path()
                        .answering("Keep"),
                    Asked::InstallFolder,
                ));
            }
            BundleRow::Trust => self.bundle_trust = !self.bundle_trust,
            BundleRow::Check | BundleRow::Install if self.bundle_source.trim().is_empty() => {
                self.popup = Some(Popup::Text(TextPopup::new(
                    "No bundle chosen",
                    "Choose the bundle's folder first, on the row above.",
                    true,
                )));
            }
            BundleRow::Check => self.start_bundle_task(
                BundleTask::Check,
                "Checking every file in the bundle against its checksum…",
            ),
            BundleRow::Install => self.start_bundle_task(
                BundleTask::Install,
                "Checking every file in the bundle against its checksum…",
            ),
            BundleRow::MakeHeading | BundleRow::InstallHeading => {}
        }
    }

    /// Show what is being done; it is done on the next tick.
    fn start_bundle_task(&mut self, task: BundleTask, notice: &str) {
        self.popup = Some(Popup::Text(TextPopup::new("Offline bundles", notice, true)));
        self.bundle_due = Some(task);
    }

    pub(in crate::tui::app) fn do_bundle_task(&mut self, task: BundleTask) {
        let said = |title: &str, text: String| Some(Popup::Text(TextPopup::new(title, text, true)));
        match task {
            BundleTask::Contents => {
                let ids: Vec<String> = self.queue.iter().map(|entry| entry.id.clone()).collect();
                let found = bundle::scratch()
                    .map_err(|error| format!("cannot create a scratch folder: {error}"))
                    .and_then(|scratch| {
                        let found = bundle::contents(
                            &ids,
                            &self.catalog,
                            &self.platform,
                            self.bundle_complete,
                            &doctor::Here,
                            &scratch,
                        );
                        let _ = std::fs::remove_dir_all(&scratch);
                        found.map_err(|error| error.to_string())
                    });
                self.popup = match found {
                    Ok(found) => {
                        let mut lines = rows::bundle_contents(&found, self.bundle_complete);
                        if !found.entries.iter().any(|entry| entry.offline) {
                            lines.push(String::new());
                            lines.push(
                                "None of these entries can be bundled; they all need the network."
                                    .to_string(),
                            );
                        }
                        said("What the bundle needs", lines.join("\n"))
                    }
                    Err(error) => said("Cannot tell what it needs", error),
                };
            }
            BundleTask::Check => {
                let checked = typed_path(&self.bundle_source).and_then(|directory| {
                    bundle::verify(&directory, &self.platform)
                        .map(|manifest| rows::manifest(&directory, &manifest))
                        .map_err(|error| error.to_string())
                });
                self.popup = match checked {
                    Ok(lines) => said("The bundle checks out", lines.join("\n")),
                    Err(error) => said("The bundle does not check out", error),
                };
            }
            BundleTask::Install => {
                let planned = typed_path(&self.bundle_source).and_then(|directory| {
                    let manifest = bundle::verify(&directory, &self.platform)
                        .map_err(|error| error.to_string())?;
                    bundle::plan_install(
                        &directory,
                        &manifest,
                        &[],
                        &self.platform,
                        &doctor::Here,
                        self.bundle_trust,
                    )
                    .map_err(|error| error.to_string())
                });
                self.popup = None;
                match planned {
                    Ok(planned) => self.confirm_plan(Ok(planned)),
                    Err(error) => self.popup = said("Cannot install from it", error),
                }
            }
        }
    }

    /// Plan making a bundle of the Install Plan, which the command line does
    /// in the running window.
    fn make_bundle(&mut self) {
        let ids: Vec<String> = self.queue.iter().map(|entry| entry.id.clone()).collect();
        let planned = typed_path(&self.bundle_folder).and_then(|directory| {
            if std::fs::read_dir(&directory).is_ok_and(|mut listing| listing.next().is_some()) {
                return Err(bundle::BundleError::NotEmpty(directory).to_string());
            }
            let this = std::env::current_exe()
                .map_err(|error| format!("cannot tell where this program is: {error}"))?;
            Ok(bundle::plan_create(
                &this,
                &directory,
                &ids,
                self.bundle_complete,
            ))
        });
        match planned {
            Ok(planned) => self.confirm_plan(Ok(planned)),
            Err(error) => {
                self.popup = Some(Popup::Text(TextPopup::new(
                    "Cannot make the bundle",
                    error,
                    true,
                )))
            }
        }
    }

    pub(in crate::tui::app) fn draw_bundles(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Offline bundles",
            "Packages downloaded here, for a machine of this distribution without internet.",
        );
        let help = self
            .selected_bundle_row()
            .map(|row| self.bundle_help(row))
            .unwrap_or_default();
        let width = (inner.width.saturating_sub(4) as usize).max(1);
        let height = help.chars().count().div_ceil(width).max(1) as u16 + 2;
        let parts = Layout::vertical([
            Constraint::Min(6),
            Constraint::Length(1),
            Constraint::Length(height),
        ])
        .split(inner);
        let focused = self.focus == Focus::Main;
        let room = parts[0].width.saturating_sub(6) as usize;
        let items: Vec<Line> = BUNDLE_ROWS
            .iter()
            .map(|&row| {
                let (label, value) = self.bundle_row(row);
                if matches!(row, BundleRow::MakeHeading | BundleRow::InstallHeading) {
                    return Line::styled(label, Style::default().fg(self.theme.group()).bold());
                }
                let label = format!("  {label}");
                let value = if value.chars().count() + label.chars().count() + 2 > room {
                    // A long path shows its end.
                    let keep = room.saturating_sub(label.chars().count() + 3);
                    let skip = value.chars().count().saturating_sub(keep);
                    format!("…{}", value.chars().skip(skip).collect::<String>())
                } else {
                    value
                };
                let gap = room.saturating_sub(label.chars().count() + value.chars().count());
                Line::from(vec![
                    Span::styled(label, Style::default().fg(self.theme.entry())),
                    Span::raw(" ".repeat(gap)),
                    Span::styled(value, Style::default().fg(self.theme.focused()).bold()),
                ])
            })
            .collect();
        self.fill(frame, parts[0]);
        let list = List::new(self.measure(items))
            .block(self.card("Offline bundles"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, parts[0], &mut self.main_state, &self.hits);
        self.fill(frame, parts[2]);
        frame.render_widget(
            Paragraph::new(help)
                .block(self.card("About this"))
                .style(Style::default().fg(self.theme.muted()))
                .wrap(Wrap { trim: true }),
            parts[2],
        );
    }
}
