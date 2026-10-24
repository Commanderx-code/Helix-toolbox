//! The Hardware screen.

use super::super::*;
use super::{Ask, Cx, Read, ToolScreen, View};
use helix_toolbox_engine::hardware;

/// Firmware, disks, temperatures, batteries and devices.
#[derive(Default)]
pub(in crate::tui::app) struct Hardware {
    /// What it last read, and its rows.
    found: Option<hardware::Hardware>,
    pub(in crate::tui::app) rows: Vec<HardwareRow>,
    /// A look at the hardware running in the background.
    reading: Read<hardware::Hardware>,
    /// The firmware update waiting for its confirming word.
    firmware: Option<Plan>,
    /// A run while another screen was shown left what it read out of
    /// date.
    stale: bool,
}

/// A line of the Hardware screen.
#[derive(Clone, Debug)]
pub(in crate::tui::app) enum HardwareRow {
    Heading(String),
    Item {
        text: String,
        /// Shown in a warning color.
        attention: bool,
        details: String,
        act: Option<HardwareAct>,
    },
}

#[derive(Clone, Debug)]
pub(in crate::tui::app) enum HardwareAct {
    /// Update this firmware, by its index in the updates.
    Firmware(usize),
    /// Check this disk's SMART health.
    Smart(String),
}

/// The Hardware screen's lines, section by section, from the rows both
/// interfaces show, with what this screen's keys do.
pub(in crate::tui::app) fn hardware_rows(found: &hardware::Hardware) -> Vec<HardwareRow> {
    let shown = rows::hardware(found);
    let item = |row: &rows::Row, act: Option<HardwareAct>| {
        let keys = match &act {
            Some(HardwareAct::Firmware(_)) => {
                "\n\nf updates it through fwupd after you type FIRMWARE and confirm the plan. Keep the machine on mains power; some updates finish at the next reboot."
            }
            Some(HardwareAct::Smart(_)) => {
                "\n\nSMART health needs root to read; s checks it after confirmation. It only reads the disk's own health record."
            }
            None => "",
        };
        HardwareRow::Item {
            text: row.line.clone(),
            attention: row.attention,
            details: format!("{}{keys}", row.more),
            act,
        }
    };
    let mut list = vec![HardwareRow::Heading("Firmware".to_string())];
    list.extend(shown.note.iter().map(|row| item(row, None)));
    list.extend(
        shown
            .updates
            .iter()
            .enumerate()
            .map(|(index, row)| item(row, Some(HardwareAct::Firmware(index)))),
    );
    list.extend(shown.firmware.iter().map(|row| item(row, None)));
    list.push(HardwareRow::Heading("Disks".to_string()));
    list.extend(
        shown
            .disks
            .iter()
            .zip(&found.disks)
            .map(|(row, disk)| item(row, Some(HardwareAct::Smart(disk.name.clone())))),
    );
    list.push(HardwareRow::Heading("Temperatures".to_string()));
    list.push(item(&shown.temperatures, None));
    if !shown.batteries.is_empty() {
        list.push(HardwareRow::Heading("Batteries".to_string()));
    }
    list.extend(shown.batteries.iter().map(|row| item(row, None)));
    for (title, devices) in [("PCI devices", &shown.pci), ("USB devices", &shown.usb)] {
        list.push(HardwareRow::Heading(title.to_string()));
        list.extend(devices.iter().map(|row| item(row, None)));
    }
    list
}

impl Hardware {
    /// Read the hardware in the background; fwupd can take a moment.
    fn read(&mut self, cx: &Cx) {
        self.reading.start(cx.host, |host| hardware::gather(&host));
    }

    fn selected(&self, at: Option<usize>) -> Option<&HardwareRow> {
        self.rows.get(at?)
    }

    /// The word typed to confirm the firmware update: FIRMWARE shows its
    /// plan; anything else, or no answer, drops it.
    pub(in crate::tui::app) fn confirm_firmware(&mut self, cx: &mut Cx, word: Option<&str>) {
        let Some(word) = word else {
            self.firmware = None;
            return;
        };
        match self.firmware.take() {
            Some(planned) if word.trim() == "FIRMWARE" => cx.confirm(planned),
            _ => {
                *cx.popup = Some(Popup::Text(TextPopup::new(
                    "Not confirmed",
                    "The firmware update needs the word FIRMWARE, in capitals. Nothing was changed.",
                    true,
                )))
            }
        }
    }
}

impl ToolScreen for Hardware {
    fn open(&mut self, cx: &mut Cx) {
        if std::mem::take(&mut self.stale) || self.found.is_none() {
            self.read(cx);
        }
    }

