//! The Updates screen.

use super::super::*;

impl App {
    /// Look at the update channels in the background; `online` also asks the
    /// network, through read-only commands.
    pub(in crate::tui::app) fn gather_updates(&mut self, online: bool) {
        if !self.examine_here {
            return;
        }
        let platform = self.platform.clone();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let channels =
                updates::overview(&doctor::Here, &platform, &crate::this_places(), online);
            let _ = sender.send((channels, online));
        });
        self.gathering = Some(receiver);
    }

    /// Package entries whose status says an update waits.
    pub(in crate::tui::app) fn entries_with_updates(&self) -> Vec<&Entry> {
        self.catalog
            .entries()
            .into_iter()
            .filter(|entry| matches!(self.statuses.get(&entry.id), Some(Status::Update { .. })))
            .collect()
    }

    /// What the selected row of the Updates screen is: a channel, then the
    /// entries with an update, then the row that updates them all.
    pub(in crate::tui::app) fn selected_update_row(&self) -> UpdateRow {
        let Some(index) = self.main_state.selected() else {
            return UpdateRow::Nothing;
        };
        if index < self.channels.len() {
            return UpdateRow::Channel;
        }
        let waiting = self.entries_with_updates();
        match waiting.get(index - self.channels.len()) {
            Some(entry) => UpdateRow::Entry(Box::new((*entry).clone())),
            None if !waiting.is_empty() && index == self.channels.len() + waiting.len() => {
                UpdateRow::Every
            }
            None => UpdateRow::Nothing,
        }
    }

    pub(in crate::tui::app) fn selected_channel(&self) -> Option<&updates::Channel> {
        self.main_state
            .selected()
            .and_then(|index| self.channels.get(index))
    }

    pub(in crate::tui::app) fn describe_channel(&mut self) {
        let Some(channel) = self.selected_channel() else {
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
        self.popup = Some(Popup::Text(TextPopup::new("Updates", text, true)));
    }

    /// Update every channel with updates known to wait, one after another.
    pub(in crate::tui::app) fn update_every_channel(&mut self) {
        let chosen: Vec<&updates::Channel> = self
            .channels
            .iter()
            .filter(|channel| channel.count().is_some_and(|count| count > 0))
            .filter(|channel| !channel.commands.is_empty())
            .collect();
        if chosen.is_empty() {
            self.popup = Some(Popup::Text(TextPopup::new(
                "Nothing to update",
                "No channel is known to have updates waiting. r checks online; u updates the selected channel anyway, which is how Nix channels are updated, since they count nothing beforehand.",
                true,
            )));
            return;
        }
        let planned = updates::plan(&chosen);
        self.confirm_plan(Ok(planned));
    }

    pub(in crate::tui::app) fn draw_updates(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Updates",
            "What each tool has waiting; updating runs that tool, after confirmation.",
        );
        let focused = self.focus == Focus::Main;
        let mut items: Vec<Line> = Vec::new();
        if self.channels.is_empty() {
            items.push(Line::styled(
                if self.gathering.is_some() {
                    "Looking…"
                } else {
                    "No package manager, Flatpak or Nix setup Helix Toolbox knows was found. r looks again, online."
                },
                Style::default().fg(self.theme.muted()),
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
                Some(0) => self.theme.success(),
                Some(_) => self.theme.warning(),
                None => self.theme.muted(),
            };
            items.push(Line::from(vec![
                Span::styled(
                    format!("{:<name_width$}", channel.name),
                    Style::default().fg(self.theme.entry()),
                ),
                Span::styled(
                    format!("{:<14}", channel.summary()),
                    Style::default().fg(color).bold(),
                ),
                Span::styled(
                    channel.note.clone(),
                    Style::default().fg(self.theme.muted()),
                ),
            ]));
        }
        let waiting = self.entries_with_updates();
        if !waiting.is_empty() {
            let width = waiting
                .iter()
                .map(|entry| entry.name.chars().count())
                .max()
                .unwrap_or(0)
                .max(name_width.saturating_sub(8))
                + 2;
            for entry in &waiting {
                let detail = self
                    .statuses
                    .get(&entry.id)
                    .map(|status| status.detail())
                    .unwrap_or_default();
                items.push(Line::from(vec![
                    Span::styled("Entry   ", Style::default().fg(self.theme.group())),
                    Span::styled(
                        format!("{:<width$}", entry.name),
                        Style::default().fg(self.theme.entry()),
                    ),
                    Span::styled("Update  ", Style::default().fg(self.theme.warning()).bold()),
                    Span::styled(detail, Style::default().fg(self.theme.muted())),
                ]));
            }
            items.push(Line::styled(
                format!("▶ Update every entry ({})", waiting.len()),
                Style::default().fg(self.theme.focused()).bold(),
            ));
        }
        let title = match (self.gathering.is_some(), self.channels_online) {
            (true, _) if !self.channels.is_empty() => "Channels (looking again…)",
            (_, true) => "Channels · checked online",
            _ => "Channels",
        };
        self.fill(frame, inner);
        let list = List::new(self.measure(items))
            .block(self.card(title))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused));
        bordered_list(frame, list, inner, &mut self.main_state, &self.hits);
    }
}
