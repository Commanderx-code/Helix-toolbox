# Testing

Tests never change the machine they run on. Fixtures live in
`tests/fixtures/`; anything a test runs writes only to temporary
directories, with `HOME`, `XDG_STATE_HOME` and `TMPDIR` pointed there. Real
package installs happen only in the Distributions workflow's containers.
`scripts/verify.sh` runs every local layer below; check its exit status.

## Layers

| Layer | Where | What it covers |
| :--- | :--- | :--- |
| Unit tests | each crate's `src/` | catalog validation, plans, the shell program, history, the pseudo-terminal session, the unpacked-copy lock and sweep |
| Screen tests | `apps/helixforge/src/tui/` | the interface drawn into a test backend: keys, screens, popups, the small-terminal warning |
| Engine end to end | `apps/helixforge/tests/end_to_end.rs` | fixture plans run through the mock runner and a real shell |
| Program in a terminal | `apps/helixforge/tests/terminal.rs` | the built `helixforge` driven through a pseudo-terminal, below |
| Installer tests | `tests/installers/` | the Commander installers against stand-in commands and temporary homes |
| Shellcheck | `scripts/verify.sh` | every shared helper and installer, with errors gated for the general catalog |
| Workflow lint | `scripts/verify.sh` | actionlint over every workflow, including the shell in each step |
| Source render | `scripts/verify.sh` | rendering the dotfiles and Myfish exports again changes no catalog file |
| Distributions | `.github/workflows/distributions.yml` | Arch, Debian, Ubuntu and Fedora containers with real package installs and removals |

## The program in a terminal

`terminal.rs` starts the real binary in a pseudo-terminal with the hidden
`--catalog-dir tests/fixtures` option, reads the screen with a terminal
emulator and types keys. It checks:

- sidebar navigation, the help popup, search and clearing it
- adding to the install plan, declining the confirmation (nothing runs) and
  accepting it
- typing an answer into an entry that asks a question
- `Ctrl-C` stopping an entry and its child process, recorded as interrupted
- a child that ignores the hangup: the window counts it, and a second
  `Ctrl-C` kills it
- a failing entry's exit status, the History screen and `history.jsonl`
- resizing while an entry runs, the too-small warning and recovery, and a
  key pressed together with a resize being read at once
- `Ctrl-C` in the confirmation leaving without running
- a killed HelixForge's unpacked copy being removed by the next start, and a
  clean exit removing its own
- refusing to open the menu without a terminal, and opening it when standard
  input is a pipe, as with `curl … | sh`
- the command line's `list`, `run --dry-run`, refusal to confirm without a
  terminal, `run --yes`, failure status and `history`
- a run whose history cannot be written: the command line and the menu both
  say so
- the menu or the command line killed during a run: the next history lists
  it once as not finished; killed after a run ended: its real result
- the command line's confirmation asked on the terminal when standard input
  is a pipe, and a wrong word cancelling

The fixture catalog's entries:

| Entry | Does |
| :--- | :--- |
| marker | writes a marker file where `HELIXFORGE_TEST_OUT` points |
| failing | exits with status 9 |
| report | the native system report |
| bundle | a profile of marker and report |
| greeter | asks for a name and greets it |
| sleeper | records a child's process id and sleeps |
| stubborn | the same, ignoring the hangup |

## Release checks

Every push to `main` runs the Checks, Distributions and Sync sources
workflows; the last publishes `helixforge-<12 characters of the commit>` with
`helixforge-linux-x86_64` and `SHA256SUMS`. To check a published build by
hand, without running any entry:

```sh
tag=$(gh release view --repo Commanderx-code/Helix-Forge --json tagName --jq .tagName)
dir=$(mktemp -d) && cd "$dir"
gh release download "$tag" --repo Commanderx-code/Helix-Forge
sha256sum -c SHA256SUMS
chmod +x helixforge-linux-x86_64
./helixforge-linux-x86_64 --version
./helixforge-linux-x86_64 list | head
./helixforge-linux-x86_64 run system-report --dry-run
```

## Not covered

- Boot, login-screen and desktop entries (GRUB, Limine, Plymouth, SDDM)
  need a real machine or a virtual machine; containers cannot boot them.
- Flatpak installs, Nix entries and `sudo` password prompts are not run by
  any automated test.
- The release is built for Linux x86_64 with glibc 2.34 or later only.
