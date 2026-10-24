//! The Bundles screen.

use super::super::*;
use super::{Ask, Cx, ToolScreen, View};

/// Offline bundles: made from the Install Plan, installed from a folder.
pub(in crate::tui::app) struct Bundles {
    /// The folder to make a bundle in, as typed.
    pub(in crate::tui::app) folder: String,
    /// Every dependency, not only what this machine lacks.
    pub(in crate::tui::app) complete: bool,
    /// The bundle to install from, as typed.
    source: String,
    /// Take the bundle's word for what it holds.
    trust: bool,
    /// Bundle work waiting for its notice to be drawn.
    due: Option<BundleTask>,
}

impl Default for Bundles {
    fn default() -> Self {
        Self {
            folder: "~/helix-toolbox-bundle".to_string(),
            complete: false,
            source: String::new(),
            trust: false,
            due: None,
        }
    }
}

/// A line of the Offline bundles screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::tui::app) enum BundleRow {
    MakeHeading,
    Entries,
    Folder,
    Complete,
    Contents,
    Make,
    InstallHeading,
    Source,
    Trust,
    Check,
    Install,
}

pub(in crate::tui::app) const BUNDLE_ROWS: [BundleRow; 11] = [
    BundleRow::MakeHeading,
    BundleRow::Entries,
    BundleRow::Folder,
    BundleRow::Complete,
    BundleRow::Contents,
    BundleRow::Make,
    BundleRow::InstallHeading,
    BundleRow::Source,
    BundleRow::Trust,
    BundleRow::Check,
    BundleRow::Install,
];

/// Bundle work that asks the package manager or reads every file: done on
/// the tick after its notice is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BundleTask {
    Contents,
    Check,
    Install,
}

impl Bundles {
    fn selected(at: Option<usize>) -> Option<BundleRow> {
        BUNDLE_ROWS.get(at?).copied()
    }

    /// The folder typed for making a bundle.
    pub(in crate::tui::app) fn set_folder(&mut self, text: &str) {
        self.folder = text.trim().to_string();
    }

    /// The bundle's folder typed for installing from it.
    pub(in crate::tui::app) fn set_source(&mut self, text: &str) {
        self.source = text.trim().to_string();
    }

    /// A row's label and value; headings have no value.
    fn row(&self, queue: &[Entry], row: BundleRow) -> (String, String) {
        let yes_no = |on: bool| if on { "yes" } else { "no" }.to_string();
        match row {
            BundleRow::MakeHeading => ("Make a bundle".to_string(), String::new()),
            BundleRow::Entries => (
                "Entries".to_string(),
                match queue.len() {
                    0 => "none: the Install Plan is empty".to_string(),
                    1 => "1 from the Install Plan".to_string(),
                    count => format!("{count} from the Install Plan"),
                },
            ),
            BundleRow::Folder => ("Folder, new or empty".to_string(), self.folder.clone()),
            BundleRow::Complete => ("Every dependency".to_string(), yes_no(self.complete)),
            BundleRow::Contents => ("Check what it needs".to_string(), String::new()),
            BundleRow::Make => ("Make the bundle".to_string(), String::new()),
            BundleRow::InstallHeading => ("Install from a bundle".to_string(), String::new()),
            BundleRow::Source => (
                "The bundle's folder".to_string(),
                if self.source.is_empty() {
                    "not chosen".to_string()
                } else {
                    self.source.clone()
                },
            ),
            BundleRow::Trust => (
                "Take the bundle's word for what it holds".to_string(),
                yes_no(self.trust),
            ),
            BundleRow::Check => ("Check it".to_string(), String::new()),
            BundleRow::Install => ("Install from it".to_string(), String::new()),
        }
    }

