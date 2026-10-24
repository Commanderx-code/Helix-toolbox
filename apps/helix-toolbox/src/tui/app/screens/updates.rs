//! The Updates screen.

use super::super::*;
use super::{Ask, Cx, Read, ToolScreen, View};

/// Pending updates of each tool that keeps the machine, then the package
/// entries with an update.
#[derive(Default)]
pub(in crate::tui::app) struct Updates {
    /// The update channels, from the last look.
    pub(in crate::tui::app) channels: Vec<updates::Channel>,
    /// Whether that look asked the network.
    online: bool,
    /// A look at the update channels running in the background.
    gathering: Read<(Vec<updates::Channel>, bool)>,
    /// A run while another screen was shown left what it read out of
    /// date.
    stale: bool,
}

/// The selected line of the Updates screen.
#[derive(Clone, Debug)]
enum UpdateRow {
    Nothing,
    Channel,
    /// A package entry with an update.
    Entry(Box<Entry>),
    /// The line that updates every entry with an update.
    Every,
}

/// Package entries whose status says an update waits.
fn entries_with_updates<'a>(
    catalog: &'a Catalog,
    statuses: &HashMap<String, Status>,
) -> Vec<&'a Entry> {
    catalog
        .entries()
        .into_iter()
        .filter(|entry| matches!(statuses.get(&entry.id), Some(Status::Update { .. })))
        .collect()
}

impl Updates {
    /// Look at the update channels in the background; `online` also asks the
    /// network, through read-only commands.
    pub(in crate::tui::app) fn gather(&mut self, cx: &Cx, online: bool) {
        let platform = cx.platform.clone();
        // The running Helix Toolbox, with its own update, is on this machine
        // only.
        let places = if cx.host.is_here() {
            crate::this_places()
        } else {
            updates::Places::of(cx.host)
        };
        self.gathering.start(cx.host, move |host| {
            let channels = updates::overview(&host, &platform, &places, online);
            (channels, online)
        });
    }

    /// The rows above the entries: the channels, or the line that says
    /// they are being looked for or none was found.
    fn channel_rows(&self) -> usize {
        self.channels.len().max(1)
    }

    /// What row `at` is: a channel, then the entries with an update, then
    /// the row that updates them all.
    fn row(
        &self,
        catalog: &Catalog,
        statuses: &HashMap<String, Status>,
        at: Option<usize>,
    ) -> UpdateRow {
        let Some(index) = at else {
            return UpdateRow::Nothing;
        };
        if index < self.channels.len() {
            return UpdateRow::Channel;
        }
        let Some(index) = index.checked_sub(self.channel_rows()) else {
            return UpdateRow::Nothing;
        };
        let waiting = entries_with_updates(catalog, statuses);
        match waiting.get(index) {
            Some(entry) => UpdateRow::Entry(Box::new((*entry).clone())),
            None if !waiting.is_empty() && index == waiting.len() => UpdateRow::Every,
            None => UpdateRow::Nothing,
        }
    }

    fn selected_channel(&self, at: Option<usize>) -> Option<&updates::Channel> {
        at.and_then(|index| self.channels.get(index))
    }

    fn describe_channel(&self, cx: &mut Cx) {
        let Some(channel) = self.selected_channel(cx.at()) else {
            return;
        };
        let mut text = format!(
            "{}: {}\n{}\n",
            channel.name,
            channel.summary(),
            channel.note
        );
        if let Some(pending) = &channel.pending
            && !pending.is_empty()
        {
            text.push('\n');
            for item in pending {
                let versions = match (&item.from, &item.to) {
                    (Some(from), Some(to)) => format!("{from} → {to}"),
                    (None, Some(to)) => to.clone(),
                    _ => String::new(),
                };
                text.push_str(&format!(
                    "  {}  {}\n",
                    helix_toolbox_core::printable(&item.name),
                    helix_toolbox_core::printable(&versions)
                ));
            }
        }
        if channel.commands.is_empty() {
            text.push_str("\nThere is nothing to run here.\n");
        } else {
            text.push_str(&format!("\nu runs: {}\n", channel.command_line()));
        }
        if let Some(caveat) = &channel.caveat {
            text.push_str(&format!("Note: {caveat}\n"));
        }
        *cx.popup = Some(Popup::Text(TextPopup::new("Updates", text, true)));
    }

