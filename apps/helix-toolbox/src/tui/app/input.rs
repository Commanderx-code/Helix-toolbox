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

    pub(super) fn press(&mut self, code: KeyCode) -> bool {
        self.handle_key(&KeyEvent::new(code, KeyModifiers::NONE))
    }

    /// Give the main list the focus, as the keys would: Enter from the
    /// sidebar or the search line.
    pub(super) fn focus_main(&mut self) -> bool {
        match self.focus {
            Focus::Main => true,
            Focus::Sidebar | Focus::Search => self.press(KeyCode::Enter),
        }
    }

    /// A click on a row selects it, by the arrow keys; a click on the row
    /// already selected in the focused list is Enter.
    pub(super) fn click_row(&mut self, index: usize) -> bool {
        if self.focus != Focus::Main {
            return self.focus_main() && self.select_by_keys(index);
        }
        let selected = self.main_state.selected().unwrap_or(0);
        if index == selected {
            return self.press(KeyCode::Enter);
        }
        self.select_by_keys(index)
    }

    pub(super) fn select_by_keys(&mut self, index: usize) -> bool {
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
    pub(super) fn wheel(&mut self, down: bool, event: &MouseEvent) -> bool {
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
                    Answer::Given(name) => self.save_look_as(&name),
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
                        self.firmware_pending = None;
                    }
                    Answer::Given(word) => {
                        self.popup = None;
                        match self.firmware_pending.take() {
                            Some(planned) if word.trim() == "FIRMWARE" => {
                                self.confirm_plan(Ok(planned))
                            }
                            _ => {
                                self.popup = Some(Popup::Text(TextPopup::new(
                                    "Not confirmed",
                                    "The firmware update needs the word FIRMWARE, in capitals. Nothing was changed.",
                                    true,
                                )))
                            }
                        }
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
                    Answer::Given(name) => self.save_baseline(&name),
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
                        if let Some(planned) = self.pending.clone() {
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
                    let unfinished = popup.unfinished();
                    let summary = popup.summary();
                    self.popup = popup
                        .take_problem()
                        .map(|problem| Popup::Text(TextPopup::new("Not recorded", problem, true)));
                    if std::mem::take(&mut self.ran_from_plan) {
                        let mut kept: Vec<Entry> = Vec::new();
                        for id in &unfinished {
                            if let Some(entry) = self.catalog.find(id)
                                && !kept.iter().any(|kept| kept.id == entry.id)
                            {
                                kept.push(entry.clone());
                            }
                        }
                        self.queue = kept;
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
                    self.refresh_statuses();
                    if self.screen == Screen::Updates {
                        self.gather_updates(false);
                    }
                    if self.screen == Screen::History {
                        self.reload_history();
                    }
                    if self.screen == Screen::Services {
                        self.reload_services();
                    }
                    if self.screen == Screen::Storage {
                        self.read_storage();
                    }
                    if self.screen == Screen::Recovery {
                        self.reload_recovery();
                    }
                    if self.screen == Screen::Migration {
                        self.reload_migration();
                    }
                    if self.screen == Screen::Restic {
                        self.read_restic();
                    }
                    if self.screen == Screen::Configs {
                        self.read_configs();
                    }
                    if self.screen == Screen::Repositories {
                        self.reload_repos();
                    }
                    if self.screen == Screen::HomeManager {
                        self.read_home();
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

    pub(super) fn first_area(&self) -> usize {
        self.menu
            .iter()
            .find_map(|item| match item.screen {
                Screen::Area(index) => Some(index),
                _ => None,
            })
            .unwrap_or(0)
    }

    pub(super) fn handle_sidebar_key(&mut self, key: &KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.next_menu(1),
            KeyCode::Char('k') | KeyCode::Up => self.next_menu(-1),
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => self.focus = Focus::Main,
            _ => {}
        }
    }

    pub(super) fn next_menu(&mut self, step: isize) {
        let count = self.menu.len() as isize;
        let current = self.menu_state.selected().unwrap_or(0) as isize;
        let next = ((current + step).rem_euclid(count)) as usize;
        let screen = self.menu[next].screen;
        self.open_screen(screen);
    }

    pub(super) fn handle_main_key(&mut self, key: &KeyEvent) {
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
            Screen::History => match key.code {
                KeyCode::Enter => self.show_log(),
                KeyCode::Char('w') => {
                    if let Some(record) = self
                        .main_state
                        .selected()
                        .and_then(|index| self.records.get(index))
                        .cloned()
                    {
                        self.report_worked(&record, None);
                    }
                }
                KeyCode::Char('r') => self.reload_history(),
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Doctor => match key.code {
                KeyCode::Enter => match self.selected_diagnosis() {
                    Some(_) => self.describe_diagnosis(),
                    None => {
                        let index = self
                            .main_state
                            .selected()
                            .unwrap_or(0)
                            .saturating_sub(self.diagnoses.len());
                        if let Some(entry) = self.doctor_entries().get(index).cloned() {
                            self.confirm(vec![entry]);
                        }
                    }
                },
                KeyCode::Char('f') => self.repair_selected(),
                KeyCode::Char('r') => self.examine(),
                KeyCode::Char('d') | KeyCode::Char('D') => match self.selected_diagnosis() {
                    Some(_) => self.describe_diagnosis(),
                    None => self.describe(),
                },
                // A report row is an entry like any other.
                KeyCode::Char('p') | KeyCode::Char('P') => self.preview(),
                KeyCode::Char('v') => {
                    if let Some(id) = self.selected_entry().map(|entry| entry.id.clone()) {
                        self.inspect(&id);
                    }
                }
                KeyCode::Char(' ') => self.toggle_queue(),
                KeyCode::Char('*') => self.toggle_favorite(),
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Updates => match (key.code, self.selected_update_row()) {
                (KeyCode::Enter | KeyCode::Char('u'), UpdateRow::Entry(entry)) => {
                    self.update(vec![*entry])
                }
                (KeyCode::Char('d') | KeyCode::Char('D'), UpdateRow::Entry(_)) => self.describe(),
                (KeyCode::Enter, UpdateRow::Every) => {
                    let waiting = self.entries_with_updates().into_iter().cloned().collect();
                    self.update(waiting);
                }
                (code, _) => match code {
                    KeyCode::Enter | KeyCode::Char('d') => self.describe_channel(),
                    KeyCode::Char('u') => match self.selected_channel().cloned() {
                        Some(channel) if channel.commands.is_empty() => {
                            self.popup = Some(Popup::Text(TextPopup::new(
                                "Nothing to run",
                                format!("{}: {}", channel.name, channel.note),
                                true,
                            )));
                        }
                        Some(channel) => self.confirm_plan(Ok(updates::plan(&[&channel]))),
                        None => {}
                    },
                    KeyCode::Char('U') => self.update_every_channel(),
                    KeyCode::Char('r') => self.gather_updates(true),
                    KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                    _ => {}
                },
            },
            Screen::Desktop => match key.code {
                KeyCode::Enter | KeyCode::Char('d') => match self.selected_desktop_row().cloned() {
                    Some(DesktopRow::Info { text, details }) => {
                        self.popup = Some(Popup::Text(TextPopup::new(text, details, true)));
                    }
                    Some(DesktopRow::Entry { id, .. }) => {
                        if let Some(entry) = self.catalog.find(&id).cloned() {
                            self.confirm(vec![entry]);
                        }
                    }
                    Some(DesktopRow::Look { slug, .. }) => self.apply_look(&slug),
                    _ => {}
                },
                KeyCode::Char(' ') => {
                    if let Some(DesktopRow::Entry { id, .. }) = self.selected_desktop_row().cloned()
                        && let Some(entry) = self.catalog.find(&id).cloned()
                    {
                        self.toggle_queued(entry);
                        self.reload_desktop();
                    }
                }
                KeyCode::Char('s') => {
                    self.popup = Some(Popup::LookName(InputPopup::new(
                        "Save this look",
                        "A name for the desktop's look (it is saved in ~/.config/helix-toolbox/looks):",
                    )));
                }
                KeyCode::Char('r') => self.reload_desktop(),
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Hardware => match key.code {
                KeyCode::Enter | KeyCode::Char('d') => {
                    if let Some(HardwareRow::Item { text, details, .. }) = self.selected_hardware()
                    {
                        self.popup = Some(Popup::Text(TextPopup::new(
                            text.clone(),
                            details.clone(),
                            true,
                        )));
                    }
                }
                KeyCode::Char('f') => {
                    if let Some(HardwareRow::Item {
                        act: Some(HardwareAct::Firmware(index)),
                        ..
                    }) = self.selected_hardware()
                        && let Some(update) = self
                            .hardware
                            .as_ref()
                            .and_then(|found| found.firmware.updates.get(*index))
                        && let Some(planned) = helix_toolbox_engine::hardware::plan_firmware(update)
                    {
                        let prompt = format!(
                            "Updating firmware can leave {} unusable if it is interrupted. Keep the machine on mains power. Type FIRMWARE to see the plan:",
                            update.device
                        );
                        self.firmware_pending = Some(planned);
                        self.popup = Some(Popup::FirmwareWord(InputPopup::new(
                            "Firmware update",
                            prompt,
                        )));
                    }
                }
                KeyCode::Char('s') => {
                    if let Some(HardwareRow::Item {
                        act: Some(HardwareAct::Smart(disk)),
                        ..
                    }) = self.selected_hardware()
                        && let Some(planned) = helix_toolbox_engine::hardware::plan_smart(disk)
                    {
                        self.confirm_plan(Ok(planned));
                    }
                }
                KeyCode::Char('r') => self.read_hardware(),
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Reliability => match key.code {
                KeyCode::Enter | KeyCode::Char('d') => self.describe_reliability(),
                KeyCode::Char('v') => {
                    if let Some((id, _, _)) = self
                        .main_state
                        .selected()
                        .and_then(|index| self.reliability_rows.get(index))
                    {
                        let id = id.clone();
                        self.inspect(&id);
                    }
                }
                KeyCode::Char('i') => {
                    self.coverage = self.coverage.next();
                    self.main_state.select(Some(0));
                    self.reload_reliability();
                }
                KeyCode::Char('w') => {
                    if let Some((id, name, _)) = self
                        .main_state
                        .selected()
                        .and_then(|index| self.reliability_rows.get(index))
                        .cloned()
                    {
                        match hardware_report::last_success(&self.run_history(), &id).cloned() {
                            Some(record) => self.report_worked(&record, Some(&id)),
                            None => {
                                self.popup = Some(Popup::Text(TextPopup::new(
                                    "Not run here",
                                    format!(
                                        "A hardware report comes from a run that worked on this machine, and {name} has no successful run in this machine's history."
                                    ),
                                    true,
                                )))
                            }
                        }
                    }
                }
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Drift => match key.code {
                KeyCode::Enter | KeyCode::Char('d') => self.describe_difference(),
                KeyCode::Char(' ') => {
                    if let Some(difference) = self.selected_difference()
                        && restorable(difference.kind)
                        && let Some(entry) = difference
                            .entry
                            .as_deref()
                            .and_then(|id| self.catalog.find(id))
                            .cloned()
                    {
                        self.toggle_queued(entry);
                    }
                }
                KeyCode::Char('a') => self.queue_missing(),
                KeyCode::Char('R') => self.run_missing(),
                KeyCode::Char('b') => self.next_drift_target(1),
                KeyCode::Char('B') => self.next_drift_target(-1),
                KeyCode::Char('s') => {
                    self.popup = Some(Popup::BaselineName(InputPopup::new(
                        "Save this machine as a baseline",
                        "A name for the baseline (it is saved in ~/.config/helix-toolbox/baselines):",
                    )));
                }
                KeyCode::Char('r') => {
                    self.reload_drift();
                    self.refresh_statuses();
                }
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Backups => match key.code {
                KeyCode::Enter => {
                    if let Some(backup) = self.selected_backup().cloned() {
                        self.confirm_plan(Ok(backups::plan_restore(&backup)));
                    }
                }
                KeyCode::Char('d') => self.describe_backup(),
                KeyCode::Char('e') => {
                    if let Some(backup) = self.selected_backup().cloned() {
                        self.show_effects(&backups::plan_restore(&backup), false);
                    }
                }
                KeyCode::Char('r') => self.reload_backups(),
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Services => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('l') | KeyCode::Right => {
                    match self.selected_service_row() {
                        Some(ServiceRow::Unit(index)) => self.describe_unit(index),
                        Some(ServiceRow::Autostart(index)) => self.switch_autostart(index),
                        _ => {}
                    }
                }
                KeyCode::Char('d') | KeyCode::Char('D') => match self.selected_service_row() {
                    Some(ServiceRow::Unit(index)) => self.describe_unit(index),
                    Some(ServiceRow::Autostart(index)) => self.describe_autostart(index),
                    _ => {}
                },
                KeyCode::Char('s') => {
                    if let Some(ServiceRow::Unit(index)) = self.selected_service_row()
                        && let Some(unit) = self.unit(index)
                    {
                        let verb = if unit.running() {
                            Verb::Stop
                        } else {
                            Verb::Start
                        };
                        self.change_unit(index, verb);
                    }
                }
                KeyCode::Char('e') => {
                    if let Some(ServiceRow::Unit(index)) = self.selected_service_row()
                        && let Some(unit) = self.unit(index)
                        && unit.switchable()
                    {
                        let verb = if unit.enabled() {
                            Verb::Disable
                        } else {
                            Verb::Enable
                        };
                        self.change_unit(index, verb);
                    }
                }
                KeyCode::Char('R') => {
                    if let Some(ServiceRow::Unit(index)) = self.selected_service_row() {
                        self.change_unit(index, Verb::Restart);
                    }
                }
                KeyCode::Char('i') => {
                    self.service_view = self.service_view.next();
                    self.main_state.select(Some(0));
                    self.build_service_rows();
                }
                KeyCode::Char('f') => {
                    self.popup = Some(Popup::Ask(
                        InputPopup::new(
                            "Filter",
                            "Only units and applications whose name or description has (blank for all):",
                        )
                        .with_value(self.service_filter.clone())
                        .answering("Filter"),
                        Asked::ServiceFilter,
                    ));
                }
                KeyCode::Char('r') => self.reload_services(),
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Storage => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('l') | KeyCode::Right => {
                    self.clean_selected()
                }
                KeyCode::Char('d') => self.describe_storage(),
                KeyCode::Char('r') => self.read_storage(),
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Performance => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('l') | KeyCode::Right => {
                    self.act_on_performance()
                }
                KeyCode::Char('d') => self.describe_performance(),
                KeyCode::Char('b') => {
                    let earlier = self.earlier_boots().len();
                    if earlier > 1 {
                        self.performance_against = (self.performance_against + 1) % earlier;
                        self.build_performance_rows();
                    }
                }
                KeyCode::Char('r') => self.read_performance(),
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Recovery => match key.code {
                KeyCode::Enter | KeyCode::Char('d') | KeyCode::Char('l') | KeyCode::Right => {
                    if let Some(check) = self.selected_readiness().cloned() {
                        self.show_diagnosis(&check);
                    }
                }
                KeyCode::Char('f') => {
                    if let Some(entry) = self
                        .selected_readiness()
                        .and_then(|check| check.entry.as_deref())
                        .and_then(|id| self.catalog.find(id))
                        .cloned()
                    {
                        self.confirm(vec![entry]);
                    }
                }
                KeyCode::Char('c') => match self.snapshots_here() {
                    Some(tool) => self.confirm_plan(Ok(recovery::plan_check(tool))),
                    None => {
                        self.popup = Some(Popup::Text(TextPopup::new(
                            "No snapshots",
                            "Neither Snapper (with a root configuration) nor Timeshift is set up, so there are no snapshots to check.",
                            true,
                        )))
                    }
                },
                KeyCode::Char('r') => self.reload_recovery(),
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Migration => match key.code {
                KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                    match self.selected_migration_item() {
                        Some(item) if item.standing == Standing::Install => self.apply_migration(),
                        Some(_) => self.describe_migration(),
                        None => {}
                    }
                }
                KeyCode::Char('d') => self.describe_migration(),
                KeyCode::Char('e') => {
                    let name = self
                        .migration_here
                        .as_ref()
                        .map(migration::file_name)
                        .unwrap_or_else(|| "helix-toolbox-migration.toml".to_string());
                    self.popup = Some(Popup::Ask(
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
                    if let Some((path, _)) = &self.migration_opened {
                        ask = ask.with_value(path.display().to_string());
                    }
                    self.popup = Some(Popup::Ask(ask, Asked::OpenMigration));
                }
                KeyCode::Char('a') => self.apply_migration(),
                KeyCode::Char('r') => self.reload_migration(),
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Restic => match key.code {
                KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => self.open_restic_row(),
                KeyCode::Char('R') => self.ask_restore(),
                KeyCode::Char('F') => self.ask_restore_from(),
                KeyCode::Char('i') => {
                    self.restic_ignore_case = !self.restic_ignore_case;
                    match self
                        .restic_search
                        .as_ref()
                        .map(|(pattern, _)| pattern.clone())
                    {
                        Some(pattern) => self.search_restic(pattern),
                        None => self.build_restic_rows(),
                    }
                }
                KeyCode::Char('d') => self.describe_restic(),
                KeyCode::Char('b') => self.plan_restic(restic::plan_backup),
                KeyCode::Char('c') => self.plan_restic(restic::plan_check),
                KeyCode::Char('s') if self.restic_config().is_some() => {
                    let mut ask = InputPopup::new(
                        "A file's history",
                        "A file name or path to look for in every snapshot, with * wildcards, such as notes.txt:",
                    )
                    .answering("Look");
                    if let Some((pattern, _)) = &self.restic_search {
                        ask = ask.with_value(pattern.clone());
                    }
                    self.popup = Some(Popup::Ask(ask, Asked::ResticHistory));
                }
                KeyCode::Char('n') => {
                    self.popup = Some(Popup::Ask(
                        InputPopup::new(
                            "Record a baseline",
                            "A file of your home folder whose SHA-256 to record, to test restores of it against:",
                        )
                        .path()
                        .answering("Record"),
                        Asked::ResticBaseline,
                    ));
                }
                KeyCode::Char('v') => self.ask_test(),
                KeyCode::Char('w') => self.toggle_restic_timer(restic::Every::Day),
                KeyCode::Char('W') => self.toggle_restic_timer(restic::Every::Week),
                KeyCode::Char('r') => self.read_restic(),
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Repositories => self.repo_key(key.code),
            Screen::Configs => self.config_key(key.code),
            Screen::HomeManager => self.home_key(key.code),
            Screen::Bundles => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('l') | KeyCode::Right => {
                    self.bundle_enter()
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
        }
    }

    pub(super) fn handle_search_key(&mut self, key: &KeyEvent) {
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

    pub(super) fn move_selection(&mut self, step: isize) {
        let count = self.main_len();
        if count == 0 {
            return;
        }
        let current = self.main_state.selected().unwrap_or(0) as isize;
        self.main_state
            .select(Some((current + step).clamp(0, count as isize - 1) as usize));
    }

    pub(super) fn enter(&mut self) {
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

    pub(super) fn back(&mut self) {
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
