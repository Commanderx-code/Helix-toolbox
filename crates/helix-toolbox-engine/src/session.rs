//! A run session: one confirmed plan's run, from the user's yes to its
//! record, the same for every interface.
//!
//! A [`RunSession`] holds the plan the confirmation made ready
//! ([`crate::confirmation::Prepared::session`]). Starting it checks the
//! plan's requirements, builds the shell program, marks the run in the
//! history and starts the program through a [`Runner`]; the session follows
//! it as [`ExecutionEvent`]s. When the program ends, the session concludes
//! the run once: it reads how each step ended, checks what finished, keeps
//! the outcome in the marker and keeps the checks for after the reboot. It
//! writes the record when the interface is done with it. Afterward it says
//! which queued entries remain and what an interface should read again.
//!
//! The command line runs through [`crate::InheritedRunner`], the terminal
//! interface and the window through [`crate::PtyRunner`], and tests through
//! [`crate::MockRunner`]: one lifecycle for all of them.

use crate::{
    EngineError, Finished, Outcome, Plan, StepState, check_requirements,
    execution::{self, ExecutionEvent, HistoryPlace, Options},
    history,
    runner::{Launch, Runner},
    shell_program_with, status_file, verification,
};
use serde::Serialize;
use std::{
    io,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
};

/// What an interface shows that a run can leave out of date.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reading {
    /// How entries stand: what is installed, and at which version.
    Statuses,
    /// What each package tool has waiting.
    Updates,
    /// What the tool screens read from the machine: services, storage,
    /// backups, configurations, repositories and the rest.
    Tools,
    /// The run history.
    History,
}

/// One confirmed plan's run. Every method takes `&self` except
/// [`start`](RunSession::start), and the session can be shared between
/// threads.
pub struct RunSession {
    plan: Plan,
    shared: Option<Arc<Shared>>,
}

/// What the session and the thread that follows the run share.
pub(crate) struct Shared {
    pub(crate) runner: Mutex<Box<dyn Runner>>,
    pub(crate) plan: Plan,
    /// Where the program writes each step's outcome.
    pub(crate) status: Option<PathBuf>,
    /// The concluded run, once the program has ended.
    pub(crate) finished: OnceLock<Finished>,
    book: Mutex<Book>,
    /// Set when the session is dropped: the thread that follows it stops.
    pub(crate) closed: AtomicBool,
}

/// The run's history, from its marker to the record.
struct Book {
    place: HistoryPlace,
    pending: Option<history::Pending>,
    recorded: bool,
}

impl RunSession {
    /// A plan the user confirmed, waiting to start. Interfaces get one from
    /// the confirmation.
    pub fn new(plan: Plan) -> Self {
        Self { plan, shared: None }
    }

    /// The plan it runs.
    pub fn plan(&self) -> &Plan {
        &self.plan
    }