    /// What a row is for.
    fn help(&self, view: &View, row: BundleRow) -> String {
        match row {
            BundleRow::MakeHeading => format!(
                "The Install Plan's entries and the packages they need for {}, downloaded with Helix Toolbox into a folder. Nothing is installed here.",
                view.platform.pretty_name
            ),
            BundleRow::Entries if view.queue.is_empty() => {
                "Queue the entries to bundle first: Space on an entry puts it in the Install Plan. Enter opens the Install Plan.".to_string()
            }
            BundleRow::Entries => format!(
                "{}. Enter opens the Install Plan.",
                view.queue
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

    fn enter(&mut self, cx: &mut Cx) {
        let Some(row) = Self::selected(cx.at()) else {
            return;
        };
        match row {
            BundleRow::Entries => cx.ask(Ask::Open(Screen::Plan)),
            BundleRow::Folder => {
                *cx.popup = Some(Popup::Ask(
                    InputPopup::new(
                        "Make a bundle",
                        "The folder to download into, new or empty:",
                    )
                    .with_value(self.folder.clone())
                    .path()
                    .answering("Keep"),
                    Asked::MakeFolder,
                ));
            }
            BundleRow::Complete => self.complete = !self.complete,
            BundleRow::Contents | BundleRow::Make if cx.queue.is_empty() => {
                *cx.popup = Some(Popup::Text(TextPopup::new(
                    "The Install Plan is empty",
                    "Queue the entries to bundle in the Install Plan first: Space on an entry puts it there.",
                    true,
                )));
            }
            BundleRow::Contents => self.start_task(
                cx,
                BundleTask::Contents,
                "Asking the package manager what the bundle needs…",
            ),
            BundleRow::Make => self.make(cx),
            BundleRow::Source => {
                *cx.popup = Some(Popup::Ask(
                    InputPopup::new("Install from a bundle", "The bundle's folder:")
                        .with_value(self.source.clone())
                        .path()
                        .answering("Keep"),
                    Asked::InstallFolder,
                ));
            }
            BundleRow::Trust => self.trust = !self.trust,
            BundleRow::Check | BundleRow::Install if self.source.trim().is_empty() => {
                *cx.popup = Some(Popup::Text(TextPopup::new(
                    "No bundle chosen",
                    "Choose the bundle's folder first, on the row above.",
                    true,
                )));
            }
            BundleRow::Check => self.start_task(
                cx,
                BundleTask::Check,
                "Checking every file in the bundle against its checksum…",
            ),
            BundleRow::Install => self.start_task(
                cx,
                BundleTask::Install,
                "Checking every file in the bundle against its checksum…",
            ),
            BundleRow::MakeHeading | BundleRow::InstallHeading => {}
        }
    }

    /// Show what is being done; it is done on the next tick.
    fn start_task(&mut self, cx: &mut Cx, task: BundleTask, notice: &str) {
        *cx.popup = Some(Popup::Text(TextPopup::new("Offline bundles", notice, true)));
        self.due = Some(task);
    }

    fn do_task(&self, cx: &mut Cx, task: BundleTask) {
        let said = |title: &str, text: String| Some(Popup::Text(TextPopup::new(title, text, true)));
        match task {
            BundleTask::Contents => {
                let ids: Vec<String> = cx.queue.iter().map(|entry| entry.id.clone()).collect();
                let found = bundle::scratch()
                    .map_err(|error| format!("cannot create a scratch folder: {error}"))
                    .and_then(|scratch| {
                        let found = bundle::contents(
                            &ids,
                            cx.catalog,
                            cx.platform,
                            self.complete,
                            cx.host,
                            &scratch,
                        );
                        let _ = std::fs::remove_dir_all(&scratch);
                        found.map_err(|error| error.to_string())
                    });
                *cx.popup = match found {
                    Ok(found) => {
                        let mut lines = rows::bundle_contents(&found, self.complete);
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
                let checked = typed_path(&self.source).and_then(|directory| {
                    bundle::verify(&directory, cx.platform)
                        .map(|manifest| rows::manifest(&directory, &manifest))
                        .map_err(|error| error.to_string())
                });
                *cx.popup = match checked {
                    Ok(lines) => said("The bundle checks out", lines.join("\n")),
                    Err(error) => said("The bundle does not check out", error),
                };
            }
            BundleTask::Install => {
                let planned = typed_path(&self.source).and_then(|directory| {
                    let manifest = bundle::verify(&directory, cx.platform)
                        .map_err(|error| error.to_string())?;
                    bundle::plan_install(
                        &directory,
                        &manifest,
                        &[],
                        cx.platform,
                        cx.host,
                        self.trust,
                    )
                    .map_err(|error| error.to_string())
                });
                *cx.popup = None;
                match planned {
                    Ok(planned) => cx.confirm(planned),
                    Err(error) => *cx.popup = said("Cannot install from it", error),
                }
            }
        }
    }

    /// Plan making a bundle of the Install Plan, which the command line does
    /// in the running window.
    fn make(&self, cx: &mut Cx) {
        let ids: Vec<String> = cx.queue.iter().map(|entry| entry.id.clone()).collect();
        let planned = typed_path(&self.folder).and_then(|directory| {
            if std::fs::read_dir(&directory).is_ok_and(|mut listing| listing.next().is_some()) {
                return Err(bundle::BundleError::NotEmpty(directory).to_string());
            }
            let this = std::env::current_exe()
                .map_err(|error| format!("cannot tell where this program is: {error}"))?;
            Ok(bundle::plan_create(&this, &directory, &ids, self.complete))
        });
        match planned {
            Ok(planned) => cx.confirm(planned),
            Err(error) => {
                *cx.popup = Some(Popup::Text(TextPopup::new(
                    "Cannot make the bundle",
                    error,
                    true,
                )))
            }
        }
    }
}

impl ToolScreen for Bundles {
    fn open(&mut self, _cx: &mut Cx) {}

    fn len(&self, _view: &View) -> usize {
        BUNDLE_ROWS.len()
    }

    fn key(&mut self, cx: &mut Cx, code: KeyCode) {
        match code {
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('l') | KeyCode::Right => {
                self.enter(cx)
            }
            KeyCode::Char('h') | KeyCode::Left => cx.ask(Ask::Sidebar),
            _ => {}
        }
    }

    fn hints(&self, _view: &View, at: Option<usize>, list: &mut Vec<Hint>) {
        list.push(Hint::new(&["↑↓"], "Navigate"));
        let verb = match Self::selected(at) {
            Some(BundleRow::Entries) => "Install Plan",
            Some(BundleRow::Folder | BundleRow::Source) => "Change",
            Some(BundleRow::Complete | BundleRow::Trust) => "Switch",
            Some(BundleRow::Contents | BundleRow::Check) => "Check",
            Some(BundleRow::Make) => "Make",
            Some(BundleRow::Install) => "Install",
            _ => "",
        };
        if !verb.is_empty() {
            list.push(Hint::new(&["Enter"], verb));
        }
    }

    /// Bundle work waiting for its notice to be drawn is done now.
    fn poll(&mut self, cx: &mut Cx) -> bool {
        let Some(task) = self.due.take() else {
            return false;
        };
        self.do_task(cx, task);
        true
    }

    fn draw(&self, view: &View, frame: &mut Frame, area: Rect, state: &mut ListState) {
        let inner = view.page(
            frame,
            area,
            "Offline bundles",
            "Packages downloaded here, for a machine of this distribution without internet.",
        );
        let help = Self::selected(state.selected())
            .map(|row| self.help(view, row))
            .unwrap_or_default();
        let width = (inner.width.saturating_sub(4) as usize).max(1);
        let height = help.chars().count().div_ceil(width).max(1) as u16 + 2;
        let parts = Layout::vertical([
            Constraint::Min(6),
            Constraint::Length(1),
            Constraint::Length(height),
        ])
        .split(inner);
        let focused = view.focused;
        let room = parts[0].width.saturating_sub(6) as usize;
        let items: Vec<Line> = BUNDLE_ROWS
            .iter()
            .map(|&row| {
                let (label, value) = self.row(view.queue, row);
                if matches!(row, BundleRow::MakeHeading | BundleRow::InstallHeading) {
                    return Line::styled(label, Style::default().fg(view.theme.group()).bold());
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
                    Span::styled(label, Style::default().fg(view.theme.entry())),
                    Span::raw(" ".repeat(gap)),
                    Span::styled(value, Style::default().fg(view.theme.focused()).bold()),
                ])
            })
            .collect();
        view.fill(frame, parts[0]);
        let list = List::new(view.measure(items))
            .block(view.card("Offline bundles"))
            .highlight_style(view.highlight(focused))
            .highlight_symbol(view.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, parts[0], state, view.hits);
        view.fill(frame, parts[2]);
        frame.render_widget(
            Paragraph::new(help)
                .block(view.card("About this"))
                .style(Style::default().fg(view.theme.muted()))
                .wrap(Wrap { trim: true }),
            parts[2],
        );
    }
}
