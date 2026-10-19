// What the commands in src-tauri/src/main.rs return; views.rs builds them.

export type Overview = { rows: [string, string][] };

export type About = {
  version: string;
  overview: Overview;
  command_line: boolean;
};

export type EntryView = {
  id: string;
  name: string;
  description: string;
  flags: string;
  kind: 'package' | 'script' | 'command' | 'nix' | 'native' | 'profile';
  queueable: boolean;
  interactive: boolean;
  runs: string[];
};

export type ItemView =
  | ({ type: 'entry' } & EntryView)
  | { type: 'group'; id: string; name: string; description: string; items: ItemView[] };

export type AreaView = { id: string; name: string; items: ItemView[] };

export type StatusView = {
  kind: 'installed' | 'update' | 'managed' | 'absent';
  label: string;
  detail: string;
};

export type Line = { name: string; detail: string };

export type Prepared =
  | { kind: 'nothing' | 'blocked' | 'error'; lines: string[] }
  | { kind: 'ready'; header: string[]; lines: Line[]; risk: string; privileged: boolean };

export type StepInfo = { index: number; id: string; name: string };

export type StepState =
  | { state: 'done' | 'interrupted' | 'skipped' | 'not_run' }
  | { state: 'failed'; exit_code: number };

export type Finished = {
  outcome:
    | { kind: 'success' }
    | { kind: 'failed'; exit_code: number }
    | { kind: 'interrupted' }
    | { kind: 'could_not_start'; reason: string };
  description: string;
  succeeded: boolean;
  verified: boolean;
  steps: (StepInfo & StepState)[];
  checks: { name: string; passed: boolean; detail: string }[];
};

/** An event of a run: ExecutionEvent's JSON form, output as base64. */
export type RunEvent =
  | { event: 'run_started'; entries: { id: string; name: string }[]; steps: StepInfo[] }
  | { event: 'step_started' | 'step_completed' | 'step_skipped'; step: StepInfo }
  | { event: 'step_failed'; step: StepInfo; exit_code: number; interrupted: boolean }
  | { event: 'output'; offset: number; data: string }
  | { event: 'progress'; ended: number; total: number }
  | { event: 'warning'; message: string }
  | ({ event: 'completed' | 'interrupted' } & Finished);

export type RecordView = {
  date: string;
  unix_timestamp: number;
  names: string[];
  success: boolean;
  result: string;
  log: string | null;
};
