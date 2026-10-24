//! Confirming and running a plan from the command line: the confirmation
//! every interface shows, printed as text, the confirmation word, and the
//! run in this terminal through the run session every interface uses.

use crate::{EFFECTS, JSON, effects, json};
use anyhow::{Context, Result, bail};
use helix_toolbox_catalog::Catalog;
use helix_toolbox_engine::{
    InheritedRunner, Plan,
    confirmation::{self, Prepared, Verdict},
    execution::{ExecutionEvent, HistoryPlace, Options},
    settings,
};
use helix_toolbox_platform::LinuxInfo;
use std::{
    io::{self, BufRead, IsTerminal, Write},
    sync::atomic::Ordering,
};

/// The run choices in Settings, with --keep-going and --snapshot turning
/// theirs on.
pub fn run_choices(keep_going: bool, snapshot: bool) -> settings::RunChoices {
    let saved = settings::path()
        .map(|path| settings::run_choices(&path))
        .unwrap_or_default();
    settings::RunChoices {
        keep_going: saved.keep_going || keep_going,
        snapshots: saved.snapshots || snapshot,
    }
}

/// Get a plan ready to confirm, as every interface does; a plan that keeps
/// going after a failure still does.
fn prepare(catalog: &Catalog, platform: &LinuxInfo, planned: &Plan) -> Prepared {
    confirmation::prepare(
        planned.clone(),
        catalog,
        platform,
        run_choices(planned.keep_going, false),
    )
}

/// Show a plan, ask for the confirmation word unless told not to, and run it,
/// with the run choices in Settings.
pub fn confirm_and_run(
    catalog: &Catalog,
    platform: &LinuxInfo,
    planned: &Plan,
    dry_run: bool,
    yes: bool,
    word: &str,
) -> Result<()> {
    run_prepared(
        catalog,
        platform,
        prepare(catalog, platform, planned),
        dry_run,
        yes,
        word,
    )
}

/// `confirm_and_run` with these run choices.
pub fn confirm_and_run_with(
    catalog: &Catalog,
    platform: &LinuxInfo,
    planned: &Plan,
    choices: settings::RunChoices,
    dry_run: bool,
    yes: bool,
    word: &str,
) -> Result<()> {
    let prepared = confirmation::prepare(planned.clone(), catalog, platform, choices);
    run_prepared(catalog, platform, prepared, dry_run, yes, word)
}

fn run_prepared(
    catalog: &Catalog,
    platform: &LinuxInfo,
    prepared: Prepared,
    dry_run: bool,
    yes: bool,
    word: &str,
) -> Result<()> {
    if JSON.load(Ordering::Relaxed) {
        if !dry_run {
            bail!("--json shows plans only: add --dry-run; nothing was changed");
        }
        print_json(catalog, platform, &prepared);
        return Ok(());
    }
    print_prepared(catalog, platform, &prepared);
    match &prepared.verdict {
        Verdict::Nothing(_) => return Ok(()),
        _ if dry_run => {
            println!("Dry run: nothing was changed.");
            return Ok(());
        }
        Verdict::Blocked(_) => {
            bail!("not run: fix what is blocked above first; nothing was changed")
        }
        Verdict::Ready { .. } => {}
    }
    if !yes && confirm(word)?.trim() != word {
        bail!("canceled; nothing was changed");
    }
    let Some(mut session) = prepared.session() else {
        bail!("nothing is ready to run; nothing was changed");
    };
    let this = std::env::current_exe().context("cannot find the Helix Toolbox executable")?;
    let events = session.start(
        InheritedRunner::default(),
        &catalog.lib_dir(),
        &this,
        Options::default(),
    )?;
    // The program writes to this terminal itself; the events add only what
    // the user should know, such as a start the history did not record.
    for event in events {
        match event {
            ExecutionEvent::Warning(message) => eprintln!("Warning: {message}"),
            ExecutionEvent::Completed(_) | ExecutionEvent::Interrupted(_) => break,
            _ => {}
        }
    }
    let finished = session.wait().context("the run did not start")?;
    if let Err(error) = session.record(None) {
        match HistoryPlace::Default.directory() {
            Some(directory) => eprintln!(
                "Helix Toolbox could not record this run in {}: {error}",
                directory.display()
            ),
            None => eprintln!("Helix Toolbox could not record this run: {error}"),
        }
    }
    for check in &finished.checks {
        println!("{}: {}: {}", check.label(), check.name, check.detail);
    }
    if !finished.unfinished().is_empty() {
        println!();
        for step in &finished.steps {
            println!("{}: {}", step.name, step.state.describe());
        }
        // What to run again: the entries whose steps did not finish.
        let retry = session.remaining(&|id| catalog.find(id).is_some());
        if !retry.is_empty() {
            println!(
                "To try the unfinished ones again: helix-toolbox run {}",
                retry.join(" ")
            );
        }
    }
    if !finished.outcome.succeeded() {
        bail!("the run stopped: {}", finished.describe());
    }
    if !finished.succeeded() {
        bail!("the run finished, but not everything it did could be verified");
    }
    Ok(())
}

