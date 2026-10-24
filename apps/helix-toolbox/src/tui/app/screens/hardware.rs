//! The Hardware screen.

use super::super::*;

impl App {
    /// Read the hardware in the background; fwupd can take a moment.
    pub(in crate::tui::app) fn read_hardware(&mut self) {
        if !self.examine_here {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(helix_toolbox_engine::hardware::gather(&doctor::Here));
        });
        self.reading_hardware = Some(receiver);
    }

    pub(in crate::tui::app) fn selected_hardware(&self) -> Option<&HardwareRow> {
        self.hardware_rows.get(self.main_state.selected()?)
    }

    pub(in crate::tui::app) fn draw_hardware(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Hardware",
            "Firmware, disks, sensors, batteries and devices, read-only; updates and SMART checks ask first.",
        );
        self.fill(frame, inner);
        let focused = self.focus == Focus::Main;
        let mut items: Vec<Line> = Vec::new();
        if self.hardware_rows.is_empty() {
            items.push(Line::styled(
                if self.reading_hardware.is_some() {
                    "Reading the hardware…"
                } else {
                    "Press r to read this machine's hardware."
                },
                Style::default().fg(self.theme.muted()),
            ));
        }
        for row in &self.hardware_rows {
            items.push(match row {
                HardwareRow::Heading(title) => Line::styled(
                    title.clone(),
                    Style::default().fg(self.theme.group()).bold(),
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
                            self.theme.warning()
                        } else {
                            self.theme.entry()
                        }),
                    ),
                    Span::styled(
                        match act {
                            Some(HardwareAct::Firmware(_)) => "  · f updates it",
                            Some(HardwareAct::Smart(_)) => "  · s checks its health",
                            None => "",
                        },
                        Style::default().fg(self.theme.focused()),
                    ),
                ]),
            });
        }
        let title = if self.reading_hardware.is_some() && !self.hardware_rows.is_empty() {
            "This machine (reading again…)"
        } else {
            "This machine"
        };
        let list = List::new(self.measure(items))
            .block(self.card(title))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, inner, &mut self.main_state, &self.hits);
    }
}
