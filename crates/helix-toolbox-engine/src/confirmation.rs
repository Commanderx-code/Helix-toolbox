//! Between a plan and its run: what an interface shows to ask the user, or
//! why there is nothing to ask. The command line, the terminal interface
//! and the window show the same lines, from the same checks.

use crate::{
    Plan, backups, preflight,
    reliability::{self, Evidence},
    session::RunSession,
    settings::RunChoices,
};
use helix_toolbox_catalog::Catalog;
use helix_toolbox_core::Entry;
use helix_toolbox_platform::LinuxInfo;
use serde::Serialize;

/// A plan checked and ready to ask about, or why not.
#[derive(Debug)]
pub struct Prepared {
    /// The plan as it would run on a yes: the snapshot first where it is
    /// wanted and worth one, the choice after a failure applied.
    pub plan: Plan,
    /// What the checks before a run found.
    pub findings: Vec<preflight::Finding>,
    pub verdict: Verdict,
}

/// Whether a prepared plan can run, with what to show.
#[derive(Debug)]
pub enum Verdict {
    /// The plan runs nothing; its notes and what it left out say why.
    Nothing(Vec<String>),
    /// The checks before a run found something that stops it. Nothing was
    /// changed.
    Blocked(Vec<String>),
    /// Ready for the user's yes.
    Ready {
        /// What to know first: the risk and password, the checks' warnings,
        /// warnings about how entries were tested, what happens after a
        /// failure.
        header: Vec<String>,
        /// Each step's name and command, then the notes and what is not
        /// run.
        lines: Vec<Line>,
    },
}

impl Prepared {
    /// Whether the plan can run: it has steps and nothing blocks it.
    pub fn is_ready(&self) -> bool {
        matches!(self.verdict, Verdict::Ready { .. })
    }

    /// The run session for the user's yes; None when there is nothing to
    /// run or something blocks it.
    pub fn session(self) -> Option<RunSession> {
        self.is_ready().then(|| RunSession::new(self.plan))
    }
}

/// One line of the confirmation's list.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Line {
    pub name: String,
    /// The command, or empty for a note.
    pub detail: String,
}

/// How the run should go, from the settings, for [`prepare_with`].
#[derive(Clone, Copy, Debug, Default)]
pub struct Choices {
    /// Whether to take a snapshot first, and with which tool.
    pub snapshot: Snapshot,
    /// After a failed step, continue with the steps that do not need it.
    /// A plan that does so itself (a migration's) still does.
    pub keep_going: bool,
}

/// Whether a run takes a snapshot before it changes the system.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Snapshot {
    /// Settings turns snapshots off.
    #[default]
    Off,
    /// Settings wants one before system changes: with this tool, or, where
    /// the machine has neither Snapper nor Timeshift, a note saying so.
    Wanted(Option<backups::Snapshots>),
    /// Asked for on this run (`run --snapshot`): as `Wanted`, and a run
    /// that changes no system files says why it takes none.
    Asked(Option<backups::Snapshots>),
}

/// What the user chose for a run: the run choices in Settings (with what
/// the command line's flags turned on), and whether this run itself asked
/// for a snapshot.
#[derive(Clone, Copy, Debug, Default)]
pub struct Wants {
    pub run_choices: RunChoices,
    /// `run --snapshot`: a run not worth a snapshot says so.
    pub snapshot_asked: bool,
}

impl From<RunChoices> for Wants {
    fn from(run_choices: RunChoices) -> Self {
        Self {
            run_choices,
            snapshot_asked: false,
        }
    }
}

