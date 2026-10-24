//! Plan, review and run entries from the fixture catalog, through a mock
//! runner and through a real shell, without touching the system. Each run
//! goes through the run session every interface uses.

use helix_toolbox_catalog::Catalog;
use helix_toolbox_engine::{
    Finished, InheritedRunner, MockRunner, Outcome, Plan, RunSession, Runner,
    execution::{HistoryPlace, Options},
    history, plan,
};
use helix_toolbox_platform::LinuxInfo;
use std::path::{Path, PathBuf};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

/// Run a plan as the command line does: start its session, wait for the
/// end and record it, in the history in `state`.
fn run(planned: &Plan, catalog: &Catalog, runner: impl Runner + 'static, state: &Path) -> Finished {
    let mut session = RunSession::new(planned.clone());
    let options = Options {
        history: HistoryPlace::In(state.to_path_buf()),
        ..Options::default()
    };
    let _events = session
        .start(
            runner,
            &catalog.lib_dir(),
            Path::new("/opt/helix-toolbox"),
            options,
        )
        .unwrap();
    let finished = session.wait().unwrap();
    session.record(None).unwrap();
    finished
}

fn platform() -> LinuxInfo {
    LinuxInfo::from_os_release("ID=garuda\nID_LIKE=arch\nPRETTY_NAME=\"Garuda Linux\"\n").unwrap()
}

#[test]
fn a_profile_plans_into_its_members_and_is_reviewed_through_a_mock() {
    let catalog = Catalog::from_directory(&fixtures(), true).unwrap();
    let bundle = catalog.find("bundle").unwrap();
    let planned = plan([bundle], &platform(), &|id| catalog.find(id)).unwrap();
    assert_eq!(planned.steps.len(), 2);
    assert_eq!(planned.steps[0].id(), "marker");
    assert_eq!(planned.steps[1].id(), "report");
    assert!(!planned.privileged);
    assert!(planned.steps[0].display().contains("marker.sh"));
    let state = tempfile::tempdir().unwrap();
    let runner = MockRunner::default();
    let programs = runner.programs.clone();
    let outcome = run(&planned, &catalog, runner, state.path());
    assert_eq!(outcome.outcome, Outcome::Success);
    let programs = programs.lock().unwrap();
    assert!(programs[0].contains("marker.sh"));
    assert!(programs[0].contains("'/opt/helix-toolbox' 'native' 'system-report'"));
    let records = history::read_in(state.path(), 5).unwrap();
    assert_eq!(records[0].entries, ["marker", "report"]);
    assert!(records[0].success);
}

#[test]
fn a_script_entry_runs_in_a_real_shell_with_the_helpers() {
    let catalog = Catalog::from_directory(&fixtures(), true).unwrap();
    let marker = catalog.find("marker").unwrap();
    let planned = plan([marker], &platform(), &|id| catalog.find(id)).unwrap();
    let out = tempfile::tempdir().unwrap();
    let marker_file = out.path().join("marker.txt");
    // SAFETY: this variable is read only by the child shell started below.
    unsafe { std::env::set_var("HELIX_TOOLBOX_TEST_OUT", &marker_file) };
    let outcome = run(&planned, &catalog, InheritedRunner::default(), out.path());
    assert_eq!(outcome.outcome, Outcome::Success);
    let text = std::fs::read_to_string(&marker_file).unwrap();
    assert!(text.starts_with("marker from "));
    assert!(text.contains("scripts/installers"));
    let records = history::read_in(out.path(), 5).unwrap();
    assert_eq!(records[0].result, "exit 0");
}

#[test]
fn a_failing_entry_is_recorded_as_failed_with_its_status() {
    let catalog = Catalog::from_directory(&fixtures(), true).unwrap();
    let failing = catalog.find("failing").unwrap();
    let planned = plan([failing], &platform(), &|id| catalog.find(id)).unwrap();
    let state = tempfile::tempdir().unwrap();
    let outcome = run(&planned, &catalog, InheritedRunner::default(), state.path());
    assert_eq!(outcome.outcome, Outcome::Failed { exit_code: 9 });
    let records = history::read_in(state.path(), 5).unwrap();
    assert!(!records[0].success);
    assert_eq!(records[0].result, "exit 9");
}

#[test]
fn a_confirmed_queue_runs_through_its_session_and_keeps_what_did_not_finish() {
    use helix_toolbox_engine::{
        Machine,
        confirmation::{Choices, prepare_with},
        plan_with, preflight,
        reliability::Evidence,
        session::Reading,
    };
    let catalog = Catalog::from_directory(&fixtures(), true).unwrap();
    let lookup = |id: &str| catalog.find(id);
    // The queue: a profile of two entries, then one that fails first.
    let queue = ["failing", "bundle"].map(|id| catalog.find(id).unwrap());
    let blind = Machine {
        holds: &|_| false,
        on_path: &|_| false,
        ..Machine::blind()
    };
    let planned = plan_with(queue, &platform(), &lookup, blind).unwrap();
    let facts = preflight::Facts {
        machine: blind,
        present: &|_| true,
        free_bytes: &|_| None,
        gpus: &Vec::new,
        repositories: &|| None,
        sizes: &|_, _| None,
        active_runs: &Vec::new,
    };
    let prepared = prepare_with(
        planned,
        &platform(),
        &lookup,
        Choices::default(),
        &facts,
        &Evidence::default(),
    );
    let mut session = prepared.session().expect("ready to run");
    let ids: Vec<&str> = session.plan().steps.iter().map(|step| step.id()).collect();
    assert_eq!(ids, ["failing", "marker", "report"]);
    let state = tempfile::tempdir().unwrap();
    let runner = MockRunner {
        outcome: Some(Outcome::Failed { exit_code: 9 }),
        steps: vec!["0 failed 9".into()],
        ..MockRunner::default()
    };
    let _events = session
        .start(
            runner,
            &catalog.lib_dir(),
            Path::new("/opt/helix-toolbox"),
            Options {
                history: HistoryPlace::In(state.path().to_path_buf()),
                ..Options::default()
            },
        )
        .unwrap();
    let finished = session.wait().unwrap();
    assert_eq!(finished.outcome, Outcome::Failed { exit_code: 9 });
    session.record(None).unwrap();
    // The profile's members stay queued in its place, as the terminal
    // interface and the window both keep them.
    let known = |id: &str| catalog.find(id).is_some();
    assert_eq!(session.remaining(&known), ["failing", "marker", "report"]);
    assert!(session.read_again().contains(&Reading::Updates));
    let records = history::read_in(state.path(), 5).unwrap();
    assert_eq!(records[0].result, "exit 9, 0 of 3 steps done");
}
