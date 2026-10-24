//! Keys and the mouse.

use super::*;

impl App {
    /// Returns false when Helix Toolbox should exit.
    /// Whether the terminal should report the mouse.
    pub fn mouse(&self) -> bool {
        self.mouse
    }

    /// A click or the wheel, done as the keys that do the same, so nothing
    /// a click starts skips its review. Returns false to quit, as a click
    /// on the q hint asks.
    pub fn handle_mouse(&mut self, event: &MouseEvent) -> bool {
        if !self.mouse {
            return true;
        }
        let target = self.hits.borrow().at(event.column, event.row);
        match event.kind {
            MouseEventKind::ScrollDown => self.wheel(true, event),
            MouseEventKind::ScrollUp => self.wheel(false, event),
            MouseEventKind::Down(MouseButton::Left) => match target {
                Some(Target::Key(key)) => self.handle_key(&key),
                // Behind a popup only its hints answer.
                _ if self.popup.is_some() => true,
                Some(Target::Menu(index)) => {
                    let screen = self.menu[index].screen;
                    if screen == self.screen && self.focus == Focus::Sidebar {
                        self.press(KeyCode::Enter)
                    } else {
                        if screen != self.screen {
                            self.open_screen(screen);
                        }
                        self.focus = Focus::Sidebar;
                        true
                    }
                }
                Some(Target::Row(index)) => self.click_row(index),
                None => true,
            },
            _ => true,
        }
    }

    pub(in crate::tui::app) fn press(&mut self, code: KeyCode) -> bool {
        self.handle_key(&KeyEvent::new(code, KeyModifiers::NONE))
    }

    /// Give the main list the focus, as the keys would: Enter from the
    /// sidebar or the search line.
    fn focus_main(&mut self) -> bool {
        match self.focus {
            Focus::Main => true,
            Focus::Sidebar | Focus::Search => self.press(KeyCode::Enter),
        }
    }

    /// A click on a row selects it, by the arrow keys; a click on the row
    /// already selected in the focused list is Enter.
    fn click_row(&mut self, index: usize) -> bool {
        if self.focus != Focus::Main {
            return self.focus_main() && self.select_by_keys(index);
        }
        let selected = self.main_state.selected().unwrap_or(0);
        if index == selected {
            return self.press(KeyCode::Enter);
        }
        self.select_by_keys(index)
    }

    fn select_by_keys(&mut self, index: usize) -> bool {
        for _ in 0..self.main_len() {
            let before = self.main_state.selected().unwrap_or(0);
            if before == index {
                break;
            }
            let code = if index > before {
                KeyCode::Down
            } else {
                KeyCode::Up
            };
            if !self.press(code) {
                return false;
            }
            if self.main_state.selected().unwrap_or(0) == before {
                break;
            }
        }
        true
    }

    /// The wheel scrolls what is under it: a popup's text or list, the
    /// output of a run, or the main list's selection.
    pub(in crate::tui::app) fn wheel(&mut self, down: bool, event: &MouseEvent) -> bool {
        let code = if down { KeyCode::Down } else { KeyCode::Up };
        match &mut self.popup {
            // Keys go to the command running, so the wheel scrolls its output instead.
            Some(Popup::Running(running)) => {
                running.scroll(if down { -3 } else { 3 });
                true
            }
            // A typed answer has nothing to scroll.
            Some(
                Popup::ProfileName(_)
                | Popup::BaselineName(_)
                | Popup::FirmwareWord(_)
                | Popup::LookName(_)
                | Popup::Ask(..),
            ) => true,
            Some(_) => self.press(code),
            None if self.hits.borrow().in_main(event.column, event.row) => {
                self.focus_main() && self.press(code)
            }
            None => true,
        }
    }

