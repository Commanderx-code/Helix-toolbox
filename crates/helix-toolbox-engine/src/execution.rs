//! A run of a plan as a stream of events, for any interface to show.
//!
//! [`Execution::start`] runs a plan's program in a pseudo-terminal and
//! returns a handle with a bounded channel of [`ExecutionEvent`]s. The handle
//! is what an interface acts through: keyboard input, the terminal's size,
//! stopping the run and writing its history record. The events are what it
//! shows: the run and each step starting and ending, the terminal's output as
//! it arrives, progress, warnings, and how the run ended, with its checks.
//!
//! A consumer that falls behind never holds the run up. Output waits in the
//! run's transcript until the channel has room and then arrives in larger
//! pieces, each saying where in the transcript it starts; the other events
//! wait in order behind it. Nothing is dropped. A consumer that goes away
//! (drops the receiver) leaves the run going and its record still written.
//!
//! The order is the run's: `RunStarted` first, a step's `StepStarted` before
//! its end, and `Completed` or `Interrupted` after the output that came
//! before the program ended. Step events come from the program's step file
//! and output from the terminal, read side by side every few milliseconds,
//! so a step's first or last bytes can fall on the other side of its event.
//! Output can follow the end when processes the run left behind still write.

use crate::{
    EngineError, Finished, Outcome, Plan, StepState, check_requirements, history, pty::Session,
    shell_program_with, status_file,
};
use serde::{Serialize, Serializer};
use std::{
    collections::VecDeque,
    io,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError},
    },
    time::{Duration, Instant},
};

/// How many events the channel holds before the rest wait for room.
pub const DEFAULT_CAPACITY: usize = 256;

/// The most output one event carries.
pub const OUTPUT_CHUNK: usize = 64 * 1024;

/// How often the run is looked at when nothing wakes the monitor sooner.
const TICK: Duration = Duration::from_millis(10);

/// How long the end of a run waits for output still on its way from the
/// terminal, when something the run started keeps it open.
const DRAIN: Duration = Duration::from_millis(250);

/// One step of the plan, as the events name it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StepInfo {
    /// Its position in the plan, from 0.
    pub index: usize,
    pub id: String,
    pub name: String,
}

/// Something that happened in a run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecutionEvent {
    /// Always the first event: the entries and the steps, in order.
    RunStarted {
        entries: Vec<(String, String)>,
        steps: Vec<StepInfo>,
    },
    StepStarted(StepInfo),
    StepCompleted(StepInfo),
    /// The step ended with a non-zero status; `interrupted` when a signal
    /// ended it, which also ends the run.
    StepFailed {
        step: StepInfo,
        exit_code: u32,
        interrupted: bool,
    },
    /// Not run, because a step it needs did not finish.
    StepSkipped(StepInfo),
    /// Terminal output: the transcript's bytes from `offset` on, exactly as
    /// the program wrote them, escape sequences included.
    Output {
        offset: u64,
        bytes: Vec<u8>,
    },
    /// After each step ends, however it ended: how many have, of all.
    Progress {
        ended: usize,
        total: usize,
    },
    /// Something the user should know that does not stop the run.
    Warning(String),
    /// The run ended by itself: it succeeded, failed or could not start.
    /// The checks of the steps that finished are done.
    Completed(Finished),
    /// The run was stopped, by request or by a signal; nothing is checked.
    Interrupted(Finished),
}

/// Where a run's history record goes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HistoryPlace {
    /// The user's history, as every interface keeps it.
    Default,
    /// The history in this state directory.
    In(PathBuf),
    /// No record, for tests and previews.
    Off,
}

/// How to start a run.
#[derive(Clone, Debug)]
pub struct Options {
    /// The terminal's first size; [`Execution::resize`] changes it.
    pub rows: u16,
    pub cols: u16,
    /// How many events the channel holds; at least 1.
    pub capacity: usize,
    pub history: HistoryPlace,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            rows: 24,
            cols: 80,
            capacity: DEFAULT_CAPACITY,
            history: HistoryPlace::Default,
        }
    }
}

/// A running plan. Every method takes `&self`, and the handle can be shared
/// between threads.
pub struct Execution {
    shared: Arc<Shared>,
}

struct Shared {
    session: Mutex<Session>,
    plan: Plan,
    /// Where the program writes each step's outcome.
    status: Option<PathBuf>,
    /// The finished run, once the monitor has seen the program end.
    finished: OnceLock<Finished>,
    history: Mutex<Book>,
    /// Set when the handle is dropped: the monitor stops.
    closed: AtomicBool,
}

/// The run's history record, from its marker to the record.
struct Book {
    place: HistoryPlace,
    pending: Option<history::Pending>,
    recorded: bool,
}