    /// Update every channel with updates known to wait, one after another.
    fn update_every_channel(&self, cx: &mut Cx) {
        let chosen: Vec<&updates::Channel> = self
            .channels
            .iter()
            .filter(|channel| channel.count().is_some_and(|count| count > 0))
            .filter(|channel| !channel.commands.is_empty())
            .collect();
        if chosen.is_empty() {
            *cx.popup = Some(Popup::Text(TextPopup::new(
                "Nothing to update",
                "No channel is known to have updates waiting. r checks online; u updates the selected channel anyway, which is how Nix channels are updated, since they count nothing beforehand.",
                true,
            )));
            return;
        }
        let planned = updates::plan(&chosen);
        cx.confirm(planned);
    }
}

impl ToolScreen for Updates {
    fn open(&mut self, cx: &mut Cx) {
        if std::mem::take(&mut self.stale) || self.channels.is_empty() {
            self.gather(cx, false);
        }
    }

    fn len(&self, view: &View) -> usize {
        let waiting = entries_with_updates(view.catalog, view.statuses).len();
        self.channel_rows() + if waiting > 0 { waiting + 1 } else { 0 }
    }

    fn key(&mut self, cx: &mut Cx, code: KeyCode) {
        match (code, self.row(cx.catalog, cx.statuses, cx.at())) {
            (KeyCode::Enter | KeyCode::Char('u'), UpdateRow::Entry(entry)) => {
                cx.ask(Ask::Update(vec![*entry]))
            }
            (KeyCode::Char('d') | KeyCode::Char('D'), UpdateRow::Entry(_)) => cx.ask(Ask::Describe),
            (KeyCode::Enter, UpdateRow::Every) => {
                let waiting = entries_with_updates(cx.catalog, cx.statuses)
                    .into_iter()
                    .cloned()
                    .collect();
                cx.ask(Ask::Update(waiting));
            }
            (code, _) => match code {
                KeyCode::Enter | KeyCode::Char('d') => self.describe_channel(cx),
                KeyCode::Char('u') => match self.selected_channel(cx.at()).cloned() {
                    Some(channel) if channel.commands.is_empty() => {
                        *cx.popup = Some(Popup::Text(TextPopup::new(
                            "Nothing to run",
                            format!("{}: {}", channel.name, channel.note),
                            true,
                        )));
                    }
                    Some(channel) => cx.confirm(updates::plan(&[&channel])),
                    None => {}
                },
                KeyCode::Char('U') => self.update_every_channel(cx),
                KeyCode::Char('r') => self.gather(cx, true),
                KeyCode::Char('h') | KeyCode::Left => cx.ask(Ask::Sidebar),
                _ => {}
            },
        }
    }

    fn hints(&self, view: &View, at: Option<usize>, list: &mut Vec<Hint>) {
        list.push(Hint::new(&["↑↓"], "Navigate"));
        match self.row(view.catalog, view.statuses, at) {
            UpdateRow::Channel => {
                list.push(Hint::new(&["Enter"], "Details"));
                list.push(Hint::new(&["u"], "Update"));
                list.push(Hint::new(&["U"], "Update all"));
            }
            UpdateRow::Entry(_) => {
                list.push(Hint::new(&["Enter", "u"], "Update it"));
                list.push(Hint::new(&["d"], "Details"));
            }
            UpdateRow::Every => list.push(Hint::new(&["Enter"], "Update every entry")),
            UpdateRow::Nothing => {}
        }
        list.push(Hint::new(&["r"], "Check online"));
    }