    pub fn handle_key(&mut self, key: &KeyEvent) -> bool {
        let control_c =
            key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c');
        if let Some(Popup::Text(popup) | Popup::HardwareReport(popup, _)) = &mut self.popup
            && key.code == KeyCode::Char('c')
            && let Some(text) = popup.copyable()
        {
            let said = (self.copier)(text)
                .unwrap_or_else(|error| format!("The report could not be copied: {error}"));
            popup.noted(said);
            return true;
        }
        match &mut self.popup {
            Some(Popup::Text(popup)) => {
                if popup.handle_key(key) {
                    self.popup = None;
                }
                return true;
            }
            Some(Popup::HardwareReport(popup, url)) => {
                if matches!(key.code, KeyCode::Char('o') | KeyCode::Char('O')) {
                    let text = match (self.opener)(url) {
                        Ok(()) => "The issue is open in your browser, filled in from the report. Say there what you checked, then submit it.".to_string(),
                        Err(error) => format!(
                            "It could not be opened: {error}\n\nThe link is in ~/.local/state/helix-toolbox/hardware-report.md, or helix-toolbox report --worked prints it."
                        ),
                    };
                    self.popup = Some(Popup::Text(TextPopup::new("Hardware report", text, true)));
                } else if popup.handle_key(key) {
                    self.popup = None;
                }
                return true;
            }
            Some(Popup::ProfileName(popup)) => {
                if control_c {
                    return false;
                }
                match popup.handle_key(key) {
                    Answer::Typing => {}
                    Answer::Canceled => self.popup = None,
                    Answer::Given(name) => self.save_profile(&name),
                }
                return true;
            }
            Some(Popup::Wizard(popup)) => {
                if control_c {
                    return false;
                }
                match popup.handle_key(key) {
                    WizardOutcome::Open => {}
                    WizardOutcome::Closed => self.close_wizard(),
                    WizardOutcome::Chosen(index) => {
                        let picks = self.wizard_picks(index);
                        if let Some(Popup::Wizard(popup)) = &mut self.popup {
                            popup.pick(picks);
                        }
                    }
                    WizardOutcome::Queue(ids) => {
                        self.close_wizard();
                        self.queue_from_wizard(&ids);
                    }
                }
                return true;
            }
            Some(Popup::Ask(popup, asked)) => {
                if control_c {
                    return false;
                }
                match popup.handle_key(key) {
                    Answer::Typing => {}
                    Answer::Canceled => self.popup = None,
                    Answer::Given(text) => {
                        let asked = asked.clone();
                        self.popup = None;
                        self.answered(asked, &text);
                    }
                }
                return true;
            }
            Some(Popup::LookName(popup)) => {
                if control_c {
                    return false;
                }
                match popup.handle_key(key) {
                    Answer::Typing => {}
                    Answer::Canceled => self.popup = None,
                    Answer::Given(name) => {
                        self.with_screens(|screens, cx| screens.desktop.save_look_as(cx, &name))
                    }
                }
                return true;
            }
            Some(Popup::FirmwareWord(popup)) => {
                if control_c {
                    return false;
                }
                match popup.handle_key(key) {
                    Answer::Typing => {}
                    Answer::Canceled => {
                        self.popup = None;
                        self.with_screens(|screens, cx| {
                            screens.hardware.confirm_firmware(cx, None)
                        });
                    }
                    Answer::Given(word) => {
                        self.popup = None;
                        self.with_screens(|screens, cx| {
                            screens.hardware.confirm_firmware(cx, Some(&word))
                        });
                    }
                }
                return true;
            }
            Some(Popup::BaselineName(popup)) => {
                if control_c {
                    return false;
                }
                match popup.handle_key(key) {
                    Answer::Typing => {}
                    Answer::Canceled => self.popup = None,
                    Answer::Given(name) => {
                        self.with_screens(|screens, cx| screens.drift.save_baseline(cx, &name))
                    }
                }
                return true;
            }
            Some(Popup::Effects(popup)) => {
                if control_c {
                    return false;
                }
                match popup.handle_key(key) {
                    EffectsOutcome::Open => {}
                    EffectsOutcome::Closed => {
                        let back = popup.from_confirmation;
                        self.popup = None;
                        if back && let Some((header, lines)) = &self.confirming {
                            self.popup = Some(Popup::Confirm(ConfirmPopup::new(header, lines)));
                        }
                    }
                    EffectsOutcome::Download => {
                        let (planned, back) = (popup.plan.clone(), popup.from_confirmation);
                        self.download_sources(&planned, back);
                    }
                }
                return true;
            }
            Some(Popup::Palette(popup)) => {
                match popup.handle_key(key) {
                    Outcome::Open => {}
                    Outcome::Closed => self.popup = None,
                    Outcome::Chosen(index) => {
                        self.popup = None;
                        if let Some(act) = self.palette_acts.get(index).cloned() {
                            return self.act(act);
                        }
                    }
                }
                return true;
            }
            Some(Popup::Confirm(popup)) => {
                if control_c {
                    return false;
                }
                popup.handle_key(key);
                match popup.decision {
                    Decision::Run => {
                        self.popup = None;
                        self.run_pending();
                    }
                    Decision::Cancel => {
                        self.popup = None;
                        self.pending = None;
                        self.confirming = None;
                    }
                    Decision::Effects => {
                        if let Some(planned) =
                            self.pending.as_ref().map(|session| session.plan().clone())
                        {
                            self.show_effects(&planned, true);
                        }
                    }
                    Decision::Pending => {}
                }
                return true;
            }
            Some(Popup::Running(popup)) => {
                if popup.handle_key(key) {
                    // What is installed may have changed.
                    self.planned = None;
                    let remaining = popup.remaining(&|id| self.catalog.find(id).is_some());
                    let read_again = popup.read_again();
                    let summary = popup.summary();
                    self.popup = popup
                        .take_problem()
                        .map(|problem| Popup::Text(TextPopup::new("Not recorded", problem, true)));
                    if std::mem::take(&mut self.ran_from_plan) {
                        // The queue keeps what did not finish.
                        self.queue = remaining
                            .iter()
                            .filter_map(|id| self.catalog.find(id).cloned())
                            .collect();
                        self.clamp_main();
                        if !self.queue.is_empty() && self.popup.is_none() {
                            self.popup = Some(Popup::Text(TextPopup::new(
                                "Not everything finished",
                                format!(
                                    "{summary}\n\nWhat did not finish stays in the Install Plan; Enter there runs it again."
                                ),
                                true,
                            )));
                        }
                    }
                    for reading in read_again {
                        self.read_again(reading);
                    }
                }
                return true;
            }
            None => {}
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('k') {
            self.open_palette();
            return true;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('r') {
            self.check_again();
            return true;
        }
        if self.focus == Focus::Search {
            self.handle_search_key(key);
            return true;
        }
        if control_c || key.code == KeyCode::Char('q') {
            return false;
        }
        match key.code {
            KeyCode::Tab => self.next_menu(1),
            KeyCode::BackTab => self.next_menu(-1),
            KeyCode::Char('/') => {
                if self.current_area().is_none() {
                    self.open_screen(Screen::Area(self.first_area()));
                }
                // Escape comes back to this row.
                if self.search.is_empty() {
                    self.before_search = self.main_state.selected();
                }
                self.focus = Focus::Search;
                self.cursor = self.search.len();
            }
            KeyCode::Char('?') => {
                self.popup = Some(Popup::Text(TextPopup::new("Keys", HELP, false)));
            }
            KeyCode::Char('t') | KeyCode::Char('T') => {
                self.theme = self.theme.next();
                self.saved_theme = self.theme;
                self.save_look();
            }
            KeyCode::Char('g') | KeyCode::Char('G') if matches!(self.screen, Screen::Area(_)) => {
                self.popup = Some(Popup::Text(TextPopup::new("Flags", FLAGS_GUIDE, true)));
            }
            _ => match self.focus {
                Focus::Sidebar => self.handle_sidebar_key(key),
                Focus::Main => self.handle_main_key(key),
                Focus::Search => {}
            },
        }
        true
    }

    pub(in crate::tui::app) fn first_area(&self) -> usize {
        self.menu
            .iter()
            .find_map(|item| match item.screen {
                Screen::Area(index) => Some(index),
                _ => None,
            })
            .unwrap_or(0)
    }

    fn handle_sidebar_key(&mut self, key: &KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.next_menu(1),
            KeyCode::Char('k') | KeyCode::Up => self.next_menu(-1),
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => self.focus = Focus::Main,
            _ => {}
        }
    }

    fn next_menu(&mut self, step: isize) {
        let count = self.menu.len() as isize;
        let current = self.menu_state.selected().unwrap_or(0) as isize;
        let next = ((current + step).rem_euclid(count)) as usize;
        let screen = self.menu[next].screen;
        self.open_screen(screen);
    }

    pub(in crate::tui::app) fn handle_main_key(&mut self, key: &KeyEvent) {
        // The Install Plan's order: K or Shift-Up earlier, J or Shift-Down later.
        if self.screen == Screen::Plan {
            let shifted = key.modifiers.contains(KeyModifiers::SHIFT);
            let by = match key.code {
                KeyCode::Char('K') => Some(-1),
                KeyCode::Char('J') => Some(1),
                KeyCode::Up if shifted => Some(-1),
                KeyCode::Down if shifted => Some(1),
                _ => None,
            };
            if let Some(by) = by {
                self.move_queued(by);
                return;
            }
        }
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            _ => {}
        }
        match self.screen {
            Screen::Dashboard => match key.code {
                KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                    let selected = self.main_state.selected().unwrap_or(0);
                    let quick = QUICK_ACTIONS
                        .get(selected)
                        .map_or(Quick::Applications, |action| action.3);
                    let screen = match quick {
                        Quick::Applications => Screen::Area(self.first_area()),
                        Quick::Plan => Screen::Plan,
                        Quick::Doctor => Screen::Doctor,
                        Quick::Favorites => Screen::Favorites,
                        Quick::History => Screen::History,
                        Quick::Wizard => return self.open_wizard(false),
                        Quick::Palette => return self.open_palette(),
                    };
                    self.open_screen(screen);
                }
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Area(_) | Screen::Favorites => match key.code {
                KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => self.enter(),
                KeyCode::Char('*') => self.toggle_favorite(),
                KeyCode::Char('i') => {
                    self.filter = self.filter.next();
                    self.update_rows();
                    self.main_state.select(Some(0));
                }
                KeyCode::Char('h') | KeyCode::Left => self.back(),
                KeyCode::Char('p') | KeyCode::Char('P') => self.preview(),
                KeyCode::Char('d') | KeyCode::Char('D') => self.describe(),
                KeyCode::Char('v') => {
                    if let Some(id) = self.selected_entry().map(|entry| entry.id.clone()) {
                        self.inspect(&id);
                    }
                }
                KeyCode::Char(' ') => self.toggle_queue(),
                KeyCode::Char('e') => self.queue_profile_entries(),
                KeyCode::Char('c') => self.check_selected(),
                KeyCode::Char('o') if self.in_profiles() => {
                    self.popup = Some(Popup::Ask(
                        InputPopup::new(
                            "Import a profile",
                            "The profile file to add to your profiles, such as ~/Downloads/work.toml:",
                        )
                        .path()
                        .answering("Import"),
                        Asked::ImportProfile,
                    ));
                }
                KeyCode::Char('x') => {
                    if let Some(entry) = self.selected_entry().filter(|entry| own_profile(entry)) {
                        let slug = entry.id.trim_start_matches(profiles::PREFIX);
                        self.popup = Some(Popup::Ask(
                            InputPopup::new(
                                format!("Export {}", entry.name),
                                "The file to write it to, to share or keep:",
                            )
                            .with_value(format!("~/{slug}.toml"))
                            .path()
                            .answering("Export"),
                            Asked::ExportProfile(entry.id.clone()),
                        ));
                    }
                }
                KeyCode::Delete | KeyCode::Char('X') => {
                    if let Some(entry) = self.selected_entry().filter(|entry| own_profile(entry)) {
                        self.popup = Some(Popup::Ask(
                            InputPopup::new(
                                format!("Delete {}", entry.name),
                                "Its file in ~/.config/helix-toolbox/profiles is removed. Type yes to delete it:",
                            )
                            .answering("Delete"),
                            Asked::DeleteProfile(entry.id.clone()),
                        ));
                    }
                }
                KeyCode::Char('r') | KeyCode::Char('R') => {
                    if let Some(entry) = self.selected_entry().cloned()
                        && removable(&entry)
                    {
                        let planned = plan_removal([&entry], &self.platform);
                        self.confirm_plan(planned);
                    }
                }
                KeyCode::Char('u') => {
                    if let Some(entry) = self.selected_entry().cloned() {
                        self.update(vec![entry]);
                    }
                }
                KeyCode::Char('U') => {
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
                _ => {}
            },
            Screen::Plan => match key.code {
                KeyCode::Enter => self.confirm(self.queue.clone()),
                KeyCode::Char('x') | KeyCode::Delete => {
                    if let Some(selected) = self.main_state.selected()
                        && selected < self.queue.len()
                    {
                        self.queue.remove(selected);
                        self.clamp_main();
                    }
                }
                KeyCode::Char('c') => {
                    self.queue.clear();
                    self.clamp_main();
                }
                KeyCode::Char('e') if !self.queue.is_empty() => {
                    match plan(self.queue.iter(), &self.platform, &|id| {
                        self.catalog.find(id)
                    }) {
                        Ok(planned) => self.show_effects(&planned, false),
                        Err(error) => {
                            self.popup = Some(Popup::Text(TextPopup::new(
                                "Cannot plan",
                                error.to_string(),
                                true,
                            )))
                        }
                    }
                }
                KeyCode::Char('s') if !self.queue.is_empty() => {
                    self.popup = Some(Popup::ProfileName(InputPopup::new(
                        "Save as a profile",
                        "A name for this profile (it is saved in ~/.config/helix-toolbox/profiles):",
                    )));
                }
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Settings => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('l') | KeyCode::Right => {
                    self.change_setting()
                }
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            _ => {
                self.with_tool(|tool, cx| tool.key(cx, key.code));
            }
        }
    }

    fn handle_search_key(&mut self, key: &KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                self.focus = Focus::Main;
                self.main_state.select(Some(0));
            }
            KeyCode::Esc => {
                self.search.clear();
                self.cursor = 0;
                self.focus = Focus::Main;
                self.update_rows();
                self.main_state
                    .select(Some(self.before_search.take().unwrap_or(0)));
                self.clamp_main();
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search.clear();
                self.cursor = 0;
                self.focus = Focus::Main;
                self.update_rows();
            }
            KeyCode::Tab => {
                if let Some(completion) = self.completion.take() {
                    self.search.extend(completion.chars());
                    self.cursor = self.search.len();
                    self.update_rows();
                }
            }
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.search.remove(self.cursor);
                    self.update_rows();
                }
            }
            KeyCode::Delete => {
                if self.cursor < self.search.len() {
                    self.search.remove(self.cursor);
                    self.update_rows();
                }
            }
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.search.len()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.search.len(),
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search.insert(self.cursor, character);
                self.cursor += 1;
                self.update_rows();
            }
            _ => {}
        }
    }

    fn move_selection(&mut self, step: isize) {
        let count = self.main_len();
        if count == 0 {
            return;
        }
        let current = self.main_state.selected().unwrap_or(0) as isize;
        self.main_state
            .select(Some((current + step).clamp(0, count as isize - 1) as usize));
    }

    fn enter(&mut self) {
        match self.selected_row().cloned() {
            Some(Row::Up) => self.back(),
            Some(Row::Group { index, .. }) => {
                let row = self.main_state.selected().unwrap_or(0);
                if let Some(area) = self.area_mut() {
                    area.path.push((index, row));
                }
                self.main_state.select(Some(0));
                self.update_rows();
            }
            Some(Row::Entry(entry)) => self.confirm(vec![*entry]),
            None => {}
        }
    }

    pub(in crate::tui::app) fn back(&mut self) {
        if self.searching() {
            self.search.clear();
            self.cursor = 0;
            self.update_rows();
            self.main_state
                .select(Some(self.before_search.take().unwrap_or(0)));
            self.clamp_main();
            return;
        }
        // A filtered list is flat, so there is no group to leave.
        let popped = if self.filter == Filter::All {
            self.area_mut().and_then(|area| area.path.pop())
        } else {
            None
        };
        match popped {
            Some((_, row)) => {
                self.update_rows();
                self.main_state.select(Some(row));
            }
            None => self.focus = Focus::Sidebar,
        }
    }
}