impl Execution {
    /// Check the plan's requirements and start its program, with its
    /// history marker. Errors are what keeps it from starting.
    pub fn start(
        plan: &Plan,
        lib_dir: &Path,
        this: &Path,
        options: Options,
    ) -> Result<(Self, Receiver<ExecutionEvent>), EngineError> {
        check_requirements(plan)?;
        tracing::info!(steps = ?plan.entries(), "run started");
        let status = status_file();
        let (bell, ring) = mpsc::sync_channel(1);
        let session = Session::spawn_sized(
            &shell_program_with(plan, lib_dir, this, status.as_deref()),
            lib_dir,
            options.rows,
            options.cols,
            Some(bell),
        )
        .inspect_err(|_| {
            if let Some(status) = &status {
                let _ = std::fs::remove_file(status);
            }
        })?;
        let entries = plan.entries();
        let mut first = vec![ExecutionEvent::RunStarted {
            entries: entries.clone(),
            steps: (0..plan.steps.len())
                .map(|index| step_info(plan, index))
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
            session: Mutex::new(session),
            plan: plan.clone(),
            status,
            finished: OnceLock::new(),
            history: Mutex::new(Book {
                place: options.history,
                pending,
                recorded: false,
            }),
            closed: AtomicBool::new(false),
        });
        let (sender, receiver) = mpsc::sync_channel(options.capacity.max(1));
        let monitored = shared.clone();
        std::thread::Builder::new()
            .name("helix-toolbox-run".into())
            .spawn(move || monitor(&monitored, sender, &ring, first))
            .map_err(EngineError::Spawn)?;
        Ok((Self { shared }, receiver))
    }

    /// Send keyboard input to the program, as a terminal would: `\r` for
    /// Enter, `\x03` for Ctrl-C (which the program sees, unlike [`stop`]).
    ///
    /// [`stop`]: Execution::stop
    pub fn write(&self, bytes: &[u8]) {
        self.shared.session.lock().unwrap().write(bytes);
    }

    /// Give the terminal a new size; the program is told when it changes.
    pub fn resize(&self, rows: u16, cols: u16) {
        self.shared.session.lock().unwrap().resize(rows, cols);
    }

    /// Stop the run. The first request hangs up every process in the run's
    /// process group, as a closing terminal does; a later one, while any of
    /// them is still running, kills them.
    pub fn stop(&self) {
        self.shared.session.lock().unwrap().kill();
    }

    /// Whether a stop was requested.
    pub fn stop_requested(&self) -> bool {
        self.shared.session.lock().unwrap().stop_requested()
    }

    /// How many processes of the run are still alive, such as ones that
    /// ignored the hangup.
    pub fn survivors(&self) -> usize {
        self.shared.session.lock().unwrap().survivors()
    }

    /// Whether the run has ended and been checked: true from just before
    /// its `Completed` or `Interrupted` event is sent.
    pub fn is_finished(&self) -> bool {
        self.shared.finished.get().is_some()
    }

    /// How the run ended, once it has.
    pub fn finished(&self) -> Option<Finished> {
        self.shared.finished.get().cloned()
    }

    /// Everything the program has written so far, as the `Output` events
    /// carry it.
    pub fn transcript(&self) -> Vec<u8> {
        let transcript = self.shared.session.lock().unwrap().transcript();
        transcript.lock().unwrap().clone()
    }