/// Print a plan as it would be confirmed, without asking or running it.
pub fn print_plan(catalog: &Catalog, platform: &LinuxInfo, planned: &Plan) {
    print_prepared(catalog, platform, &prepare(catalog, platform, planned));
}

/// A plan as it would be confirmed, and what the checks before a run
/// found, as JSON.
pub fn print_plan_json(catalog: &Catalog, platform: &LinuxInfo, planned: &Plan) {
    print_json(catalog, platform, &prepare(catalog, platform, planned));
}

fn print_json(catalog: &Catalog, platform: &LinuxInfo, prepared: &Prepared) {
    let mut value = json::plan(&prepared.plan, &prepared.findings);
    if EFFECTS.load(Ordering::Relaxed) {
        value["effects"] = serde_json::to_value(effects(catalog, platform, &prepared.plan))
            .unwrap_or(serde_json::Value::Null);
    }
    json::print("plan", value);
}

/// Print the confirmation as text and, with --effects, what the plan
/// changes.
fn print_prepared(catalog: &Catalog, platform: &LinuxInfo, prepared: &Prepared) {
    for line in text(prepared) {
        println!("{line}");
    }
    if EFFECTS.load(Ordering::Relaxed) && !prepared.plan.is_empty() {
        println!("\nWhat it changes (✓ known, ~ expected, ? not known in advance):");
        for step in effects(catalog, platform, &prepared.plan) {
            println!("{}", step.name);
            for change in step.changes {
                println!("  {} {}", change.certainty.mark(), change.text);
                if let Some(diff) = change.diff.filter(|diff| diff.changed) {
                    for line in diff.unified().lines() {
                        println!("      {line}");
                    }
                }
            }
        }
        println!();
    }
}

/// The confirmation every interface shows, as lines of text: each step
/// numbered with its command and the sources tried after it, the notes and
/// what is not run, then what to know first (the risk, the checks, the
/// warnings, what happens after a failure). A plan that runs nothing says
/// why; a blocked one, what blocks it.
fn text(prepared: &Prepared) -> Vec<String> {
    let mut text = Vec::new();
    let steps = &prepared.plan.steps;
    let step = |text: &mut Vec<String>, index: usize, name: &str, detail: &str| {
        text.push(format!("{}. {name}", index + 1));
        text.push(format!("   {detail}"));
        if let Some(step) = steps.get(index) {
            text.extend(
                step.fallbacks()
                    .into_iter()
                    .map(|fallback| format!("   {fallback}")),
            );
        }
    };
    match &prepared.verdict {
        Verdict::Nothing(lines) => {
            text.extend(lines.iter().cloned());
            if lines.last().is_none_or(|last| last != "Nothing to run.") {
                text.push("Nothing to run.".to_string());
            }
        }
        Verdict::Blocked(lines) => {
            for (index, planned) in steps.iter().enumerate() {
                step(&mut text, index, planned.name(), &planned.display());
            }
            text.extend(prepared.plan.notes.iter().cloned());
            text.extend(
                prepared
                    .plan
                    .blocked
                    .iter()
                    .map(|blocked| format!("Not run: {blocked}")),
            );
            text.extend(lines.iter().cloned());
        }
        Verdict::Ready { header, lines } => {
            for (index, line) in lines.iter().enumerate() {
                if index < steps.len() {
                    step(&mut text, index, &line.name, &line.detail);
                } else {
                    text.push(line.name.clone());
                }
            }
            text.extend(header.iter().cloned());
        }
    }
    text
}