/// Get a plan ready to confirm on this machine, as the settings' run
/// choices say: add the snapshot (Snapper or Timeshift, where set up), run
/// the checks before a run, and gather what the user should see, with how
/// this build's entries were tested.
pub fn prepare(
    planned: Plan,
    catalog: &Catalog,
    platform: &LinuxInfo,
    wants: impl Into<Wants>,
) -> Prepared {
    let Wants {
        run_choices,
        snapshot_asked,
    } = wants.into();
    let choices = Choices {
        snapshot: if snapshot_asked {
            Snapshot::Asked(backups::Snapshots::here())
        } else if run_choices.snapshots {
            Snapshot::Wanted(backups::Snapshots::here())
        } else {
            Snapshot::Off
        },
        keep_going: run_choices.keep_going,
    };
    prepare_with(
        planned,
        platform,
        &|id| catalog.find(id),
        choices,
        &preflight::Facts::real(),
        &Evidence::load(catalog.root()),
    )
}

/// `prepare` with the entries, the snapshot tool, the machine's answers and
/// the testing record given, for tests.
pub fn prepare_with<'a>(
    planned: Plan,
    platform: &LinuxInfo,
    lookup: &dyn Fn(&str) -> Option<&'a Entry>,
    choices: Choices,
    facts: &preflight::Facts,
    evidence: &Evidence,
) -> Prepared {
    let mut planned = planned;
    // A plan that keeps going after a failure itself (a migration's) still
    // does; Settings can only turn it on for the rest.
    let own_choice = planned.keep_going;
    planned.keep_going |= choices.keep_going;
    if planned.is_empty() {
        let mut lines = planned.notes.clone();
        lines.extend(
            planned
                .blocked
                .iter()
                .map(|blocked| format!("Not run: {blocked}")),
        );
        if lines.is_empty() {
            lines.push("Nothing to run.".to_string());
        }
        let findings = preflight::check(&planned, platform, lookup, facts);
        return Prepared {
            plan: planned,
            findings,
            verdict: Verdict::Nothing(lines),
        };
    }
    // A snapshot first when the run changes the system, where possible;
    // where a wanted one is not taken, why, when that tells the user
    // something.
    let flags = |id: &str| lookup(id).map(|entry| entry.flags.clone());
    let worth = backups::worth_a_snapshot(&planned, &flags);
    match choices.snapshot {
        Snapshot::Wanted(tool) | Snapshot::Asked(tool) if worth => match tool {
            Some(tool) => backups::with_snapshot(&mut planned, tool, &flags),
            None => planned
                .notes
                .push("No snapshot: neither Snapper nor Timeshift is set up here.".into()),
        },
        Snapshot::Asked(_) => planned.notes.push(
            "No snapshot: this run does not change system files, the kernel or disks.".into(),
        ),
        Snapshot::Wanted(_) | Snapshot::Off => {}
    }
    let findings = preflight::check(&planned, platform, lookup, facts);
    if preflight::blocked(&findings) {
        let lines = findings.iter().map(preflight::Finding::line).collect();
        return Prepared {
            plan: planned,
            findings,
            verdict: Verdict::Blocked(lines),
        };
    }
    let mut header = vec![format!(
        "Risk: {}{}",
        planned.risk.label(),
        if planned.privileged {
            " · asks for your password"
        } else {
            ""
        }
    )];
    header.extend(findings.iter().map(preflight::Finding::line));
    header.extend(
        planned
            .warnings
            .iter()
            .map(|warning| format!("Warning: {warning}")),
    );
    // The warnings borrow the step ids only as long as the entries they find.
    let near = |id: &str| lookup(id);
    header.extend(
        reliability::warnings(planned.steps.iter().map(|step| step.id()), &near, evidence)
            .into_iter()
            .map(|warning| format!("Warning: {warning}")),
    );
    if planned.steps.len() > 1 {
        header.push(match (planned.keep_going, own_choice) {
            (_, true) => "After a failed step: continue with what does not need it".to_string(),
            (true, false) => {
                "After a failed step: continue with what does not need it (Settings changes it)"
                    .to_string()
            }
            (false, _) => "After a failed step: stop (Settings changes it)".to_string(),
        });
    }
    let mut lines: Vec<Line> = planned
        .steps
        .iter()
        .map(|step| Line {
            name: step.name().to_string(),
            detail: step.display(),
        })
        .collect();
    let note = |name: String| Line {
        name,
        detail: String::new(),
    };
    lines.extend(planned.notes.iter().cloned().map(note));
    lines.extend(
        planned
            .blocked
            .iter()
            .map(|blocked| note(format!("Not run: {blocked}"))),
    );
    Prepared {
        plan: planned,
        findings,
        verdict: Verdict::Ready { header, lines },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Machine, plan_with};
    use helix_toolbox_core::{Action, PackageManager, PackageNames};

    fn arch() -> LinuxInfo {
        LinuxInfo {
            id: "garuda".into(),
            pretty_name: "Garuda Linux".into(),
            id_like: vec!["arch".into()],
            manager: Some(PackageManager::Arch),
        }
    }

    fn package(id: &str, flags: &str) -> Entry {
        Entry {
            id: id.into(),
            name: id.to_uppercase(),
            description: String::new(),
            flags: flags.into(),
            multi_select: true,
            writes: Vec::new(),
            action: Action::Package {
                binary: None,
                packages: PackageNames {
                    arch: Some(id.into()),
                    ..Default::default()
                },
                aur: None,
                flatpak: None,
                conflicts: Vec::new(),
                removable: true,
                release: None,
            },
            detect: None,
            needs: Default::default(),
            effects: Default::default(),
        }
    }

    const NOTHING_HELD: Machine<'static> = Machine {
        holds: &|_| false,
        on_path: &|_| false,
        ..crate::Machine::blind()
    };

    fn prepared(entries: &[Entry], choices: Choices, space: Option<u64>) -> Prepared {
        let lookup = |id: &str| entries.iter().find(|entry| entry.id == id);
        let planned = plan_with(entries, &arch(), &lookup, NOTHING_HELD).unwrap();
        let free = move |_: &std::path::Path| space;
        let facts = preflight::Facts {
            machine: NOTHING_HELD,
            present: &|_| true,
            free_bytes: &free,
            gpus: &Vec::new,
            repositories: &|| None,
            sizes: &|_, _| Some(1 << 20),
            active_runs: &Vec::new,
        };
        prepare_with(
            planned,
            &arch(),
            &lookup,
            choices,
            &facts,
            &Evidence::default(),
        )
    }

    #[test]
    fn a_plan_is_ready_with_its_risk_steps_and_the_choice_after_a_failure() {
        let entries = [package("ripgrep", "I"), package("fzf", "I")];
        let ready = prepared(&entries, Choices::default(), None);
        assert!(!ready.plan.keep_going);
        let Verdict::Ready { header, lines } = &ready.verdict else {
            panic!("ready");
        };
        assert_eq!(header[0], "Risk: medium · asks for your password");
        assert_eq!(
            header.last().unwrap(),
            "After a failed step: stop (Settings changes it)"
        );
        let names: Vec<&str> = lines.iter().map(|line| line.name.as_str()).collect();
        assert_eq!(names, ["RIPGREP", "FZF"]);
        assert!(lines[0].detail.contains("pacman -S --needed ripgrep"));
        let session = ready.session().expect("a session for the yes");
        assert_eq!(session.plan().steps.len(), 2);

        let keep_going = Choices {
            keep_going: true,
            ..Choices::default()
        };
        let going = prepared(&entries, keep_going, None);
        assert!(going.plan.keep_going);
        let Verdict::Ready { header, .. } = &going.verdict else {
            panic!("ready");
        };
        assert!(header.last().unwrap().contains("continue with what"));
    }

    #[test]
    fn a_snapshot_comes_first_only_for_system_changes() {
        let snapper = Choices {
            snapshot: Snapshot::Wanted(Some(backups::Snapshots::Snapper)),
            ..Choices::default()
        };
        let plain = prepared(&[package("ripgrep", "I")], snapper, None);
        assert!(plain.is_ready());
        assert_eq!(plain.plan.steps.len(), 1);
        let kernel = prepared(&[package("linux-zen", "I K")], snapper, None);
        assert_eq!(kernel.plan.steps.len(), 2);
        let Verdict::Ready { lines, .. } = &kernel.verdict else {
            panic!("ready");
        };
        assert!(lines[0].detail.contains("snapper"));
    }

    /// Run a prepared plan through its run session, its snapshot failing and
    /// every other step doing nothing, and say how each step ended and which
    /// entries remain queued.
    fn run_with_a_failed_snapshot(mut ready: Prepared) -> (Vec<String>, Vec<String>) {
        use crate::{
            PtyRunner, Step,
            execution::{HistoryPlace, Options},
        };
        let steps = std::mem::take(&mut ready.plan.steps);
        ready.plan.steps = steps
            .into_iter()
            .map(|step| match step {
                Step::Shell {
                    id,
                    name,
                    display,
                    requires,
                    changes,
                    ..
                } => Step::Shell {
                    id,
                    name,
                    program: "exit 1".into(),
                    display,
                    requires,
                    changes,
                },
                other => Step::Command {
                    id: other.id().into(),
                    name: other.name().into(),
                    line: "true".into(),
                },
            })
            .collect();
        ready.plan.requirements.clear();
        let mut session = ready.session().expect("a session for the yes");
        let state = tempfile::tempdir().unwrap();
        let options = Options {
            history: HistoryPlace::In(state.path().to_path_buf()),
            ..Options::default()
        };
        let lib = std::path::Path::new("/lib");
        let this = std::path::Path::new("/bin/hf");
        let _events = session
            .start(PtyRunner::default(), lib, this, options)
            .unwrap();
        let finished = session.wait().expect("the run ends");
        let states = finished
            .steps
            .iter()
            .map(|step| format!("{}: {}", step.id, step.state.describe()))
            .collect();
        (states, session.remaining(&|id| id != "snapshot"))
    }

    #[test]
    fn a_failed_snapshot_stops_the_system_changes_after_it_even_when_keeping_going() {
        let entries = || [package("grub-theme", "I PFM"), package("dotfiles", "")];
        let snapper = Snapshot::Wanted(Some(backups::Snapshots::Snapper));
        let going = Choices {
            snapshot: snapper,
            keep_going: true,
        };
        let ready = prepared(&entries(), going, None);
        assert!(ready.is_ready());
        let (states, remaining) = run_with_a_failed_snapshot(ready);
        assert_eq!(
            states,
            [
                "snapshot: failed (exit 1)",
                "grub-theme: not run: the snapshot before it failed",
                "dotfiles: done",
            ]
        );
        // What needed the snapshot stays queued to try again.
        assert_eq!(remaining, ["grub-theme"]);

        // Without keep-going, the failed snapshot stops the run as before.
        let stop = Choices {
            snapshot: snapper,
            keep_going: false,
        };
        let (states, remaining) = run_with_a_failed_snapshot(prepared(&entries(), stop, None));
        assert_eq!(
            states,
            [
                "snapshot: failed (exit 1)",
                "grub-theme: not run: the run stopped first",
                "dotfiles: not run: the run stopped first",
            ]
        );
        assert_eq!(remaining, ["grub-theme", "dotfiles"]);
    }

    /// The notes about a snapshot not taken, or None when there is none.
    fn no_snapshot(entry: Entry, snapshot: Snapshot) -> Option<String> {
        let choices = Choices {
            snapshot,
            ..Choices::default()
        };
        let ready = prepared(&[entry], choices, None);
        let Verdict::Ready { lines, .. } = &ready.verdict else {
            panic!("ready");
        };
        let notes: Vec<&str> = lines
            .iter()
            .filter(|line| line.name.starts_with("No snapshot"))
            .map(|line| line.name.as_str())
            .collect();
        (!notes.is_empty()).then(|| notes.join("\n"))
    }

    #[test]
    fn a_snapshot_not_taken_is_explained_only_where_one_was_expected() {
        let kernel = || package("linux-zen", "I K");
        let plain = || package("ripgrep", "I");
        let neither = "No snapshot: neither Snapper nor Timeshift is set up here.";
        let unchanged = "No snapshot: this run does not change system files, the kernel or disks.";
        // A system change on a machine without a snapshot tool says so,
        // whether Settings or `--snapshot` wants one.
        assert_eq!(
            no_snapshot(kernel(), Snapshot::Wanted(None)).as_deref(),
            Some(neither)
        );
        assert_eq!(
            no_snapshot(kernel(), Snapshot::Asked(None)).as_deref(),
            Some(neither)
        );
        // Only `--snapshot` hears that a run is not worth one.
        assert_eq!(
            no_snapshot(plain(), Snapshot::Asked(Some(backups::Snapshots::Snapper))).as_deref(),
            Some(unchanged)
        );
        assert_eq!(
            no_snapshot(plain(), Snapshot::Asked(None)).as_deref(),
            Some(unchanged)
        );
        // Otherwise nothing: the snapshot is taken, nothing wanted one, or
        // Settings wants one and the run is not worth it.
        let snapper = Snapshot::Wanted(Some(backups::Snapshots::Snapper));
        assert_eq!(no_snapshot(kernel(), snapper), None);
        assert_eq!(no_snapshot(kernel(), Snapshot::Off), None);
        assert_eq!(no_snapshot(plain(), Snapshot::Wanted(None)), None);
        assert_eq!(no_snapshot(plain(), snapper), None);
    }

    /// A migration plan keeps going after a failure (`migration::plan` sets
    /// it); with Settings' choice off it still does, in every interface.
    #[test]
    fn a_plan_that_keeps_going_still_does_with_settings_off() {
        let entries = [package("ripgrep", "I"), package("fzf", "I")];
        let lookup = |id: &str| entries.iter().find(|entry| entry.id == id);
        let mut planned = plan_with(&entries, &arch(), &lookup, NOTHING_HELD).unwrap();
        planned.keep_going = true; // as migration::plan does
        let facts = preflight::Facts {
            machine: NOTHING_HELD,
            present: &|_| true,
            free_bytes: &|_| None,
            gpus: &Vec::new,
            repositories: &|| None,
            sizes: &|_, _| Some(1 << 20),
            active_runs: &Vec::new,
        };
        let ready = prepare_with(
            planned,
            &arch(),
            &lookup,
            Choices::default(),
            &facts,
            &Evidence::default(),
        );
        assert!(ready.plan.keep_going, "Settings' choice dropped it");
        let Verdict::Ready { header, .. } = &ready.verdict else {
            panic!("ready");
        };
        // The plan's own choice, which Settings does not change.
        assert_eq!(
            header.last().unwrap(),
            "After a failed step: continue with what does not need it"
        );
    }

    #[test]
    fn a_full_disk_blocks_and_an_empty_plan_says_why() {
        let blocked = prepared(&[package("ripgrep", "I")], Choices::default(), Some(0));
        assert!(preflight::blocked(&blocked.findings));
        let Verdict::Blocked(lines) = &blocked.verdict else {
            panic!("blocked");
        };
        assert!(
            lines.iter().all(|line| line.starts_with("Blocked: ")),
            "{lines:?}"
        );
        assert_eq!(blocked.plan.steps.len(), 1, "the plan is kept to show");
        assert!(blocked.session().is_none(), "nothing to say yes to");
        let nothing = prepared(&[], Choices::default(), None);
        let Verdict::Nothing(lines) = &nothing.verdict else {
            panic!("nothing");
        };
        assert_eq!(lines, &["Nothing to run."]);
        assert!(nothing.session().is_none());
    }
}
