# Execution events

A run of a plan, as the engine reports it to an interface:
`helix_toolbox_engine::execution`. The terminal interface's running window is
built on it, and a graphical interface (Tauri 2, Milestone 5 on the
[roadmap](ROADMAP.md)) will use the same API. There is no graphical
interface yet.

Every change to the machine stays in the engine. It checks the plan's
requirements, builds and runs the shell program in a pseudo-terminal,
follows each step, verifies the package steps, stops the run's process
group and writes the history. An interface only sends what the user typed,
the terminal's size and a request to stop, and draws the events it gets back.

This API is new and is not covered by [STABILITY.md](STABILITY.md) yet. It
may change before the graphical interface ships; the command line, `--json`
at schema 1, and the files listed there do not change because of it.

## Starting a run

```rust
use helix_toolbox_engine::execution::{Execution, ExecutionEvent, HistoryPlace, Options};

let (execution, events) = Execution::start(&plan, &lib_dir, &this, Options::default())?;
```

| Argument | Meaning |
| :--- | :--- |
| `plan` | a `Plan` from `plan()`, `plan_removal()`, `updates::plan()` and the rest, already confirmed by the user |
| `lib_dir` | the catalog's script folder (`Catalog::lib_dir()`), exported to the run as `HELIX_TOOLBOX_LIB` and its old name `HELIXFORGE_LIB` |
| `this` | the Helix Toolbox executable, which native steps run again (`helix-toolbox native …`) |
| `options` | below |

It returns an error, and starts nothing, when an executable the plan needs is
missing (`EngineError::MissingExecutable`) or the pseudo-terminal cannot be
opened (`EngineError::Spawn`).