    /// Write the run's history record, with a saved log if there is one.
    /// Only the first call once the run has ended writes it; dropping the
    /// handle writes it without a log if no call did. Until then the
    /// history's marker keeps the outcome, in case Helix Toolbox stops first.
    pub fn record(&self, log: Option<PathBuf>) -> io::Result<()> {
        let Some(finished) = self.shared.finished.get() else {
            return Ok(());
        };
        let left = self.survivors();
        let mut book = self.shared.history.lock().unwrap();
        if book.recorded {
            return Ok(());
        }
        book.recorded = true;
        tracing::info!(outcome = finished.describe(), "run finished");
        let mut result = finished.describe();
        if left > 0 {
            result.push_str(&format!("; {left} process(es) left running"));
        }
        let entries = self.shared.plan.entries();
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
}

impl Drop for Execution {
    fn drop(&mut self) {
        // A run that ended just now is still checked and recorded.
        if self.shared.finished.get().is_none() && self.shared.session.lock().unwrap().is_finished()
        {
            conclude(&self.shared);
        }
        let _ = self.record(None);
        self.shared.closed.store(true, Ordering::Release);
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        if let Some(status) = &self.status {
            let _ = std::fs::remove_file(status);
        }
    }
}

fn step_info(plan: &Plan, index: usize) -> StepInfo {
    let step = &plan.steps[index];
    StepInfo {
        index,
        id: step.id().to_string(),
        name: step.name().to_string(),
    }
}

/// Follows the run until it has ended and everything is sent, or until the
/// handle is dropped.
fn monitor(
    shared: &Shared,
    sender: SyncSender<ExecutionEvent>,
    ring: &Receiver<()>,
    first: Vec<ExecutionEvent>,
) {
    let transcript = shared.session.lock().unwrap().transcript();
    let mut outlet = Outlet {
        sender,
        connected: true,
        sent: 0,
        queue: VecDeque::new(),
        transcript,
    };
    // Before any output, however much the program has written already.
    for event in first {
        outlet.next(event);
    }
    let mut steps = StepFile {
        path: shared.status.clone(),
        read: 0,
        ended: 0,
    };
    let mut exited: Option<Instant> = None;
    let mut announced = false;
    loop {
        if let Err(RecvTimeoutError::Disconnected) = ring.recv_timeout(TICK) {
            // The terminal is closed; nothing rings any more.
            std::thread::sleep(TICK);
        }
        if shared.closed.load(Ordering::Acquire) {
            return;
        }
        for (event, ends) in steps.read(&shared.plan) {
            if ends {
                outlet.after_output_so_far(event);
            } else {
                outlet.next(event);
            }
        }
        let (ended, reading) = {
            let session = shared.session.lock().unwrap();
            (session.is_finished(), session.reading())
        };
        if ended && !announced {
            let since = *exited.get_or_insert_with(Instant::now);
            if reading && since.elapsed() < DRAIN {
                outlet.pump();
                continue;
            }
            for (event, _) in steps.read(&shared.plan) {
                outlet.after_output_so_far(event);
            }
            let finished = conclude(shared);
            let left = shared.session.lock().unwrap().survivors();
            if left > 0 {
                outlet.after_output_so_far(ExecutionEvent::Warning(format!(
                    "{left} process(es) of this run are still running"
                )));
            }
            outlet.after_output_so_far(if finished.outcome == Outcome::Interrupted {
                ExecutionEvent::Interrupted(finished)
            } else {
                ExecutionEvent::Completed(finished)
            });
            announced = true;
        }
        outlet.pump();
        if announced && !reading && outlet.is_empty() {
            return;
        }
    }
}

/// Check the ended run and keep its outcome in the history's marker, once
/// however many ask.
fn conclude(shared: &Shared) -> Finished {
    shared
        .finished
        .get_or_init(|| {
            let outcome = shared
                .session
                .lock()
                .unwrap()
                .outcome()
                .unwrap_or_else(|| Outcome::CouldNotStart("unknown".into()));
            let finished = Finished::from_run(&shared.plan, outcome, shared.status.as_deref());
            if let Some(pending) = &mut shared.history.lock().unwrap().pending {
                let _ = pending.ended(finished.succeeded(), &finished.describe());
            }
            finished
        })
        .clone()
}

/// The step lines the run's program has written so far.
struct StepFile {
    path: Option<PathBuf>,
    /// How much of the file has been read, in whole lines.
    read: usize,
    ended: usize,
}

impl StepFile {
    /// The events of the lines added since the last read, each with whether
    /// it ends a step (and so comes after the output before it).
    fn read(&mut self, plan: &Plan) -> Vec<(ExecutionEvent, bool)> {
        let Some(text) = self
            .path
            .as_deref()
            .and_then(|path| std::fs::read(path).ok())
        else {
            return Vec::new();
        };
        let Some(new) = text.get(self.read..) else {
            return Vec::new();
        };
        // Only whole lines: the program may be writing the next one.
        let Some(whole) = new.iter().rposition(|&byte| byte == b'\n') else {
            return Vec::new();
        };
        self.read += whole + 1;
        let mut events = Vec::new();
        for line in String::from_utf8_lossy(&new[..whole]).lines() {
            let mut words = line.split_whitespace();
            let (Some(index), Some(state), Some(code)) = (words.next(), words.next(), words.next())
            else {
                continue;
            };
            let (Ok(index), Ok(code)) = (index.parse::<usize>(), code.parse::<u32>()) else {
                continue;
            };
            if index >= plan.steps.len() {
                continue;
            }
            let step = step_info(plan, index);
            let event = match state {
                "started" => {
                    events.push((ExecutionEvent::StepStarted(step), false));
                    continue;
                }
                "done" => ExecutionEvent::StepCompleted(step),
                "failed" | "interrupted" => ExecutionEvent::StepFailed {
                    step,
                    exit_code: code,
                    interrupted: state == "interrupted",
                },
                "skipped" => ExecutionEvent::StepSkipped(step),
                _ => continue,
            };
            self.ended += 1;
            events.push((event, true));
            events.push((
                ExecutionEvent::Progress {
                    ended: self.ended,
                    total: plan.steps.len(),
                },
                true,
            ));
        }
        events
    }
}

/// The sending end: output from the transcript, other events in order
/// behind the output that came before them, all without waiting.
struct Outlet {
    sender: SyncSender<ExecutionEvent>,
    connected: bool,
    /// How much of the transcript has been sent.
    sent: usize,
    /// Events waiting for room, each after the transcript's first so many
    /// bytes.
    queue: VecDeque<(usize, ExecutionEvent)>,
    transcript: Arc<Mutex<Vec<u8>>>,
}

impl Outlet {
    /// Queue an event to follow the output so far.
    fn after_output_so_far(&mut self, event: ExecutionEvent) {
        let length = self.transcript.lock().unwrap().len();
        self.queue.push_back((length, event));
    }

