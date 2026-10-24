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
    /// Take a snapshot first with this tool, when the plan is worth one.
    pub snapshot: Option<backups::Snapshots>,
    /// After a failed step, continue with the steps that do not need it.
    pub keep_going: bool,
}

/// Get a plan ready to confirm on this machine, as the settings' run
/// choices say: add the snapshot (Snapper or Timeshift, where set up), run
/// the checks before a run, and gather what the user should see, with how
/// this build's entries were tested.
pub fn prepare(
    planned: Plan,
    catalog: &Catalog,
    platform: &LinuxInfo,
    run_choices: RunChoices,
) -> Prepared {
    let choices = Choices {
        snapshot: run_choices
            .snapshots
            .then(backups::Snapshots::here)
            .flatten(),
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
    planned.keep_going = choices.keep_going;
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
    // A snapshot first when the run changes the system, where possible.
    if let Some(tool) = choices.snapshot
        && backups::worth_a_snapshot(&planned, &|id| lookup(id).map(|entry| entry.flags.clone()))
    {
        backups::with_snapshot(&mut planned, tool);
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
        header.push(format!(
            "After a failed step: {} (Settings changes it)",
            if choices.keep_going {
                "continue with what does not need it"
            } else {
                "stop"
            }
        ));
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
            snapshot: Some(backups::Snapshots::Snapper),
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