| `Options` field | Default | Meaning |
| :--- | :--- | :--- |
| `rows`, `cols` | 24, 80 | the terminal's size when the run starts |
| `capacity` | 256 (`DEFAULT_CAPACITY`) | how many events the channel holds; at least 1 |
| `history` | `HistoryPlace::Default` | where the record goes: `Default` (the user's history, as the terminal interface keeps it), `In(state_dir)`, or `Off` |

## The handle

`Execution` is `Send` and `Sync`, and every method takes `&self`, so it can
live in shared state (in Tauri, managed state behind an `Arc`). The receiver
is a `std::sync::mpsc::Receiver`: one consumer, which can be moved to its
own thread.

| Method | What it does |
| :--- | :--- |
| `write(bytes)` | sends keyboard input to the program, as a terminal would: `\r` for Enter, `\x7f` for Backspace, escape sequences for the arrow keys. A password prompt (`sudo`) reads it like any other input, with the terminal's echo off. |
| `resize(rows, cols)` | gives the terminal a new size; the program gets `SIGWINCH` when it changes. Calling it with the same size does nothing. |
| `stop()` | the first call hangs up (`SIGHUP`) every process in the run's process group, as closing a terminal does; a later call, while any of them is still running, kills them (`SIGKILL`) |
| `stop_requested()` | whether `stop()` was called |
| `survivors()` | how many processes of the run are still alive, such as ones that ignored the hangup |
| `is_finished()`, `finished()` | whether the run has ended and been checked, and its `Finished` (outcome, step results, checks) |
| `transcript()` | everything the program has written so far, the same bytes the `Output` events carry |
| `record(log)` | writes the history record, with the path of a saved log (`history::save_log`) if there is one. Only the first call after the end writes; dropping the handle writes it without a log if nothing did. |

Until the record is written, the history's marker for the run keeps its
outcome, so a Helix Toolbox that stops before the record is written still
leaves an accurate history. Dropping the handle before the run ends hangs
the run up, as closing the window would.

## The events

| Event | When | Carries |
| :--- | :--- | :--- |
| `RunStarted` | always first | the entries `(id, name)` and the steps (`StepInfo`: `index`, `id`, `name`) in order |
| `StepStarted` | a step begins | its `StepInfo` |
| `StepCompleted` | a step exited 0 | its `StepInfo` |
| `StepFailed` | a step exited non-zero | its `StepInfo`, `exit_code`, and `interrupted` when a signal ended it (128 or more), which also ends the run |
| `StepSkipped` | a step was not run because a step it needs did not finish | its `StepInfo` |
| `Output` | the terminal had output | `offset`, where in the transcript `bytes` starts, and `bytes`, raw, escape sequences included; at most 64 KiB (`OUTPUT_CHUNK`) |
| `Progress` | after each step ends, however it ended | `ended` and `total` steps |
| `Warning` | something the user should know that does not stop the run: the start was not recorded, the steps cannot be followed one by one, processes of the run are still running after it ended | a message |
| `Completed` | the run ended by itself: success, failure, or could not start; the package steps that finished are verified | `Finished` |
| `Interrupted` | the run was stopped, by `stop()` or a signal; nothing is verified | `Finished` |

A step that never starts because the run stopped first (the default plan
stops at the first failure) has no event of its own; the end's `Finished`
lists it as not run.

### Delivery

- **Nothing is lost.** Output waits in the run's transcript until the
  channel has room. A consumer that falls behind gets it later, in fewer,
  larger pieces, each starting at the `offset` where the previous one ended.
  The other events wait in order behind the output that came before them.
- **The run never waits for the consumer.** Neither a full channel nor a
  consumer that stops reading slows the program down or changes how it
  ends: `is_finished()` and the history are right either way.
- **Bounded.** The channel holds at most `capacity` events, and an event at
  most `OUTPUT_CHUNK` bytes of output.
- **A consumer that goes away** (drops the receiver) leaves the run going;
  it still ends, is verified and is recorded.
- **Order.** `RunStarted` comes first. A step's `StepStarted` comes before
  its end, and its `Progress` after. `Completed` or `Interrupted` comes after
  the output the program wrote before it ended. Steps are reported through a
  file the program writes and output through the terminal, read side by
  side every few milliseconds, so a step's first or last bytes can fall on
  the other side of its event. Output can follow the end when processes the
  run left behind still write; the receiver closes once the terminal is
  closed and everything is sent.

## A consumer

The terminal interface takes what arrived on each pass of its event loop,
without waiting:

```rust
while let Ok(event) = events.try_recv() {
    match event {
        ExecutionEvent::Output { offset, bytes } => {
            // Skip anything already held, in case of a resynchronized copy.
            let held = output.len() as u64;
            if offset <= held {
                output.extend_from_slice(&bytes[(held - offset) as usize..]);
            }
        }
        ExecutionEvent::Completed(finished) | ExecutionEvent::Interrupted(finished) => {
            done = Some(finished);
        }
        _ => {}
    }
}
```

It draws the output with a terminal emulator (`vt100`), calls `resize` with
the size of its terminal panel on every draw, forwards keys with `write`,
calls `stop` on Ctrl-C, and calls `record` when the window closes, with a
log if the user saved one.

A graphical interface would move the receiver to a thread that forwards
each event to its window, for example through a Tauri channel, and draw
`Output` with a terminal emulator such as xterm.js. `Output` bytes may split
a UTF-8 character or an escape sequence; the emulator joins them.

## JSON form

`ExecutionEvent` implements `serde::Serialize`: an object whose `event` names
the kind in snake case.

```json
{"event": "run_started", "entries": [{"id": "ripgrep", "name": "Ripgrep"}],
 "steps": [{"index": 0, "id": "ripgrep", "name": "Ripgrep"}]}
{"event": "step_started", "step": {"index": 0, "id": "ripgrep", "name": "Ripgrep"}}
{"event": "output", "offset": 0, "bytes": [13, 10, 61, 61, 62, 32]}
{"event": "step_completed", "step": {"index": 0, "id": "ripgrep", "name": "Ripgrep"}}
{"event": "step_failed", "step": {"index": 0, "id": "ripgrep", "name": "Ripgrep"},
 "exit_code": 2, "interrupted": false}
{"event": "step_skipped", "step": {"index": 1, "id": "meld", "name": "Meld"}}
{"event": "progress", "ended": 1, "total": 1}
{"event": "warning", "message": "1 process(es) of this run are still running"}
{"event": "completed", "outcome": {"kind": "success"},
 "description": "exit 0, verified", "succeeded": true, "verified": true,
 "steps": [{"index": 0, "id": "ripgrep", "name": "Ripgrep", "state": "done"}],
 "checks": [{"name": "Ripgrep", "passed": true, "detail": "pacman lists ripgrep"}]}
{"event": "interrupted", "outcome": {"kind": "interrupted"},
 "description": "interrupted", "succeeded": false, "verified": false,
 "steps": [{"index": 0, "id": "ripgrep", "name": "Ripgrep", "state": "not_run"}],
 "checks": []}
```

`outcome.kind` is `success`, `failed` (with `exit_code`), `interrupted` or
`could_not_start` (with `reason`). A step's `state` is `done`, `failed` (with
`exit_code`), `interrupted`, `skipped` or `not_run`. `description` is the
result as the history records it. `bytes` is an array of numbers in JSON; an
interface that can pass raw bytes should do so instead.

## Threads

There is no async runtime: a run has three threads of its own. One waits
for the shell to exit, one reads the terminal into the transcript, and the
monitor reads the program's step file, sends the events and verifies the
run once it ends. The command line's `helix-toolbox run` does not use the
pseudo-terminal or the events: it runs the same program in the caller's
terminal (`InheritedRunner`) and records it the same way.