    /// Queue an event to follow only the output already promised before it.
    fn next(&mut self, event: ExecutionEvent) {
        let after = self.queue.back().map_or(self.sent, |(after, _)| *after);
        self.queue.push_back((after.max(self.sent), event));
    }

    fn is_empty(&self) -> bool {
        !self.connected
            || (self.queue.is_empty() && self.sent >= self.transcript.lock().unwrap().len())
    }

    /// Send what there is room for, in order.
    fn pump(&mut self) {
        while self.connected {
            let limit = match self.queue.front() {
                Some((after, _)) if self.sent >= *after => {
                    let (after, event) = self.queue.pop_front().unwrap();
                    if let Some(event) = self.offer(event) {
                        self.queue.push_front((after, event));
                        return;
                    }
                    continue;
                }
                Some((after, _)) => *after,
                None => usize::MAX,
            };
            let bytes = {
                let transcript = self.transcript.lock().unwrap();
                let end = transcript.len().min(limit).min(self.sent + OUTPUT_CHUNK);
                if end <= self.sent {
                    return;
                }
                transcript[self.sent..end].to_vec()
            };
            let length = bytes.len();
            let event = ExecutionEvent::Output {
                offset: self.sent as u64,
                bytes,
            };
            if self.offer(event).is_some() {
                return;
            }
            self.sent += length;
        }
    }

    /// Send an event if there is room; give it back if not.
    fn offer(&mut self, event: ExecutionEvent) -> Option<ExecutionEvent> {
        match self.sender.try_send(event) {
            Ok(()) => None,
            Err(TrySendError::Full(event)) => Some(event),
            Err(TrySendError::Disconnected(_)) => {
                self.connected = false;
                self.queue.clear();
                None
            }
        }
    }
}

// ----- the events as JSON, for an interface in another language -----

#[derive(Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum Wire<'a> {
    RunStarted {
        entries: Vec<WireEntry<'a>>,
        steps: Vec<WireStep<'a>>,
    },
    StepStarted {
        step: WireStep<'a>,
    },
    StepCompleted {
        step: WireStep<'a>,
    },
    StepFailed {
        step: WireStep<'a>,
        exit_code: u32,
        interrupted: bool,
    },
    StepSkipped {
        step: WireStep<'a>,
    },
    Output {
        offset: u64,
        bytes: &'a [u8],
    },
    Progress {
        ended: usize,
        total: usize,
    },
    Warning {
        message: &'a str,
    },
    Completed {
        #[serde(flatten)]
        result: WireResult<'a>,
    },
    Interrupted {
        #[serde(flatten)]
        result: WireResult<'a>,
    },
}

#[derive(Serialize)]
struct WireEntry<'a> {
    id: &'a str,
    name: &'a str,
}

#[derive(Serialize)]
struct WireStep<'a> {
    index: usize,
    id: &'a str,
    name: &'a str,
}

impl<'a> From<&'a StepInfo> for WireStep<'a> {
    fn from(step: &'a StepInfo) -> Self {
        Self {
            index: step.index,
            id: &step.id,
            name: &step.name,
        }
    }
}

#[derive(Serialize)]
struct WireResult<'a> {
    outcome: WireOutcome<'a>,
    /// As the history records it.
    description: String,
    succeeded: bool,
    verified: bool,
    steps: Vec<WireStepResult<'a>>,
    checks: Vec<WireCheck<'a>>,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum WireOutcome<'a> {
    Success,
    Failed { exit_code: u32 },
    Interrupted,
    CouldNotStart { reason: &'a str },
}

#[derive(Serialize)]
struct WireStepResult<'a> {
    index: usize,
    id: &'a str,
    name: &'a str,
    state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_code: Option<u32>,
}

#[derive(Serialize)]
struct WireCheck<'a> {
    name: &'a str,
    passed: bool,
    detail: &'a str,
}