    fn len(&self, _view: &View) -> usize {
        self.rows.len()
    }

    fn key(&mut self, cx: &mut Cx, code: KeyCode) {
        match code {
            KeyCode::Enter | KeyCode::Char('d') => {
                if let Some(HardwareRow::Item { text, details, .. }) = self.selected(cx.at()) {
                    *cx.popup = Some(Popup::Text(TextPopup::new(
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
                }) = self.selected(cx.at())
                    && let Some(update) = self
                        .found
                        .as_ref()
                        .and_then(|found| found.firmware.updates.get(*index))
                    && let Some(planned) = hardware::plan_firmware(update)
                {
                    let prompt = format!(
                        "Updating firmware can leave {} unusable if it is interrupted. Keep the machine on mains power. Type FIRMWARE to see the plan:",
                        update.device
                    );
                    self.firmware = Some(planned);
                    *cx.popup = Some(Popup::FirmwareWord(InputPopup::new(
                        "Firmware update",
                        prompt,
                    )));
                }
            }
            KeyCode::Char('s') => {
                if let Some(HardwareRow::Item {
                    act: Some(HardwareAct::Smart(disk)),
                    ..
                }) = self.selected(cx.at())
                    && let Some(planned) = hardware::plan_smart(disk)
                {
                    cx.confirm(planned);
                }
            }
            KeyCode::Char('r') => self.read(cx),
            KeyCode::Char('h') | KeyCode::Left => cx.ask(Ask::Sidebar),
            _ => {}
        }
    }

    fn hints(&self, _view: &View, at: Option<usize>, list: &mut Vec<Hint>) {
        list.push(Hint::new(&["↑↓"], "Navigate"));
        if let Some(HardwareRow::Item { act, .. }) = self.selected(at) {
            list.push(Hint::new(&["Enter"], "Details"));
            match act {
                Some(HardwareAct::Firmware(_)) => list.push(Hint::new(&["f"], "Update firmware")),
                Some(HardwareAct::Smart(_)) => list.push(Hint::new(&["s"], "SMART check")),
                None => {}
            }
        }
        list.push(Hint::new(&["r"], "Read again"));
    }

    fn poll(&mut self, cx: &mut Cx) -> bool {
        let Some(answer) = self.reading.take() else {
            return false;
        };
        if let Ok(found) = answer {
            self.rows = hardware_rows(&found);
            self.found = Some(found);
        }
        if cx.screen == Screen::Hardware {
            cx.clamp();
        }
        true
    }

    fn after_run(&mut self, cx: &mut Cx, reading: Reading) {
        if reading == Reading::Tools {
            self.read(cx);
        }
    }

    fn stale(&mut self, reading: Reading) {
        if reading == Reading::Tools {
            self.stale = true;
        }
    }

    fn draw(&self, view: &View, frame: &mut Frame, area: Rect, state: &mut ListState) {
        let inner = view.page(
            frame,
            area,
            "Hardware",
            "Firmware, disks, sensors, batteries and devices, read-only; updates and SMART checks ask first.",
        );
        view.fill(frame, inner);
        let focused = view.focused;
        let mut items: Vec<Line> = Vec::new();
        if self.rows.is_empty() {
            items.push(Line::styled(
                if self.reading.busy() {
                    "Reading the hardware…"
                } else if self.reading.failed() {
                    "Could not read this machine's hardware. r reads again."
                } else {
                    "Press r to read this machine's hardware."
                },
                Style::default().fg(view.theme.muted()),
            ));
        }
        for row in &self.rows {
            items.push(match row {
                HardwareRow::Heading(title) => Line::styled(
                    title.clone(),
                    Style::default().fg(view.theme.group()).bold(),
                ),
                HardwareRow::Item {
                    text,
                    attention,
                    act,
                    ..
                } => Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        text.clone(),
                        Style::default().fg(if *attention {
                            view.theme.warning()
                        } else {
                            view.theme.entry()
                        }),
                    ),
                    Span::styled(
                        match act {
                            Some(HardwareAct::Firmware(_)) => "  · f updates it",
                            Some(HardwareAct::Smart(_)) => "  · s checks its health",
                            None => "",
                        },
                        Style::default().fg(view.theme.focused()),
                    ),
                ]),
            });
        }
        let title = if self.reading.busy() && !self.rows.is_empty() {
            "This machine (reading again…)"
        } else {
            "This machine"
        };
        let list = List::new(view.measure(items))
            .block(view.card(title))
            .highlight_style(view.highlight(focused))
            .highlight_symbol(view.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, inner, state, view.hits);
    }
}
