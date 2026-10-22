//! Between a plan and its run: what an interface shows to ask the user, or
//! why there is nothing to ask. The terminal interface's confirmation and a
//! graphical one show the same lines.

use crate::{
    Plan, backups, preflight,
    reliability::{self, Evidence},
};
use helix_toolbox_core::Entry;
use helix_toolbox_platform::LinuxInfo;
use serde::Serialize;

/// What to ask, or why not.
#[derive(Debug)]
pub enum Prepared {
    /// The plan runs nothing; its notes and what it left out say why.
    Nothing(Vec<String>),
    /// The checks before a run found something that stops it. Nothing was
    /// changed.
    Blocked(Vec<String>),
    /// Ready for the user's yes.
    Ready(Confirmation),
}

/// A plan ready to run, with what to show before it does.
#[derive(Debug)]
pub struct Confirmation {
    /// The plan to run on a yes, with the snapshot and the choice after a
    /// failure applied.
    pub plan: Plan,
    /// What to know first: the risk and password, the checks' warnings,
    /// warnings about how entries were tested, what happens after a failure.
    pub header: Vec<String>,
    /// Each step's name and command, then the notes and what is not run.
    pub lines: Vec<Line>,
}

/// One line of the confirmation's list.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Line {
    pub name: String,
    /// The command, or empty for a note.
    pub detail: String,
}

/// How the run should go, from the settings.
#[derive(Clone, Copy, Debug, Default)]
pub struct Choices {
    /// Take a snapshot first with this tool, when the plan is worth one.
    pub snapshot: Option<backups::Snapshots>,
    /// After a failed step, continue with the steps that do not need it.
    pub keep_going: bool,
}

/// Get a plan ready to confirm: add the snapshot, run the checks before a
/// run, and gather what the user should see.
pub fn prepare<'a>(
    planned: Plan,
    platform: &LinuxInfo,
    lookup: &dyn Fn(&str) -> Option<&'a Entry>,
    choices: Choices,
    facts: &preflight::Facts,
    evidence: &Evidence,
) -> Prepared {
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
        return Prepared::Nothing(lines);
    }
    // A snapshot first when the run changes the system, where possible.
    let mut planned = planned;
    if let Some(tool) = choices.snapshot
        && backups::worth_a_snapshot(&planned, &|id| lookup(id).map(|entry| entry.flags.clone()))
    {
        backups::with_snapshot(&mut planned, tool);
    }
    let findings = preflight::check(&planned, platform, lookup, facts);
    if preflight::blocked(&findings) {
        return Prepared::Blocked(findings.iter().map(preflight::Finding::line).collect());
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
    planned.keep_going = choices.keep_going;
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
    Prepared::Ready(Confirmation {
        plan: planned,
        header,
        lines,
    })
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
            },
            detect: None,
            needs: Default::default(),
            effects: Default::default(),
        }
    }

    const NOTHING_HELD: Machine<'static> = Machine {
        holds: &|_| false,
        on_path: &|_| false,
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
        prepare(
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
        let Prepared::Ready(ready) = prepared(&entries, Choices::default(), None) else {
            panic!("ready");
        };
        assert_eq!(ready.header[0], "Risk: medium · asks for your password");
        assert_eq!(
            ready.header.last().unwrap(),
            "After a failed step: stop (Settings changes it)"
        );
        let names: Vec<&str> = ready.lines.iter().map(|line| line.name.as_str()).collect();
        assert_eq!(names, ["RIPGREP", "FZF"]);
        assert!(ready.lines[0].detail.contains("pacman -S --needed ripgrep"));
        assert!(!ready.plan.keep_going);

        let keep_going = Choices {
            keep_going: true,
            ..Choices::default()
        };
        let Prepared::Ready(ready) = prepared(&entries, keep_going, None) else {
            panic!("ready");
        };
        assert!(ready.plan.keep_going);
        assert!(ready.header.last().unwrap().contains("continue with what"));
    }

    #[test]
    fn a_snapshot_comes_first_only_for_system_changes() {
        let snapper = Choices {
            snapshot: Some(backups::Snapshots::Snapper),
            ..Choices::default()
        };
        let Prepared::Ready(plain) = prepared(&[package("ripgrep", "I")], snapper, None) else {
            panic!("ready");
        };
        assert_eq!(plain.plan.steps.len(), 1);
        let Prepared::Ready(kernel) = prepared(&[package("linux-zen", "I K")], snapper, None)
        else {
            panic!("ready");
        };
        assert_eq!(kernel.plan.steps.len(), 2);
        assert!(kernel.lines[0].detail.contains("snapper"));
    }

    #[test]
    fn a_full_disk_blocks_and_an_empty_plan_says_why() {
        let Prepared::Blocked(lines) =
            prepared(&[package("ripgrep", "I")], Choices::default(), Some(0))
        else {
            panic!("blocked");
        };
        assert!(
            lines.iter().all(|line| line.starts_with("Blocked: ")),
            "{lines:?}"
        );
        let Prepared::Nothing(lines) = prepared(&[], Choices::default(), None) else {
            panic!("nothing");
        };
        assert_eq!(lines, ["Nothing to run."]);
    }
}