impl<'a> From<&'a Finished> for WireResult<'a> {
    fn from(finished: &'a Finished) -> Self {
        Self {
            outcome: match &finished.outcome {
                Outcome::Success => WireOutcome::Success,
                Outcome::Failed { exit_code } => WireOutcome::Failed {
                    exit_code: *exit_code,
                },
                Outcome::Interrupted => WireOutcome::Interrupted,
                Outcome::CouldNotStart(reason) => WireOutcome::CouldNotStart { reason },
            },
            description: finished.describe(),
            succeeded: finished.succeeded(),
            verified: finished.verified(),
            steps: finished
                .steps
                .iter()
                .enumerate()
                .map(|(index, step)| {
                    let (state, exit_code) = match &step.state {
                        StepState::Done => ("done", None),
                        StepState::Failed(code) => ("failed", Some(*code)),
                        StepState::Interrupted => ("interrupted", None),
                        StepState::Skipped => ("skipped", None),
                        StepState::NotRun => ("not_run", None),
                    };
                    WireStepResult {
                        index,
                        id: &step.id,
                        name: &step.name,
                        state,
                        exit_code,
                    }
                })
                .collect(),
            checks: finished
                .checks
                .iter()
                .map(|check| WireCheck {
                    name: &check.name,
                    passed: check.passed,
                    detail: &check.detail,
                })
                .collect(),
        }
    }
}