    fn poll(&mut self, cx: &mut Cx) -> bool {
        let Some(answer) = self.gathering.take() else {
            return false;
        };
        if let Ok((channels, online)) = answer {
            self.channels = channels;
            self.online = online;
        }
        if cx.screen == Screen::Updates {
            cx.clamp();
        }
        true
    }

    fn after_run(&mut self, cx: &mut Cx, reading: Reading) {
        if reading == Reading::Updates {
            self.gather(cx, false);
        }
    }

    fn stale(&mut self, reading: Reading) {
        if reading == Reading::Updates {
            self.stale = true;
        }
    }

    /// The entries come after the channels.
    fn entry<'a>(&self, view: &View<'a>, at: Option<usize>) -> Option<&'a Entry> {
        let index = at?.checked_sub(self.channel_rows())?;
        entries_with_updates(view.catalog, view.statuses)
            .into_iter()
            .nth(index)
    }

    fn draw(&self, view: &View, frame: &mut Frame, area: Rect, state: &mut ListState) {
        let inner = view.page(
            frame,
            area,
            "Updates",
            "What each tool has waiting; updating runs that tool, after confirmation.",
        );
        let focused = view.focused;
        let mut items: Vec<Line> = Vec::new();
        if self.channels.is_empty() {
            items.push(Line::styled(
                if self.gathering.busy() {
                    "Looking…"
                } else if self.gathering.failed() {
                    "The look at the update channels stopped without an answer. r looks again, online."
                } else {
                    "No package manager, Flatpak or Nix setup Helix Toolbox knows was found. r looks again, online."
                },
                Style::default().fg(view.theme.muted()),
            ));
        }
        let name_width = self
            .channels
            .iter()
            .map(|channel| channel.name.chars().count())
            .max()
            .unwrap_or(0)
            + 2;
        for channel in &self.channels {
            let color = match channel.count() {
                Some(0) => view.theme.success(),
                Some(_) => view.theme.warning(),
                None => view.theme.muted(),
            };
            items.push(Line::from(vec![
                Span::styled(
                    format!("{:<name_width$}", channel.name),
                    Style::default().fg(view.theme.entry()),
                ),
                Span::styled(
                    format!("{:<14}", channel.summary()),
                    Style::default().fg(color).bold(),
                ),
                Span::styled(
                    channel.note.clone(),
                    Style::default().fg(view.theme.muted()),
                ),
            ]));
        }
        let waiting = entries_with_updates(view.catalog, view.statuses);
        if !waiting.is_empty() {
            let width = waiting
                .iter()
                .map(|entry| entry.name.chars().count())
                .max()
                .unwrap_or(0)
                .max(name_width.saturating_sub(8))
                + 2;
            for entry in &waiting {
                let detail = view
                    .statuses
                    .get(&entry.id)
                    .map(|status| status.detail())
                    .unwrap_or_default();
                items.push(Line::from(vec![
                    Span::styled("Entry   ", Style::default().fg(view.theme.group())),
                    Span::styled(
                        format!("{:<width$}", entry.name),
                        Style::default().fg(view.theme.entry()),
                    ),
                    Span::styled("Update  ", Style::default().fg(view.theme.warning()).bold()),
                    Span::styled(detail, Style::default().fg(view.theme.muted())),
                ]));
            }
            items.push(Line::styled(
                format!("▶ Update every entry ({})", waiting.len()),
                Style::default().fg(view.theme.focused()).bold(),
            ));
        }
        let title = match (self.gathering.busy(), self.online) {
            (true, _) if !self.channels.is_empty() => "Channels (looking again…)",
            (_, true) => "Channels · checked online",
            _ => "Channels",
        };
        view.fill(frame, inner);
        let list = List::new(view.measure(items))
            .block(view.card(title))
            .highlight_style(view.highlight(focused))
            .highlight_symbol(view.marker(focused));
        bordered_list(frame, list, inner, state, view.hits);
    }
}