    /// Check the plan's requirements and start its program through
    /// `runner`, with its history marker; the run's events arrive on the
    /// receiver. Errors are what keeps it from starting; a session starts
    /// once.
    pub fn start(
        &mut self,
        runner: impl Runner + 'static,
        lib_dir: &Path,
        this: &Path,
        options: Options,
    ) -> Result<Receiver<ExecutionEvent>, EngineError> {
        if self.shared.is_some() {
            return Err(EngineError::Spawn(io::Error::other(
                "this run has already started",
            )));
        }
        let plan = &self.plan;
        check_requirements(plan)?;
        tracing::info!(steps = ?plan.entries(), "run started");
        let status = status_file();
        let (bell, ring) = mpsc::sync_channel(1);
        let mut runner: Box<dyn Runner> = Box::new(runner);
        runner
            .start(Launch {
                program: &shell_program_with(plan, lib_dir, this, status.as_deref()),
                lib_dir,
                steps: status.as_deref(),
                rows: options.rows,
                cols: options.cols,
                bell,
            })
            .inspect_err(|_| {
                if let Some(status) = &status {
                    let _ = std::fs::remove_file(status);
                }
            })?;
        let entries = plan.entries();
        let mut first = vec![ExecutionEvent::RunStarted {
            entries: entries.clone(),
            steps: (0..plan.steps.len())
                .map(|index| execution::step_info(plan, index))
                .collect(),
        }];
        let begun = match &options.history {
            HistoryPlace::Default => Some(history::begin(&entries)),
            HistoryPlace::In(directory) => Some(history::begin_in(directory, &entries)),
            HistoryPlace::Off => None,
        };
        let pending = match begun {
            Some(Ok(pending)) => Some(pending),
            Some(Err(error)) => {
                tracing::warn!(%error, "the run start was not recorded");
                first.push(ExecutionEvent::Warning(format!(
                    "The start of this run was not recorded in the history: {error}"
                )));
                None
            }
            None => None,
        };
        if status.is_none() {
            first.push(ExecutionEvent::Warning(
                "No file for the steps' outcomes could be made, so the steps are not followed one by one".into(),
            ));
        }
        let shared = Arc::new(Shared {
            runner: Mutex::new(runner),
            plan: plan.clone(),
            status,
            finished: OnceLock::new(),
            book: Mutex::new(Book {
                place: options.history,
                pending,
                recorded: false,
            }),
            closed: AtomicBool::new(false),
        });
        self.shared = Some(shared.clone());
        execution::follow(shared, options.capacity, ring, first)
    }

    /// Send keyboard input to the program, as a terminal would: `\r` for
    /// Enter, `\x03` for Ctrl-C (which the program sees, unlike [`stop`]).
    ///
    /// [`stop`]: RunSession::stop
    pub fn write(&self, bytes: &[u8]) {
        if let Some(shared) = &self.shared {
            shared.runner.lock().unwrap().write(bytes);
        }
    }

    /// Give the terminal a new size; the program is told when it changes.
    pub fn resize(&self, rows: u16, cols: u16) {
        if let Some(shared) = &self.shared {
            shared.runner.lock().unwrap().resize(rows, cols);
        }
    }

    /// Stop the run. The first request hangs up every process in the run's
    /// process group, as a closing terminal does; a later one, while any of
    /// them is still running, kills them.
    pub fn stop(&self) {
        if let Some(shared) = &self.shared {
            shared.runner.lock().unwrap().stop();
        }
    }

    /// Whether a stop was requested.
    pub fn stop_requested(&self) -> bool {
        self.shared
            .as_ref()
            .is_some_and(|shared| shared.runner.lock().unwrap().stop_requested())
    }

    /// How many processes of the run are still alive, such as ones that
    /// ignored the hangup.
    pub fn survivors(&self) -> usize {
        self.shared
            .as_ref()
            .map_or(0, |shared| shared.runner.lock().unwrap().survivors())
    }

    /// Whether the run has ended and been concluded: true from just before
    /// its `Completed` or `Interrupted` event is sent.
    pub fn is_finished(&self) -> bool {
        self.shared
            .as_ref()
            .is_some_and(|shared| shared.finished.get().is_some())
    }

    /// How the run ended, once it has.
    pub fn finished(&self) -> Option<Finished> {
        self.shared
            .as_ref()
            .and_then(|shared| shared.finished.get().cloned())
    }

    /// Wait for the run to end and conclude it; None when it never started.
    pub fn wait(&self) -> Option<Finished> {
        let shared = self.shared.as_ref()?;
        loop {
            if let Some(finished) = shared.finished.get() {
                return Some(finished.clone());
            }
            if shared.runner.lock().unwrap().outcome().is_some() {
                return Some(conclude(shared));
            }
            std::thread::sleep(execution::TICK);
        }
    }

    /// Everything the program has written to its terminal so far, as the
    /// `Output` events carry it.
    pub fn transcript(&self) -> Vec<u8> {
        let Some(shared) = &self.shared else {
            return Vec::new();
        };
        let transcript = shared.runner.lock().unwrap().transcript();
        transcript.lock().unwrap().clone()
    }