/// The JSON form documented in docs/EVENTS.md: an object whose `event`
/// names the kind, in snake case.
impl Serialize for ExecutionEvent {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let wire = match self {
            ExecutionEvent::RunStarted { entries, steps } => Wire::RunStarted {
                entries: entries
                    .iter()
                    .map(|(id, name)| WireEntry { id, name })
                    .collect(),
                steps: steps.iter().map(WireStep::from).collect(),
            },
            ExecutionEvent::StepStarted(step) => Wire::StepStarted { step: step.into() },
            ExecutionEvent::StepCompleted(step) => Wire::StepCompleted { step: step.into() },
            ExecutionEvent::StepFailed {
                step,
                exit_code,
                interrupted,
            } => Wire::StepFailed {
                step: step.into(),
                exit_code: *exit_code,
                interrupted: *interrupted,
            },
            ExecutionEvent::StepSkipped(step) => Wire::StepSkipped { step: step.into() },
            ExecutionEvent::Output { offset, bytes } => Wire::Output {
                offset: *offset,
                bytes,
            },
            ExecutionEvent::Progress { ended, total } => Wire::Progress {
                ended: *ended,
                total: *total,
            },
            ExecutionEvent::Warning(message) => Wire::Warning { message },
            ExecutionEvent::Completed(finished) => Wire::Completed {
                result: finished.into(),
            },
            ExecutionEvent::Interrupted(finished) => Wire::Interrupted {
                result: finished.into(),
            },
        };
        wire.serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Check, Step, StepResult};

    fn command(id: &str, line: &str) -> Step {
        Step::Command {
            id: id.into(),
            name: id.to_uppercase(),
            line: line.into(),
        }
    }

    fn plan(steps: Vec<Step>) -> Plan {
        Plan {
            steps,
            ..Plan::default()
        }
    }

    fn quiet() -> Options {
        Options {
            history: HistoryPlace::Off,
            ..Options::default()
        }
    }

    fn start(plan: &Plan, options: Options) -> (Execution, Receiver<ExecutionEvent>) {
        Execution::start(plan, Path::new("/lib"), Path::new("/bin/hf"), options).unwrap()
    }

    fn is_end(event: &ExecutionEvent) -> bool {
        matches!(
            event,
            ExecutionEvent::Completed(_) | ExecutionEvent::Interrupted(_)
        )
    }

    /// Every event up to and including the run's end.
    fn until_end(events: &Receiver<ExecutionEvent>) -> Vec<ExecutionEvent> {
        let mut seen = Vec::new();
        loop {
            let event = events
                .recv_timeout(Duration::from_secs(30))
                .expect("the run should end");
            let end = is_end(&event);
            seen.push(event);
            if end {
                return seen;
            }
        }
    }

    /// The output the events carry, each piece starting where the last ended.
    fn assemble(events: &[ExecutionEvent]) -> Vec<u8> {
        let mut output = Vec::new();
        for event in events {
            if let ExecutionEvent::Output { offset, bytes } = event {
                assert_eq!(*offset as usize, output.len(), "no gap and no overlap");
                assert!(!bytes.is_empty() && bytes.len() <= OUTPUT_CHUNK);
                output.extend_from_slice(bytes);
            }
        }
        output
    }

    /// Wait for the transcript to show `text`.
    fn wait_for(execution: &Execution, text: &str) {
        let start = Instant::now();
        while !String::from_utf8_lossy(&execution.transcript()).contains(text) {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "waiting for {text:?} in {:?}",
                String::from_utf8_lossy(&execution.transcript())
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn position(events: &[ExecutionEvent], wanted: &ExecutionEvent) -> usize {
        events
            .iter()
            .position(|event| event == wanted)
            .unwrap_or_else(|| panic!("no {wanted:?} in {events:#?}"))
    }

    #[test]
    fn a_run_is_told_in_order_and_recorded() {
        let state = tempfile::tempdir().unwrap();
        let plan = plan(vec![
            command("one", "echo first"),
            command("two", "echo second"),
        ]);
        let (execution, events) = start(
            &plan,
            Options {
                history: HistoryPlace::In(state.path().to_path_buf()),
                ..Options::default()
            },
        );
        let seen = until_end(&events);
        let steps: Vec<StepInfo> = (0..2).map(|index| step_info(&plan, index)).collect();
        assert_eq!(
            seen[0],
            ExecutionEvent::RunStarted {
                entries: plan.entries(),
                steps: steps.clone(),
            }
        );
        let order = [
            ExecutionEvent::StepStarted(steps[0].clone()),
            ExecutionEvent::StepCompleted(steps[0].clone()),
            ExecutionEvent::Progress { ended: 1, total: 2 },
            ExecutionEvent::StepStarted(steps[1].clone()),
            ExecutionEvent::StepCompleted(steps[1].clone()),
            ExecutionEvent::Progress { ended: 2, total: 2 },
        ]
        .map(|event| position(&seen, &event));
        assert!(order.is_sorted(), "{seen:#?}");
        let ExecutionEvent::Completed(finished) = seen.last().unwrap() else {
            panic!("the run should complete: {seen:#?}");
        };
        assert_eq!(finished.outcome, Outcome::Success);
        assert!(execution.is_finished());
        assert_eq!(execution.finished().as_ref(), Some(finished));
        let output = assemble(&seen);
        assert_eq!(output, execution.transcript());
        let text = String::from_utf8_lossy(&output);
        assert!(text.contains("==> ONE") && text.contains("first"));
        assert!(text.contains("==> TWO") && text.contains("second"));
        // The marker has the outcome until the record is written.
        assert!(history::read_in(state.path(), 10).unwrap().is_empty());
        execution.record(None).unwrap();
        execution.record(None).unwrap();
        let records = history::read_in(state.path(), 10).unwrap();
        assert_eq!(records.len(), 1, "recorded once");
        assert!(records[0].success);
        assert_eq!(records[0].entries, ["one", "two"]);
    }

    #[test]
    fn the_first_events_come_before_output_the_program_already_wrote() {
        let (sender, receiver) = mpsc::sync_channel(8);
        let mut outlet = Outlet {
            sender,
            connected: true,
            sent: 0,
            queue: VecDeque::new(),
            transcript: Arc::new(Mutex::new(b"early".to_vec())),
        };
        let first = ExecutionEvent::Warning("first".into());
        outlet.next(first.clone());
        outlet.pump();
        assert_eq!(receiver.try_recv().unwrap(), first);
        assert_eq!(
            receiver.try_recv().unwrap(),
            ExecutionEvent::Output {
                offset: 0,
                bytes: b"early".to_vec()
            }
        );
    }

    #[test]
    fn a_slow_consumer_never_holds_the_run_up() {
        let plan = plan(vec![command(
            "lots",
            "head -c 3000000 /dev/zero | tr '\\0' x",
        )]);
        let (execution, events) = start(
            &plan,
            Options {
                capacity: 2,
                ..quiet()
            },
        );
        // Nothing is received until the run has ended.
        let start = Instant::now();
        while !execution.is_finished() {
            assert!(start.elapsed() < Duration::from_secs(30), "the run waited");
            std::thread::sleep(Duration::from_millis(20));
        }
        std::thread::sleep(Duration::from_millis(50));
        let seen = until_end(&events);
        let output = assemble(&seen);
        assert_eq!(output, execution.transcript());
        assert_eq!(
            output.iter().filter(|&&byte| byte == b'x').count(),
            3_000_000
        );
        assert!(matches!(seen.last(), Some(ExecutionEvent::Completed(_))));
    }

    #[test]
    fn large_output_arrives_whole_in_bounded_pieces() {
        let plan = plan(vec![command(
            "lots",
            "head -c 12000000 /dev/zero | tr '\\0' x; echo; echo the end",
        )]);
        let (execution, events) = start(&plan, quiet());
        let seen = until_end(&events);
        let output = assemble(&seen);
        assert_eq!(output, execution.transcript());
        assert_eq!(
            output.iter().filter(|&&byte| byte == b'x').count(),
            12_000_000
        );
        assert!(String::from_utf8_lossy(&output).ends_with("the end\r\n"));
        let pieces = seen
            .iter()
            .filter(|event| matches!(event, ExecutionEvent::Output { .. }))
            .count();
        assert!(pieces >= 12_000_000 / OUTPUT_CHUNK, "{pieces} pieces");
    }

    #[test]
    fn a_failed_step_is_told_and_what_needs_it_skipped() {
        let mut plan = plan(vec![
            command("broken", "echo trying; exit 3"),
            command("after", "echo never"),
            command("apart", "echo apart"),
        ]);
        plan.depends = vec![vec![], vec![0], vec![]];
        plan.keep_going = true;
        let (execution, events) = start(&plan, quiet());
        let seen = until_end(&events);
        let step = |index| step_info(&plan, index);
        let order = [
            ExecutionEvent::StepStarted(step(0)),
            ExecutionEvent::StepFailed {
                step: step(0),
                exit_code: 3,
                interrupted: false,
            },
            ExecutionEvent::StepSkipped(step(1)),
            ExecutionEvent::StepStarted(step(2)),
            ExecutionEvent::StepCompleted(step(2)),
            ExecutionEvent::Progress { ended: 3, total: 3 },
        ]
        .map(|event| position(&seen, &event));
        assert!(order.is_sorted(), "{seen:#?}");
        assert!(!seen.contains(&ExecutionEvent::StepStarted(step(1))));
        let ExecutionEvent::Completed(finished) = seen.last().unwrap() else {
            panic!("a failed run still completes: {seen:#?}");
        };
        assert_eq!(finished.outcome, Outcome::Failed { exit_code: 3 });
        let states: Vec<StepState> = finished.steps.iter().map(|s| s.state.clone()).collect();
        assert_eq!(
            states,
            [StepState::Failed(3), StepState::Skipped, StepState::Done]
        );
        assert!(!String::from_utf8_lossy(&execution.transcript()).contains("never"));
    }

    #[test]
    fn stopping_hangs_up_the_group_and_a_second_stop_kills_what_stayed() {
        let directory = tempfile::tempdir().unwrap();
        let pid_file = directory.path().join("pid");
        let plan = plan(vec![command(
            "stubborn",
            &format!(
                "(trap '' HUP; sh -c 'echo $$ > {}; exec sleep 300') & echo ready; wait",
                pid_file.display()
            ),
        )]);
        let (execution, events) = start(&plan, quiet());
        wait_for(&execution, "ready");
        while std::fs::read_to_string(&pid_file).map_or(true, |text| text.trim().is_empty()) {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!execution.stop_requested());
        execution.stop();
        let seen = until_end(&events);
        let ExecutionEvent::Interrupted(finished) = seen.last().unwrap() else {
            panic!("a stopped run is interrupted: {seen:#?}");
        };
        assert_eq!(finished.outcome, Outcome::Interrupted);
        assert!(finished.checks.is_empty());
        assert!(execution.stop_requested());
        assert!(
            seen.iter().any(|event| matches!(
                event,
                ExecutionEvent::Warning(message) if message.contains("1 process(es) of this run are still running")
            )),
            "{seen:#?}"
        );
        assert_eq!(execution.survivors(), 1);
        execution.stop();
        let start = Instant::now();
        while execution.survivors() > 0 {
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "the second stop kills"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn the_terminal_size_reaches_the_program_and_follows_a_resize() {
        let plan = plan(vec![command("size", "stty size; read -r line; stty size")]);
        let (execution, events) = start(
            &plan,
            Options {
                rows: 30,
                cols: 100,
                ..quiet()
            },
        );
        wait_for(&execution, "30 100");
        execution.resize(40, 120);
        execution.write(b"\r");
        let seen = until_end(&events);
        assert!(matches!(seen.last(), Some(ExecutionEvent::Completed(_))));
        assert!(String::from_utf8_lossy(&assemble(&seen)).contains("40 120"));
    }

    #[test]
    fn a_hidden_prompt_takes_input_without_showing_it() {
        // As sudo asks for a password: echo off, read, echo on.
        let plan = plan(vec![command(
            "secret",
            "stty -echo; printf 'Password: '; read -r secret; stty echo; echo; echo \"length ${#secret}\"",
        )]);
        let (execution, events) = start(&plan, quiet());
        wait_for(&execution, "Password: ");
        execution.write(b"hunter2\r");
        let seen = until_end(&events);
        let text = String::from_utf8_lossy(&assemble(&seen)).into_owned();
        assert!(text.contains("length 7"), "{text}");
        assert!(!text.contains("hunter2"), "{text}");
    }

    #[test]
    fn a_consumer_that_goes_away_leaves_the_run_and_its_record() {
        let state = tempfile::tempdir().unwrap();
        let plan = plan(vec![command("one", "echo done")]);
        let (execution, events) = start(
            &plan,
            Options {
                history: HistoryPlace::In(state.path().to_path_buf()),
                ..Options::default()
            },
        );
        drop(events);
        let start = Instant::now();
        while !execution.is_finished() {
            assert!(start.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(10));
        }
        // Dropping the handle writes the record.
        drop(execution);
        let records = history::read_in(state.path(), 10).unwrap();
        assert_eq!(records.len(), 1);
        assert!(records[0].success);
    }

    #[test]
    fn dropping_the_handle_hangs_up_a_run_in_progress() {
        let directory = tempfile::tempdir().unwrap();
        let pid_file = directory.path().join("pid");
        let plan = plan(vec![command(
            "long",
            &format!("sh -c 'echo $$ > {}; exec sleep 300'", pid_file.display()),
        )]);
        let (execution, events) = start(&plan, quiet());
        let start = Instant::now();
        while std::fs::read_to_string(&pid_file).map_or(true, |text| text.trim().is_empty()) {
            assert!(start.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(10));
        }
        let child = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .to_string();
        drop(execution);
        // The receiver closes once the run is gone.
        let start = Instant::now();
        while events.recv_timeout(Duration::from_secs(10)).is_ok() {
            assert!(start.elapsed() < Duration::from_secs(10));
        }
        let start = Instant::now();
        while std::fs::read_to_string(format!("/proc/{child}/stat")).is_ok_and(|stat| {
            !stat
                .rsplit_once(')')
                .unwrap()
                .1
                .trim_start()
                .starts_with('Z')
        }) {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "the run kept going"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn missing_requirements_stop_a_run_before_it_starts() {
        let mut plan = plan(vec![command("one", "echo hi")]);
        plan.requirements = vec!["helix-toolbox-no-such-tool".into()];
        let error = Execution::start(&plan, Path::new("/"), Path::new("/"), quiet())
            .err()
            .unwrap();
        assert!(matches!(error, EngineError::MissingExecutable(_)));
    }

    #[test]
    fn the_handle_can_be_shared_between_threads() {
        fn shared<T: Send + Sync>() {}
        shared::<Execution>();
        fn sent<T: Send>() {}
        sent::<Receiver<ExecutionEvent>>();
    }

    #[test]
    fn events_have_a_json_form() {
        let step = StepInfo {
            index: 0,
            id: "ripgrep".into(),
            name: "Ripgrep".into(),
        };
        let json = |event: &ExecutionEvent| serde_json::to_value(event).unwrap();
        assert_eq!(
            json(&ExecutionEvent::RunStarted {
                entries: vec![("ripgrep".into(), "Ripgrep".into())],
                steps: vec![step.clone()],
            }),
            serde_json::json!({
                "event": "run_started",
                "entries": [{"id": "ripgrep", "name": "Ripgrep"}],
                "steps": [{"index": 0, "id": "ripgrep", "name": "Ripgrep"}],
            })
        );
        assert_eq!(
            json(&ExecutionEvent::StepFailed {
                step: step.clone(),
                exit_code: 2,
                interrupted: false,
            }),
            serde_json::json!({
                "event": "step_failed",
                "step": {"index": 0, "id": "ripgrep", "name": "Ripgrep"},
                "exit_code": 2,
                "interrupted": false,
            })
        );
        assert_eq!(
            json(&ExecutionEvent::Output {
                offset: 5,
                bytes: b"hi\r\n".to_vec(),
            }),
            serde_json::json!({"event": "output", "offset": 5, "bytes": [104, 105, 13, 10]})
        );
        assert_eq!(
            json(&ExecutionEvent::Progress { ended: 1, total: 3 }),
            serde_json::json!({"event": "progress", "ended": 1, "total": 3})
        );
        assert_eq!(
            json(&ExecutionEvent::Warning("careful".into())),
            serde_json::json!({"event": "warning", "message": "careful"})
        );
        let finished = Finished {
            outcome: Outcome::Failed { exit_code: 1 },
            checks: vec![Check {
                name: "Ripgrep".into(),
                passed: false,
                detail: "rg is not installed".into(),
            }],
            steps: vec![StepResult {
                id: "ripgrep".into(),
                name: "Ripgrep".into(),
                state: StepState::Failed(1),
            }],
        };
        assert_eq!(
            json(&ExecutionEvent::Completed(finished.clone())),
            serde_json::json!({
                "event": "completed",
                "outcome": {"kind": "failed", "exit_code": 1},
                "description": finished.describe(),
                "succeeded": false,
                "verified": false,
                "steps": [{"index": 0, "id": "ripgrep", "name": "Ripgrep", "state": "failed", "exit_code": 1}],
                "checks": [{"name": "Ripgrep", "passed": false, "detail": "rg is not installed"}],
            })
        );
        let stopped = Finished {
            outcome: Outcome::Interrupted,
            checks: Vec::new(),
            steps: vec![StepResult {
                state: StepState::NotRun,
                ..finished.steps[0].clone()
            }],
        };
        let value = json(&ExecutionEvent::Interrupted(stopped));
        assert_eq!(value["event"], "interrupted");
        assert_eq!(value["outcome"], serde_json::json!({"kind": "interrupted"}));
        assert_eq!(value["steps"][0]["state"], "not_run");
    }
}