/// Ask for the confirmation word. When standard input is not a terminal, as
/// with `curl … | sh -s -- run <entry>`, ask on the controlling terminal.
fn confirm(word: &str) -> Result<String> {
    let prompt = format!("Type {word} to continue: ");
    let mut answer = String::new();
    if io::stdin().is_terminal() {
        print!("{prompt}");
        io::stdout().flush()?;
        io::stdin().read_line(&mut answer)?;
        return Ok(answer);
    }
    let Ok(mut terminal) = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
    else {
        bail!("confirmation needs a terminal; use --dry-run or --yes");
    };
    terminal.write_all(prompt.as_bytes())?;
    terminal.flush()?;
    io::BufReader::new(terminal).read_line(&mut answer)?;
    Ok(answer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use helix_toolbox_engine::{
        Machine, backups,
        confirmation::{Choices, prepare_with},
        plan_with, preflight,
        reliability::Evidence,
    };
    use std::path::Path;

    fn catalog() -> Catalog {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
        Catalog::from_directory(&fixtures, true).unwrap()
    }

    fn garuda() -> LinuxInfo {
        LinuxInfo::from_os_release("ID=garuda\nID_LIKE=arch\nPRETTY_NAME=\"Garuda Linux\"\n")
            .unwrap()
    }

    /// The fixture entries prepared as every interface prepares them, with
    /// the machine's answers made up.
    fn prepared(ids: &[&str], choices: Choices) -> Prepared {
        let catalog = catalog();
        let lookup = |id: &str| catalog.find(id);
        let entries = ids.iter().filter_map(|id| catalog.find(id));
        let blind = Machine {
            holds: &|_| false,
            on_path: &|_| false,
            ..Machine::blind()
        };
        let planned = plan_with(entries, &garuda(), &lookup, blind).unwrap();
        let facts = preflight::Facts {
            machine: blind,
            present: &|_| true,
            free_bytes: &|_| None,
            gpus: &Vec::new,
            repositories: &|| None,
            sizes: &|_, _| None,
            active_runs: &Vec::new,
        };
        prepare_with(
            planned,
            &garuda(),
            &lookup,
            choices,
            &facts,
            &Evidence::default(),
        )
    }

    #[test]
    fn the_command_line_prints_the_confirmation_every_interface_shows() {
        let ready = prepared(&["failing", "marker"], Choices::default());
        let Verdict::Ready { header, lines } = &ready.verdict else {
            panic!("ready: {ready:?}");
        };
        let shown = text(&ready);
        // Each step, numbered, with the command the confirmation lists.
        for (index, line) in lines.iter().enumerate() {
            let at = shown
                .iter()
                .position(|shown| *shown == format!("{}. {}", index + 1, line.name))
                .unwrap_or_else(|| panic!("{} in {shown:#?}", line.name));
            assert_eq!(shown[at + 1], format!("   {}", line.detail));
        }
        // Then what to know first, word for word, last.
        assert_eq!(shown[shown.len() - header.len()..], header[..]);
        assert!(header[0].starts_with("Risk: "), "{header:?}");
        assert_eq!(
            header.last().unwrap(),
            "After a failed step: stop (Settings changes it)"
        );

        // A run choice changes the plan and the line alike.
        let going = prepared(
            &["failing", "marker"],
            Choices {
                keep_going: true,
                snapshot: Some(backups::Snapshots::Snapper),
            },
        );
        assert!(going.plan.keep_going);
        assert!(
            text(&going)
                .last()
                .unwrap()
                .contains("continue with what does not need it")
        );
    }

    #[test]
    fn a_blocked_plan_shows_its_steps_and_what_blocks_it_and_an_empty_one_says_why() {
        let mut blocked = prepared(&["marker"], Choices::default());
        blocked.verdict = Verdict::Blocked(vec!["Blocked: the disk is full".into()]);
        let shown = text(&blocked);
        assert_eq!(shown[0], "1. Marker");
        assert_eq!(shown.last().unwrap(), "Blocked: the disk is full");

        let nothing = prepared(&[], Choices::default());
        assert_eq!(text(&nothing), ["Nothing to run."]);
        let mut noted = prepared(&[], Choices::default());
        noted.verdict = Verdict::Nothing(vec!["Ripgrep: already installed".into()]);
        assert_eq!(
            text(&noted),
            ["Ripgrep: already installed", "Nothing to run."]
        );
    }
}