    /// Write the run's history record, with a saved log if there is one,
    /// and the processes of the run still alive. Only the first call once
    /// the run has ended writes it; dropping the session writes it without a
    /// log if no call did. Until then the history's marker keeps the
    /// outcome, in case Helix Toolbox stops first.
    pub fn record(&self, log: Option<PathBuf>) -> io::Result<()> {
        let Some(shared) = &self.shared else {
            return Ok(());
        };
        let Some(finished) = shared.finished.get() else {
            return Ok(());
        };
        let left = self.survivors();
        let mut book = shared.book.lock().unwrap();
        if book.recorded {
            return Ok(());
        }
        book.recorded = true;
        tracing::info!(outcome = finished.describe(), "run finished");
        let mut result = finished.describe();
        if left > 0 {
            result.push_str(&format!("; {left} process(es) left running"));
        }
        let entries = shared.plan.entries();
        let written = match (book.pending.take(), &book.place) {
            (_, HistoryPlace::Off) => return Ok(()),
            (Some(pending), _) => pending.finish(finished.succeeded(), &result, log),
            (None, HistoryPlace::Default) => {
                history::record(&entries, finished.succeeded(), &result, log)
            }
            (None, HistoryPlace::In(directory)) => {
                history::record_in(directory, &entries, finished.succeeded(), &result, log)
            }
        };
        written.map(|_| ()).inspect_err(|error| {
            tracing::warn!(%error, "the run was not recorded");
        })
    }

    /// The queued entries that stay queued after the run: the ids of the
    /// steps that did not finish, each once, in the plan's order, of those
    /// `known` names an entry. Before the run ends, every step's.
    pub fn remaining(&self, known: &dyn Fn(&str) -> bool) -> Vec<String> {
        let finished = self.finished();
        let unfinished: Vec<&str> = match &finished {
            Some(finished) => finished
                .steps
                .iter()
                .filter(|step| step.state != StepState::Done)
                .map(|step| step.id.as_str())
                .collect(),
            None => self.plan.steps.iter().map(|step| step.id()).collect(),
        };
        let mut remaining: Vec<String> = Vec::new();
        for id in unfinished {
            if known(id) && !remaining.iter().any(|kept| kept == id) {
                remaining.push(id.to_string());
            }
        }
        remaining
    }

    /// What an interface should read again once the run is over: nothing
    /// before it starts, only the history when its program could not start,
    /// and otherwise everything a run can change.
    pub fn read_again(&self) -> Vec<Reading> {
        if self.shared.is_none() {
            return Vec::new();
        }
        match self.finished().map(|finished| finished.outcome) {
            Some(Outcome::CouldNotStart(_)) => vec![Reading::History],
            _ => vec![
                Reading::Statuses,
                Reading::Updates,
                Reading::Tools,
                Reading::History,
            ],
        }
    }
}

impl Drop for RunSession {
    fn drop(&mut self) {
        let Some(shared) = &self.shared else {
            return;
        };
        // A run that ended just now is still concluded and recorded.
        if shared.finished.get().is_none() && shared.runner.lock().unwrap().outcome().is_some() {
            conclude(shared);
        }
        let _ = self.record(None);
        shared.closed.store(true, Ordering::Release);
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        if let Some(status) = &self.status {
            let _ = std::fs::remove_file(status);
        }
    }
}

