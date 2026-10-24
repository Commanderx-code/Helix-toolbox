//! The catalog areas, search, Favorites, the Install Plan and the command palette.

use super::*;

impl App {
    pub(super) fn draw_area(&mut self, frame: &mut Frame, area: Rect) {
        let Some(index) = self.current_area() else {
            return;
        };
        let name = self.catalog.tabs[index].name.clone();
        let trail: Vec<String> = {
            let tab = &self.catalog.tabs[index];
            let mut names = Vec::new();
            let mut items = tab.items.as_slice();
            for (group, _) in &self.areas[index].path {
                if let Some(Item::Group(group)) = items.get(*group) {
                    names.push(group.name.clone());
                    items = &group.items;
                }
            }
            names
        };
        let subtitle = if trail.is_empty() {
            format!("{} entries", self.catalog.tabs[index].entries().len())
        } else {
            trail.join(" › ")
        };
        let content = self.page(frame, area, &name, &subtitle);
        let parts = Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).split(content);
        self.draw_search(frame, parts[0]);
        self.draw_list(frame, parts[1]);
    }

    pub(super) fn draw_search(&self, frame: &mut Frame, area: Rect) {
        let searching = self.focus == Focus::Search;
        let prefix = self.icon_gap(Icon::Search);
        let mut spans = vec![Span::styled(
            prefix.clone(),
            Style::default().fg(if searching {
                self.theme.focused()
            } else {
                self.theme.muted()
            }),
        )];
        if !searching && self.search.is_empty() {
            spans.push(Span::styled(
                "Type to search every area (/)",
                Style::default().fg(self.theme.muted()),
            ));
        } else {
            spans.push(Span::styled(
                self.search.iter().collect::<String>(),
                Style::default().fg(self.theme.focused()).bold(),
            ));
            if let Some(completion) = &self.completion {
                spans.push(Span::styled(
                    completion.clone(),
                    Style::default().fg(self.theme.muted()),
                ));
            }
        }
        let color = if searching {
            self.theme.focused()
        } else {
            self.theme.line()
        };
        let block = Block::bordered()
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(color))
            .padding(Padding::horizontal(1));
        self.fill(frame, area);
        if searching {
            let inner = block.inner(area);
            let x =
                inner.x + prefix.chars().count() as u16 + self.search[..self.cursor].len() as u16;
            frame.set_cursor_position((x.min(inner.right().saturating_sub(1)), inner.y));
        }
        frame.render_widget(Paragraph::new(Line::from(spans)).block(block), area);
    }

    pub(super) fn draw_list(&mut self, frame: &mut Frame, area: Rect) {
        let focused = self.focus == Focus::Main;
        let title = match (self.searching(), self.screen) {
            (true, _) => "Results",
            (false, Screen::Favorites) => "Favorites",
            (false, _) => "Entries",
        };
        let title = match self.filter {
            Filter::All => title.to_string(),
            filter => format!("{title} · {}", filter.label()),
        };
        let block = self.card(&title).title(
            Line::styled(" STATUS · FLAGS ", Style::default().fg(self.theme.muted()))
                .right_aligned(),
        );
        let marker = self.marker(focused);
        let content_width =
            (block.inner(area).width as usize).saturating_sub(marker.chars().count());
        let group = self.glyph(Icon::Group);
        let searching = self.searching();
        let lines: Vec<Line> = self
            .rows
            .iter()
            .map(|row| match row {
                Row::Up => Line::from(format!("{group}  .."))
                    .style(Style::default().fg(self.theme.group())),
                Row::Group {
                    name,
                    description,
                    count,
                    ..
                } => {
                    // Its name, what it holds, and how many, as the window shows a group.
                    let left = format!("{group}  {name}");
                    let number = format!("{count} ");
                    let room = content_width
                        .saturating_sub(left.chars().count() + number.chars().count() + 2);
                    let about = if description.is_empty() || room < 8 {
                        String::new()
                    } else {
                        format!("  {}", clipped(description, room))
                    };
                    let gap = " ".repeat(content_width.saturating_sub(
                        left.chars().count() + about.chars().count() + number.chars().count(),
                    ));
                    Line::from(vec![
                        Span::styled(left, Style::default().fg(self.theme.group())),
                        Span::styled(about, Style::default().fg(self.theme.faint())),
                        Span::raw(gap),
                        Span::styled(number, Style::default().fg(self.theme.muted())),
                    ])
                }
                Row::Entry(entry) => {
                    let queued = self.is_queued(entry);
                    let (style, icon_color) = if queued {
                        (
                            Style::default().fg(self.theme.focused()).bold(),
                            self.theme.success(),
                        )
                    } else {
                        (Style::default().fg(self.theme.entry()), self.theme.muted())
                    };
                    // An entry that cannot be queued is marked as running on its own.
                    let icon = if queued {
                        self.glyph(Icon::Queued)
                    } else if entry.multi_select {
                        self.glyph(Icon::Entry)
                    } else {
                        self.glyph(Icon::Alone)
                    };
                    // Without icons, a queued entry is marked after its name; a pinned one
                    // always is.
                    let pinned = if self.favorites.contains(&entry.id) {
                        " ★"
                    } else {
                        ""
                    };
                    let suffix =
                        format!("{}{pinned}", if queued && !self.icons { " *" } else { "" });
                    let left = format!("{icon}  {}{suffix}", entry.name);
                    let flags = format!("  {} ", entry.flags);
                    // The status word, then where from, dropped first when short of room.
                    let (word, detail, color) = match self.statuses.get(&entry.id) {
                        Some(status) => (
                            status.label().to_string(),
                            status.detail(),
                            self.status_color(status),
                        ),
                        None => (String::new(), String::new(), self.theme.muted()),
                    };
                    let room = content_width.saturating_sub(left.chars().count() + 2);
                    let mut detail = if detail.is_empty() {
                        detail
                    } else {
                        format!("  {detail}")
                    };
                    let width = |detail: &str| {
                        word.chars().count() + detail.chars().count() + flags.chars().count()
                    };
                    if width(&detail) > room {
                        detail.clear();
                    }
                    let flags = if width(&detail) > room {
                        " ".to_string()
                    } else {
                        flags
                    };
                    let right =
                        word.chars().count() + detail.chars().count() + flags.chars().count();
                    // Between them, where it fits: where a search result is
                    // listed, then the description, as the window's rows show.
                    let room = content_width.saturating_sub(left.chars().count() + right + 3);
                    let place = if searching {
                        self.place_label(&entry.id)
                    } else {
                        None
                    };
                    let about = match (place, entry.description.is_empty()) {
                        (Some(place), true) => place,
                        (Some(place), false) => format!("{place} · {}", entry.description),
                        (None, _) => entry.description.clone(),
                    };
                    let about = if about.is_empty() || room < 8 {
                        String::new()
                    } else {
                        format!("  {}", clipped(&about, room))
                    };
                    let gap = " ".repeat(
                        content_width
                            .saturating_sub(left.chars().count() + about.chars().count() + right),
                    );
                    Line::from(vec![
                        Span::styled(icon.to_string(), Style::default().fg(icon_color)),
                        Span::styled(format!("  {}{suffix}", entry.name), style),
                        Span::styled(about, Style::default().fg(self.theme.faint())),
                        Span::raw(gap),
                        Span::styled(word, Style::default().fg(color)),
                        Span::styled(detail, Style::default().fg(self.theme.faint())),
                        Span::styled(flags, Style::default().fg(self.theme.muted())),
                    ])
                }
            })
            .collect();
        self.fill(frame, area);
        let list = List::new(self.measure(lines))
            .block(block)
            .highlight_style(self.highlight(focused))
            .highlight_symbol(marker)
            .scroll_padding(1);
        bordered_list(frame, list, area, &mut self.main_state, &self.hits);
    }

    pub(super) fn draw_plan(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Install Plan",
            "Review planned changes before they run.",
        );
        if self.queue.is_empty() {
            frame.render_widget(
                Paragraph::new(
                    "Nothing is queued. In an area, press Space on an entry to add it here.",
                )
                .style(Style::default().fg(self.theme.muted()))
                .wrap(Wrap { trim: true }),
                inner,
            );
            return;
        }
        let ids: Vec<String> = self.queue.iter().map(|entry| entry.id.clone()).collect();
        let fresh = self
            .planned
            .as_ref()
            .is_some_and(|cache| cache.ids == ids && cache.made.elapsed() < PLAN_KEPT);
        if !fresh {
            let result = plan(self.queue.iter(), &self.platform, &|id| {
                self.catalog.find(id)
            })
            .map_err(|error| error.to_string());
            let findings = match &result {
                Ok(planned) => self.findings(planned),
                Err(_) => Vec::new(),
            };
            self.planned = Some(PlanCache {
                ids,
                made: std::time::Instant::now(),
                result,
                findings,
            });
        }
        let planned = self
            .planned
            .as_ref()
            .map(|cache| cache.result.clone())
            .unwrap_or_else(|| Err("not planned".to_string()));
        let summary = match &planned {
            Ok(planned) => {
                let mut lines = vec![format!(
                    "{} step{}, risk {}{}",
                    planned.steps.len(),
                    if planned.steps.len() == 1 { "" } else { "s" },
                    planned.risk.label(),
                    if planned.privileged {
                        ", asks for your password"
                    } else {
                        ""
                    }
                )];
                if !planned.requirements.is_empty() {
                    lines.push(format!("Needs: {}", planned.requirements.join(", ")));
                }
                lines.extend(
                    self.planned
                        .iter()
                        .flat_map(|cache| cache.findings.iter().map(preflight::Finding::line)),
                );
                lines.extend(planned.notes.iter().cloned());
                lines.extend(
                    planned
                        .blocked
                        .iter()
                        .map(|blocked| format!("Not run: {blocked}")),
                );
                lines
            }
            Err(error) => vec![format!("Cannot plan: {error}")],
        };
        let summary_height = summary.len() as u16 + 2;
        let parts = Layout::vertical([
            Constraint::Length(summary_height),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);
        self.fill(frame, parts[0]);
        frame.render_widget(
            Paragraph::new(Text::from(
                summary.into_iter().map(Line::from).collect::<Vec<_>>(),
            ))
            .block(self.card("Summary"))
            .wrap(Wrap { trim: true }),
            parts[0],
        );
        let focused = self.focus == Focus::Main;
        let steps = planned.map(|planned| planned.steps).unwrap_or_default();
        let block = self.card(&format!("Queued entries · {}", self.queue.len()));
        let marker = self.marker(focused);
        let width = (block.inner(parts[2]).width as usize).saturating_sub(marker.chars().count());
        // Each entry in two lines, as the window's rows: its number, name,
        // status and flags, then where it is listed, its kind and its step.
        let items: Vec<ListItem> = self
            .queue
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let command = steps
                    .iter()
                    .find(|step| step.id() == entry.id)
                    .map(|step| step.display())
                    .unwrap_or_else(|| "nothing to do".to_string());
                let icon = self.icon_gap(Icon::Queued);
                let number = format!("{}. ", index + 1);
                let (word, color) = match self.statuses.get(&entry.id) {
                    Some(status) => (status.label().to_string(), self.status_color(status)),
                    None => (String::new(), self.theme.muted()),
                };
                let flags = format!("  {} ", entry.flags.trim());
                let left = icon.chars().count() + number.len() + entry.name.chars().count();
                let gap = " ".repeat(
                    width.saturating_sub(left + word.chars().count() + flags.chars().count()),
                );
                let indent = " ".repeat(icon.chars().count() + number.len());
                let mut about = vec![kind_label(&entry.action).to_string()];
                if let Some(place) = self.place_label(&entry.id) {
                    about.insert(0, place);
                }
                let about = about.join(" · ");
                let room = width.saturating_sub(indent.len() + about.chars().count() + 3);
                ListItem::new(Text::from(vec![
                    Line::from(vec![
                        Span::styled(icon, Style::default().fg(self.theme.success())),
                        Span::styled(number, Style::default().fg(self.theme.faint())),
                        Span::styled(
                            entry.name.clone(),
                            Style::default().fg(self.theme.entry()).bold(),
                        ),
                        Span::raw(gap),
                        Span::styled(word, Style::default().fg(color)),
                        Span::styled(flags, Style::default().fg(self.theme.muted())),
                    ]),
                    Line::from(vec![
                        Span::raw(indent),
                        Span::styled(about, Style::default().fg(self.theme.faint())),
                        Span::styled(
                            format!("   {}", clipped(&command, room)),
                            Style::default().fg(self.theme.muted()),
                        ),
                    ]),
                ]))
            })
            .collect();
        self.fill(frame, parts[2]);
        let list = List::new(self.measure(items))
            .block(block)
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused));
        bordered_list(frame, list, parts[2], &mut self.main_state, &self.hits);
    }

    /// Pin the selected entry, or unpin it, and remember it.
    pub(super) fn toggle_favorite(&mut self) {
        let Some(entry) = self.selected_entry().cloned() else {
            return;
        };
        match self.favorites.iter().position(|id| *id == entry.id) {
            Some(position) => {
                self.favorites.remove(position);
            }
            None => self.favorites.push(entry.id.clone()),
        }
        if let Some(path) = &self.favorites_path
            && let Err(error) = settings::save_favorites(path, &self.favorites)
        {
            tracing::warn!(%error, "the favorites were not saved");
        }
        if self.screen == Screen::Favorites {
            self.update_rows();
        }
    }

    /// The entries run recently, newest first, from the history.
    pub(super) fn reload_recent(&mut self) {
        self.take_recent(history::read(50).unwrap_or_default());
    }

    pub(super) fn take_recent(&mut self, records: Vec<history::Record>) {
        let mut recent: Vec<String> = Vec::new();
        for record in records {
            for id in record.entries {
                if !recent.contains(&id) && self.catalog.find(&id).is_some() {
                    recent.push(id);
                }
            }
            if recent.len() >= 10 {
                break;
            }
        }
        recent.truncate(10);
        self.recent = recent;
    }

    pub(super) fn filter_hint(&self) -> &'static str {
        match self.filter {
            Filter::All => "Show: all",
            Filter::Installed => "Show: installed",
            Filter::Updates => "Show: updates",
        }
    }

    pub(super) fn draw_favorites(&mut self, frame: &mut Frame, area: Rect) {
        let content = self.page(
            frame,
            area,
            "Favorites",
            "Entries pinned with *, then the ones run recently.",
        );
        if self.rows.is_empty() {
            self.fill(frame, content);
            frame.render_widget(
                Paragraph::new(if self.filter == Filter::All {
                    "Nothing here yet. Press * on an entry anywhere to pin it here; entries you run appear here too."
                } else {
                    "Nothing here matches the filter; i changes it."
                })
                .block(self.card("Favorites"))
                .style(Style::default().fg(self.theme.muted()))
                .wrap(Wrap { trim: true }),
                content,
            );
            return;
        }
        self.draw_list(frame, content);
    }

    pub(super) fn toggle_queue(&mut self) {
        let Some(entry) = self.selected_entry().cloned() else {
            return;
        };
        self.toggle_queued(entry);
    }

    /// Add an entry to the Install Plan, or take it out.
    pub(super) fn toggle_queued(&mut self, entry: Entry) {
        if !entry.multi_select {
            self.popup = Some(Popup::Text(TextPopup::new(
                entry.name.clone(),
                "This entry runs on its own and cannot be queued with others. Press Enter to run it.",
                true,
            )));
            return;
        }
        if let Some(position) = self.queue.iter().position(|other| other.id == entry.id) {
            self.queue.remove(position);
        } else {
            self.queue.push(entry);
        }
    }

    /// Move the selected Install Plan entry earlier (-1) or later (1), as
    /// the window's arrows do; the selection follows it.
    pub(super) fn move_queued(&mut self, by: isize) {
        let Some(selected) = self.main_state.selected() else {
            return;
        };
        let Some(target) = selected.checked_add_signed(by) else {
            return;
        };
        if selected >= self.queue.len() || target >= self.queue.len() {
            return;
        }
        self.queue.swap(selected, target);
        self.main_state.select(Some(target));
    }

    /// Queue the selected profile's entries one by one, those not queued
    /// yet, to change the plan before it runs; then show the Install Plan.
    pub(super) fn queue_profile_entries(&mut self) {
        let Some(entry) = self.selected_entry().cloned() else {
            return;
        };
        let Action::Profile { entries } = &entry.action else {
            return;
        };
        let mut added = 0;
        for id in entries {
            if let Some(found) = self.catalog.find(id).cloned()
                && found.multi_select
                && !self.queue.iter().any(|queued| queued.id == found.id)
            {
                self.queue.push(found);
                added += 1;
            }
        }
        self.open_screen(Screen::Plan);
        self.focus = Focus::Main;
        let text = if added == 0 {
            "Its entries are already in the Install Plan.".to_string()
        } else {
            format!("{added} of its entries were added to the Install Plan. Nothing has run.")
        };
        self.popup = Some(Popup::Text(TextPopup::new(entry.name.clone(), text, true)));
    }

    pub(super) fn preview(&mut self) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        let text = match &entry.action {
            Action::Script { file, .. } => std::fs::read_to_string(file)
                .unwrap_or_else(|error| format!("The script cannot be read: {error}")),
            Action::Command { line } => line.clone(),
            _ => match plan([entry], &self.platform, &|id| self.catalog.find(id)) {
                Ok(planned) => {
                    let mut lines = planned.notes.clone();
                    for step in &planned.steps {
                        lines.push(step.display());
                        lines.extend(step.fallbacks());
                    }
                    lines.join("\n")
                }
                Err(error) => error.to_string(),
            },
        };
        self.popup = Some(Popup::Text(TextPopup::new(
            format!("Preview: {}", entry.name),
            text,
            false,
        )));
    }

    pub(super) fn describe(&mut self) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        // As the window's details: where it is, what kind, whether it asks.
        let mut tags = vec![kind_label(&entry.action).to_string()];
        if matches!(
            entry.action,
            Action::Script {
                interactive: true,
                ..
            }
        ) {
            tags.push("Asks questions".to_string());
        }
        if !entry.multi_select {
            tags.push("Runs on its own".to_string());
        }
        let mut text = String::new();
        if let Some(place) = self.place_label(&entry.id) {
            text.push_str(&format!("{place}\n"));
        }
        text.push_str(&tags.join(" · "));
        text.push_str("\n\n");
        text.push_str(if entry.description.is_empty() {
            "This entry has no description."
        } else {
            &entry.description
        });
        if let Some(status) = self.statuses.get(&entry.id) {
            let detail = status.detail();
            text.push_str(&format!("\n\nStatus: {}", status.label()));
            if !detail.is_empty() {
                text.push_str(&format!("\nInstalled from: {detail}"));
            }
            if let Status::Update { .. } = status {
                text.push_str(
                    "\nThe repository metadata on this machine has a newer version: u updates it (on Arch, with the whole system).",
                );
            }
        }
        if let Action::Profile { entries } = &entry.action
            && !entries.is_empty()
        {
            text.push_str("\n\nRuns, in order:");
            for (number, id) in entries.iter().enumerate() {
                let name = self
                    .catalog
                    .find(id)
                    .map_or(id.as_str(), |found| found.name.as_str());
                text.push_str(&format!("\n  {}. {name}", number + 1));
            }
            text.push_str("\n\ne puts its entries in the Install Plan one by one, to change the plan before it runs.");
        }
        let meanings = flag_meanings(&entry.flags);
        if !meanings.is_empty() {
            text.push_str("\n\nWhat it does:");
            for (flag, meaning) in meanings {
                text.push_str(&format!("\n  {flag:<4}{meaning}"));
            }
        }
        self.popup = Some(Popup::Text(TextPopup::new(entry.name.clone(), text, true)));
    }

    /// Every screen, action and entry, with what each does.
    pub(super) fn palette_choices(&self) -> (Vec<Choice>, Vec<Act>) {
        let mut choices = Vec::new();
        let mut acts = Vec::new();
        let mut add = |label: String, detail: &str, keywords: &str, listed: bool, act: Act| {
            choices.push(Choice {
                label,
                detail: detail.to_string(),
                keywords: keywords.to_string(),
                listed,
            });
            acts.push(act);
        };
        for item in &self.menu {
            add(
                format!("Open {}", item.label),
                "screen",
                "go to show",
                true,
                Act::Open(item.screen),
            );
        }
        let actions: [(&str, &str, Act); 18] = [
            (
                "Check updates online",
                "updates refresh",
                Act::UpdatesOnline,
            ),
            (
                "Run the health checks",
                "doctor diagnostics",
                Act::Doctor(None),
            ),
            (
                "Run DNS diagnostics",
                "doctor dns name resolution",
                Act::Doctor(Some("dns")),
            ),
            (
                "Check the network",
                "doctor internet connection route",
                Act::Doctor(Some("network")),
            ),
            (
                "Check disk space",
                "doctor free storage",
                Act::Doctor(Some("disk-space")),
            ),
            (
                "Check failed services",
                "doctor systemd units",
                Act::Doctor(Some("services")),
            ),
            (
                "Save this machine as a baseline",
                "drift snapshot record",
                Act::SaveBaseline,
            ),
            ("Run the Install Plan", "queue", Act::RunPlan),
            ("Clear the Install Plan", "queue empty", Act::ClearPlan),
            (
                "Save the Install Plan as a profile",
                "queue",
                Act::SaveProfile,
            ),
            ("Import a profile", "profile file", Act::ImportProfile),
            ("Make a bug report", "issue problem report", Act::Report),
            (
                "Update every entry with an update",
                "upgrade packages",
                Act::UpdateEntries,
            ),
            (
                "Check again what is installed",
                "status refresh packages installed",
                Act::CheckStatuses,
            ),
            ("Change the theme", "colors colors look", Act::Theme),
            (
                "Set up this machine",
                "wizard first run workstation",
                Act::Wizard,
            ),
            (
                if self.icons {
                    "Turn icons off"
                } else {
                    "Turn icons on"
                },
                "nerd font plain",
                Act::Icons,
            ),
            (
                if self.mouse {
                    "Turn the mouse off"
                } else {
                    "Turn the mouse on"
                },
                "mouse click wheel scroll select text",
                Act::Mouse,
            ),
        ];
        for (label, keywords, act) in actions {
            add(label.to_string(), "action", keywords, true, act);
        }
        add(
            "Show the keys".to_string(),
            "action",
            "help",
            true,
            Act::Help,
        );
        add(
            "Quit Helix Toolbox".to_string(),
            "action",
            "exit close",
            true,
            Act::Quit,
        );
        for tab in &self.catalog.tabs {
            for entry in tab.entries() {
                let verb = match (&entry.action, self.statuses.get(&entry.id)) {
                    (_, Some(Status::Absent { .. })) => "Install",
                    (Action::Package { .. }, None) => "Install",
                    (Action::Profile { .. }, _) => "Run profile",
                    _ => "Run",
                };
                add(
                    format!("{verb} {}", entry.name),
                    &tab.name,
                    &entry.id,
                    false,
                    Act::Entry(entry.id.clone()),
                );
                add(
                    format!("Inspect {}", entry.name),
                    "trust and testing",
                    &entry.id,
                    false,
                    Act::Inspect(entry.id.clone()),
                );
            }
        }
        (choices, acts)
    }

    pub(super) fn open_palette(&mut self) {
        let (choices, acts) = self.palette_choices();
        self.palette_acts = acts;
        self.popup = Some(Popup::Palette(Palette::new(choices)));
    }

    /// Do what a palette choice says; false quits.
    pub(super) fn act(&mut self, act: Act) -> bool {
        match act {
            Act::Open(screen) => {
                self.open_screen(screen);
                if screen != Screen::Dashboard {
                    self.focus = Focus::Main;
                }
            }
            Act::Entry(id) => {
                if let Some(entry) = self.catalog.find(&id).cloned() {
                    self.confirm(vec![entry]);
                }
            }
            Act::Inspect(id) => self.inspect(&id),
            Act::UpdatesOnline => {
                self.open_screen(Screen::Updates);
                self.focus = Focus::Main;
                self.gather_updates(true);
            }
            Act::Doctor(wanted) => {
                self.open_screen(Screen::Doctor);
                self.focus = Focus::Main;
                if let Some(wanted) = wanted {
                    if self.diagnoses.is_empty() || self.examining.is_some() {
                        self.doctor_wanted = Some(wanted);
                    } else {
                        self.show_check(wanted);
                    }
                } else if !self.diagnoses.is_empty() {
                    self.examine();
                }
            }
            Act::SaveBaseline => {
                self.open_screen(Screen::Drift);
                self.focus = Focus::Main;
                self.popup = Some(Popup::BaselineName(InputPopup::new(
                    "Save this machine as a baseline",
                    "A name for the baseline (it is saved in ~/.config/helix-toolbox/baselines):",
                )));
            }
            Act::Wizard => self.open_wizard(false),
            Act::RunPlan => {
                self.open_screen(Screen::Plan);
                self.focus = Focus::Main;
                self.confirm(self.queue.clone());
            }
            Act::ClearPlan => {
                self.queue.clear();
                self.clamp_main();
            }
            Act::SaveProfile => {
                self.open_screen(Screen::Plan);
                self.focus = Focus::Main;
                if !self.queue.is_empty() {
                    self.popup = Some(Popup::ProfileName(InputPopup::new(
                        "Save as a profile",
                        "A name for this profile (it is saved in ~/.config/helix-toolbox/profiles):",
                    )));
                }
            }
            Act::ImportProfile => {
                if let Some(index) = self
                    .catalog
                    .tabs
                    .iter()
                    .position(|tab| tab.id == "profiles")
                {
                    self.open_screen(Screen::Area(index));
                    self.focus = Focus::Main;
                    self.handle_main_key(&KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
                }
            }
            Act::Report => self.make_report(),
            Act::CheckStatuses => self.check_again(),
            Act::UpdateEntries => {
                let waiting: Vec<Entry> = self
                    .catalog
                    .entries()
                    .into_iter()
                    .filter(|entry| {
                        matches!(self.statuses.get(&entry.id), Some(Status::Update { .. }))
                    })
                    .cloned()
                    .collect();
                if waiting.is_empty() {
                    self.popup = Some(Popup::Text(TextPopup::new(
                        "Nothing to update",
                        "No package entry has an update in the repository metadata on this machine.",
                        true,
                    )));
                } else {
                    self.update(waiting);
                }
            }
            Act::Theme => {
                self.theme = self.theme.next();
                self.saved_theme = self.theme;
                self.save_look();
            }
            Act::Icons => {
                self.icons = !self.icons;
                self.save_look();
            }
            Act::Mouse => {
                self.mouse = !self.mouse;
                self.save_look();
            }
            Act::Help => {
                self.popup = Some(Popup::Text(TextPopup::new("Keys", HELP, false)));
            }
            Act::Quit => return false,
        }
        true
    }

    /// Select a health check by id (or the first whose id starts with it)
    /// and show its details.
    pub(super) fn show_check(&mut self, wanted: &str) {
        let found = self
            .diagnoses
            .iter()
            .position(|diagnosis| diagnosis.id == wanted)
            .or_else(|| {
                self.diagnoses
                    .iter()
                    .position(|diagnosis| diagnosis.id.starts_with(wanted))
            });
        if let Some(index) = found {
            self.main_state.select(Some(index));
            self.describe_diagnosis();
        }
    }

    /// The change preview of a plan.
    pub(super) fn show_effects(&mut self, planned: &Plan, from_confirmation: bool) {
        let scripts = self.catalog.root().join("scripts");
        let render =
            |arguments: &[String]| helix_toolbox_engine::preview::render_with(&scripts, arguments);
        let look = helix_toolbox_engine::preview::Look {
            system: &doctor::Here,
            render: &render,
            pinned: &|name| helix_toolbox_engine::preview::pinned_source(&scripts, name),
        };
        let steps = helix_toolbox_engine::preview::preview(
            planned,
            &|id| self.catalog.find(id),
            &self.platform,
            &look,
        );
        let download = helix_toolbox_engine::preview::sources_for(planned)
            .into_iter()
            .any(|name| {
                matches!(
                    helix_toolbox_engine::preview::pinned_source(&scripts, name),
                    helix_toolbox_engine::preview::Pinned::Missing { .. }
                )
            });
        self.popup = Some(Popup::Effects(EffectsPopup::new(
            steps,
            from_confirmation,
            planned.clone(),
            download,
        )));
    }

    /// Download the pinned sources a plan reads, which changes nothing
    /// else, then show its changes again.
    pub(super) fn download_sources(&mut self, planned: &Plan, from_confirmation: bool) {
        let scripts = self.catalog.root().join("scripts");
        for name in helix_toolbox_engine::preview::sources_for(planned) {
            if let Err(why) = helix_toolbox_engine::preview::fetch_source(&scripts, name) {
                self.popup = Some(Popup::Text(TextPopup::new(
                    "Cannot download",
                    format!("{name}: {why}"),
                    true,
                )));
                return;
            }
        }
        self.show_effects(planned, from_confirmation);
    }

    /// Check the selected entry now, read-only: what it installs and its
    /// checks of files, settings and services.
    pub(super) fn check_selected(&mut self) {
        let Some(entry) = self.selected_entry().cloned() else {
            return;
        };
        let rebooted = helix_toolbox_engine::verification::rebooted_since(
            &doctor::Here,
            &self.run_history(),
            &entry.id,
        );
        let checks = helix_toolbox_engine::verification::check_entry(
            &entry,
            &self.platform,
            rebooted,
            &helix_toolbox_engine::Probe::holds,
            &helix_toolbox_platform::binary_exists,
            &doctor::Here,
        );
        let text = if checks.is_empty() {
            format!(
                "{} declares no checks and installs nothing Helix Toolbox can find, so there is nothing to check.",
                entry.name
            )
        } else {
            let failed = checks.iter().filter(|check| !check.passed).count();
            let mut text = match failed {
                0 => "Everything that can be checked here is as it should be.\n\n".to_string(),
                1 => "One check found it not as it should be.\n\n".to_string(),
                count => format!("{count} checks found it not as it should be.\n\n"),
            };
            for check in &checks {
                text.push_str(&format!(
                    "{}: {}\n  {}\n",
                    check.label(),
                    check.name,
                    check.detail
                ));
            }
            text
        };
        self.popup = Some(Popup::Text(TextPopup::new(
            format!("Checks: {}", entry.name),
            text,
            true,
        )));
    }
}
