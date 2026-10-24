//! Bug reports and reports that a run worked.

use super::super::*;

impl App {
    /// Gather a bug report in the background: the health checks take a few
    /// seconds.
    pub(in crate::tui::app) fn make_report(&mut self) {
        let platform = self.platform.clone();
        let checks = self.examine_here;
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(crate::report::markdown(&crate::report::gather(
                &platform, checks,
            )));
        });
        self.reporting = Some(receiver);
        self.popup = Some(Popup::Text(TextPopup::new(
            "Bug report",
            "Gathering the report; the health checks take a few seconds…",
            true,
        )));
    }

    /// Show the gathered report, and keep a copy to paste from.
    pub(in crate::tui::app) fn show_report(&mut self, report: &str) {
        let saved = self.reports_dir.as_ref().map(|directory| {
            let path = directory.join("bug-report.md");
            std::fs::create_dir_all(directory)
                .and_then(|()| std::fs::write(&path, report))
                .map(|()| path)
        });
        let kept = match saved {
            Some(Ok(path)) => {
                let shown = path.display().to_string();
                let home = std::env::var("HOME").unwrap_or_default();
                let shown = match shown.strip_prefix(&home) {
                    Some(rest) if home.len() > 1 => format!("~{rest}"),
                    _ => shown,
                };
                format!("It is saved as {shown}, to copy from or attach.\n\n")
            }
            Some(Err(error)) => format!("It could not be saved: {error}\n\n"),
            None => String::new(),
        };
        let text = format!(
            "Paste it into a new issue at github.com/Commanderx-code/Helix-toolbox/issues, with what happened and how to make it happen again. It shows your home folder as ~ and holds no host name or logs, but read it before posting. {kept}{report}"
        );
        // Over anything but a run or a question.
        if matches!(self.popup, None | Some(Popup::Text(_))) {
            self.popup = Some(Popup::Text(
                TextPopup::new("Bug report", text, true).copying(report),
            ));
        }
    }

    /// A report that a run worked on this machine, to read, then send as an
    /// issue from the browser.
    pub(in crate::tui::app) fn report_worked(
        &mut self,
        record: &history::Record,
        only: Option<&str>,
    ) {
        let machine = match (&self.machine, self.examine_here) {
            (Some(machine), _) => machine.clone(),
            (None, true) => hardware_report::machine(
                &doctor::Here,
                &self.platform,
                &crate::setup::desktop_here(),
            ),
            (None, false) => hardware_report::Machine::default(),
        };
        let report = match hardware_report::prepare(record, only, machine, crate::VERSION, "") {
            Ok(report) => report,
            Err(why) => {
                self.popup = Some(Popup::Text(TextPopup::new("No hardware report", why, true)));
                return;
            }
        };
        let url = report.issue_url();
        let text = report.text();
        if let Some(directory) = &self.reports_dir {
            let kept = std::fs::create_dir_all(directory).and_then(|()| {
                std::fs::write(
                    directory.join("hardware-report.md"),
                    format!("{text}\nThe issue, filled in:\n\n{url}\n"),
                )
            });
            if let Err(error) = kept {
                tracing::warn!(%error, "the hardware report was not saved");
            }
        }
        self.popup = Some(Popup::HardwareReport(
            TextPopup::new(
                "Hardware report",
                format!(
                    "{text}\no opens the issue in your browser, filled in; GitHub asks you to sign in, and the form asks what you checked. Nothing is sent until you submit it there. c copies the report."
                ),
                true,
            )
            .copying(format!("{text}\nThe issue, filled in:\n\n{url}\n")),
            url,
        ));
    }
}