/// Conclude the ended run, once however many ask: read how each step
/// ended, check what finished, keep the outcome in the history's marker and
/// the checks for after the reboot beside the history.
pub(crate) fn conclude(shared: &Shared) -> Finished {
    shared
        .finished
        .get_or_init(|| {
            let outcome = shared
                .runner
                .lock()
                .unwrap()
                .outcome()
                .unwrap_or_else(|| Outcome::CouldNotStart("unknown".into()));
            let finished = Finished::from_run(&shared.plan, outcome, shared.status.as_deref());
            let mut book = shared.book.lock().unwrap();
            if let Some(pending) = &mut book.pending {
                let _ = pending.ended(finished.succeeded(), &finished.describe());
            }
            // Whether or not the marker could be written: the checks are
            // about the machine, not the record.
            if let Some(directory) = book.place.directory()
                && let Err(error) =
                    verification::keep(&directory, &shared.plan, &finished, history::now())
            {
                tracing::warn!(%error, "the checks for after the reboot were not kept");
            }
            finished
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EngineError, MockRunner, Step};
    use helix_toolbox_core::{Claim, Verify};
    use std::time::Duration;

    fn command(id: &str) -> Step {
        Step::Command {
            id: id.into(),
            name: id.to_uppercase(),
            line: format!("echo {id}"),
        }
    }

    fn plan(steps: Vec<Step>) -> Plan {
        Plan {
            steps,
            ..Plan::default()
        }
    }

    fn into(directory: &Path) -> Options {
        Options {
            history: HistoryPlace::In(directory.to_path_buf()),
            ..Options::default()
        }
    }

    /// Start a session through `runner` and take its events to the end.
    fn run(session: &mut RunSession, runner: MockRunner, options: Options) -> Vec<ExecutionEvent> {
        let events = session
            .start(runner, Path::new("/lib"), Path::new("/bin/hf"), options)
            .unwrap();
        let mut seen = Vec::new();
        loop {
            let event = events
                .recv_timeout(Duration::from_secs(10))
                .expect("the run should end");
            let end = matches!(
                event,
                ExecutionEvent::Completed(_) | ExecutionEvent::Interrupted(_)
            );
            seen.push(event);
            if end {
                return seen;
            }
        }
    }

    #[test]
    fn a_run_goes_through_the_runner_is_concluded_and_recorded_once() {
        let state = tempfile::tempdir().unwrap();
        let mut session = RunSession::new(plan(vec![command("hello")]));
        assert!(session.read_again().is_empty(), "nothing ran yet");
        let runner = MockRunner {
            outcome: Some(Outcome::Failed { exit_code: 4 }),
            steps: vec!["0 started 0".into(), "0 failed 4".into()],
            output: b"hello\r\n".to_vec(),
            ..MockRunner::default()
        };
        let programs = runner.programs.clone();
        let seen = run(&mut session, runner, into(state.path()));
        assert!(seen.contains(&ExecutionEvent::StepFailed {
            step: execution::step_info(session.plan(), 0),
            exit_code: 4,
            interrupted: false,
        }));
        assert!(seen.contains(&ExecutionEvent::Output {
            offset: 0,
            bytes: b"hello\r\n".to_vec(),
        }));
        let Some(ExecutionEvent::Completed(finished)) = seen.last() else {
            panic!("a failed run completes: {seen:#?}");
        };
        assert_eq!(finished.outcome, Outcome::Failed { exit_code: 4 });
        assert_eq!(finished.steps[0].state, StepState::Failed(4));
        assert_eq!(session.finished().as_ref(), Some(finished));
        assert_eq!(session.wait().as_ref(), Some(finished));
        let programs = programs.lock().unwrap();
        assert_eq!(programs.len(), 1);
        assert!(programs[0].contains("sh -e -c 'echo hello'"));
        // The marker keeps the outcome until the record is written.
        assert!(history::read_in(state.path(), 5).unwrap().is_empty());
        session.record(None).unwrap();
        session.record(None).unwrap();
        let records = history::read_in(state.path(), 5).unwrap();
        assert_eq!(records.len(), 1, "recorded once");
        assert_eq!(records[0].entries, ["hello"]);
        assert!(!records[0].success);
        assert_eq!(records[0].result, "exit 4");
    }

    #[test]
    fn the_command_line_waits_for_the_end_without_reading_the_events() {
        let state = tempfile::tempdir().unwrap();
        let mut session = RunSession::new(plan(vec![command("one"), command("two")]));
        let runner = MockRunner {
            steps: vec!["0 done 0".into(), "1 done 0".into()],
            ..MockRunner::default()
        };
        let events = session
            .start(
                runner,
                Path::new("/lib"),
                Path::new("/bin/hf"),
                into(state.path()),
            )
            .unwrap();
        drop(events);
        let finished = session.wait().unwrap();
        assert!(finished.succeeded());
        assert!(finished.unfinished().is_empty());
        session.record(None).unwrap();
        assert!(history::read_in(state.path(), 5).unwrap()[0].success);
    }

    #[test]
    fn processes_left_running_are_recorded_whatever_the_runner() {
        let state = tempfile::tempdir().unwrap();
        let mut session = RunSession::new(plan(vec![command("stubborn")]));
        let runner = MockRunner {
            left: 2,
            ..MockRunner::default()
        };
        let seen = run(&mut session, runner, into(state.path()));
        assert!(seen.contains(&ExecutionEvent::Warning(
            "2 process(es) of this run are still running".into()
        )));
        drop(session);
        let records = history::read_in(state.path(), 5).unwrap();
        assert_eq!(records.len(), 1, "dropping the session records it");
        assert!(
            records[0].result.ends_with("; 2 process(es) left running"),
            "{}",
            records[0].result
        );
    }

    /// A script step whose entry claims `path` only after the next reboot.
    fn after_a_reboot(path: &Path) -> Step {
        let mut expect = crate::Expected::default();
        expect.checks.push(Verify {
            claim: Claim::Path {
                path: path.display().to_string(),
                is: None,
                absent: false,
            },
            when_exists: None,
            after_reboot: true,
        });
        Step::Script {
            id: "boot-theme".into(),
            name: "Boot theme".into(),
            file: PathBuf::from("/lib/boot-theme.sh"),
            program: "bash".into(),
            args: Vec::new(),
            interactive: false,
            expect,
        }
    }

    #[test]
    fn checks_for_after_the_reboot_are_kept_even_without_a_marker() {
        let state = tempfile::tempdir().unwrap();
        let claimed = state.path().join("theme");
        let done = || MockRunner {
            steps: vec!["0 done 0".into()],
            ..MockRunner::default()
        };
        let mut session = RunSession::new(plan(vec![after_a_reboot(&claimed)]));
        let seen = run(&mut session, done(), into(state.path()));
        let Some(ExecutionEvent::Completed(finished)) = seen.last() else {
            panic!("completed: {seen:#?}");
        };
        assert!(
            finished.checks.iter().all(|check| check.later),
            "{finished:?}"
        );
        let kept = verification::kept(&crate::doctor::Here, state.path());
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].entry, "boot-theme");

        // The history's marker cannot be written (its folder is a file);
        // the checks are still kept.
        let elsewhere = tempfile::tempdir().unwrap();
        std::fs::write(elsewhere.path().join("running"), "").unwrap();
        let mut session = RunSession::new(plan(vec![after_a_reboot(&claimed)]));
        let seen = run(&mut session, done(), into(elsewhere.path()));
        assert!(
            seen.iter().any(|event| matches!(
                event,
                ExecutionEvent::Warning(message) if message.starts_with("The start of this run was not recorded")
            )),
            "{seen:#?}"
        );
        assert_eq!(
            verification::kept(&crate::doctor::Here, elsewhere.path()).len(),
            1
        );

        // Without a history, nothing is kept anywhere.
        let mut session = RunSession::new(plan(vec![after_a_reboot(&claimed)]));
        let quiet = Options {
            history: HistoryPlace::Off,
            ..Options::default()
        };
        run(&mut session, done(), quiet);
        session.record(None).unwrap();
    }

    #[test]
    fn what_did_not_finish_stays_queued_and_what_changed_is_read_again() {
        let state = tempfile::tempdir().unwrap();
        let snapshot = Step::Shell {
            id: "snapshot".into(),
            name: "Take a Snapper snapshot".into(),
            program: "true".into(),
            display: "true".into(),
            requires: Vec::new(),
            changes: Vec::new(),
        };
        let mut session = RunSession::new(plan(vec![
            snapshot,
            command("ripgrep"),
            command("fzf"),
            command("ripgrep"),
        ]));
        let entries = |id: &str| id != "snapshot";
        assert_eq!(
            session.remaining(&entries),
            ["ripgrep", "fzf"],
            "before the run, everything remains"
        );
        let runner = MockRunner {
            outcome: Some(Outcome::Failed { exit_code: 1 }),
            steps: vec!["0 done 0".into(), "1 done 0".into(), "2 failed 1".into()],
            ..MockRunner::default()
        };
        run(&mut session, runner, into(state.path()));
        // fzf failed and the second ripgrep step did not run, so both stay,
        // in the plan's order; the snapshot is no entry.
        assert_eq!(session.remaining(&entries), ["fzf", "ripgrep"]);
        assert_eq!(
            session.read_again(),
            [
                Reading::Statuses,
                Reading::Updates,
                Reading::Tools,
                Reading::History
            ]
        );

        let mut session = RunSession::new(plan(vec![command("ripgrep")]));
        let runner = MockRunner {
            steps: vec!["0 done 0".into()],
            ..MockRunner::default()
        };
        run(&mut session, runner, into(state.path()));
        assert!(session.remaining(&entries).is_empty(), "it finished");

        // A program that could not start changed nothing but the history.
        let mut session = RunSession::new(plan(vec![command("ripgrep")]));
        let runner = MockRunner {
            outcome: Some(Outcome::CouldNotStart("no shell".into())),
            ..MockRunner::default()
        };
        run(&mut session, runner, into(state.path()));
        assert_eq!(session.remaining(&entries), ["ripgrep"]);
        assert_eq!(session.read_again(), [Reading::History]);
    }

    #[test]
    fn a_stopped_run_is_interrupted_and_checks_nothing() {
        let mut session = RunSession::new(plan(vec![command("one")]));
        let runner = MockRunner {
            steps: vec!["0 started 0".into()],
            ..MockRunner::default()
        };
        // Stopped before the session concludes it.
        let mut runner = runner;
        runner.stop();
        let seen = run(
            &mut session,
            runner,
            Options {
                history: HistoryPlace::Off,
                ..Options::default()
            },
        );
        let Some(ExecutionEvent::Interrupted(finished)) = seen.last() else {
            panic!("interrupted: {seen:#?}");
        };
        assert_eq!(finished.outcome, Outcome::Interrupted);
        assert!(finished.checks.is_empty());
        assert!(session.stop_requested());
    }

    #[test]
    fn missing_requirements_stop_a_run_before_it_starts_and_a_session_starts_once() {
        let mut needs = plan(vec![command("one")]);
        needs.requirements = vec!["helix-toolbox-no-such-tool".into()];
        let runner = MockRunner::default();
        let programs = runner.programs.clone();
        let mut session = RunSession::new(needs);
        let error = session
            .start(runner, Path::new("/"), Path::new("/"), Options::default())
            .err()
            .unwrap();
        assert!(matches!(error, EngineError::MissingExecutable(_)));
        assert!(programs.lock().unwrap().is_empty());
        assert!(session.wait().is_none(), "it never started");

        let mut session = RunSession::new(plan(vec![command("one")]));
        let quiet = Options {
            history: HistoryPlace::Off,
            ..Options::default()
        };
        run(&mut session, MockRunner::default(), quiet.clone());
        assert!(
            session
                .start(MockRunner::default(), Path::new("/"), Path::new("/"), quiet)
                .is_err()
        );
    }

    #[test]
    fn the_session_can_be_shared_between_threads() {
        fn shared<T: Send + Sync>() {}
        shared::<RunSession>();
    }
}
