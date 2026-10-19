//! Plan, review and run entries from the fixture catalog, through a mock
//! runner and through a real shell, without touching the system.

use helix_toolbox_catalog::Catalog;
use helix_toolbox_engine::{InheritedRunner, MockRunner, Outcome, execute_in, history, plan};
use helix_toolbox_platform::LinuxInfo;
use std::path::{Path, PathBuf};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
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
    let mut runner = MockRunner::default();
    let outcome = execute_in(
        &planned,
        &catalog.lib_dir(),
        Path::new("/opt/helix-toolbox"),
        &mut runner,
        Some(state.path()),
    )
    .unwrap();
    assert_eq!(outcome.outcome, Outcome::Success);
    assert!(runner.programs[0].contains("marker.sh"));
    assert!(runner.programs[0].contains("'/opt/helix-toolbox' 'native' 'system-report'"));
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
    let outcome = execute_in(
        &planned,
        &catalog.lib_dir(),
        Path::new("/opt/helix-toolbox"),
        &mut InheritedRunner,
        Some(out.path()),
    )
    .unwrap();
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
    let outcome = execute_in(
        &planned,
        &catalog.lib_dir(),
        Path::new("/opt/helix-toolbox"),
        &mut InheritedRunner,
        Some(state.path()),
    )
    .unwrap();
    assert_eq!(outcome.outcome, Outcome::Failed { exit_code: 9 });
    let records = history::read_in(state.path(), 5).unwrap();
    assert!(!records[0].success);
    assert_eq!(records[0].result, "exit 9");
}
