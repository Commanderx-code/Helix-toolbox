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
| Distributions | `.github/workflows/distributions.yml` | Arch, Debian, Ubuntu and Fedora containers with real package installs and removals, each verified, and the real package manager failing over to a stand-in Flathub |

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
- the banner drawn when the terminal answers the graphics query as Kitty's
  graphics do, the text header when it does not, and the first key read in
  both cases (`tui/banner.rs` has unit tests for the answers of Kitty, Sixel,
  Konsole, WezTerm and plain terminals)
- the command line's `list`, `run --dry-run`, refusal to confirm without a
  terminal, `run --yes`, failure status and `history`
- a run whose history cannot be written: the command line and the menu both
  say so
- the menu or the command line killed during a run: the next history lists
  it once as not finished; killed after a run ended: its real result
- the command line's confirmation asked on the terminal when standard input
  is a pipe, and a wrong word cancelling
- `helixforge status` after real installs of htop and ripgrep: Installed,
  from the family's package manager, with a version (Distributions)
- package entries from `tests/fixtures/packages`, against stand-in `sudo`,
  package managers and `flatpak` on the PATH: the distribution's package
  failing over to Flathub, `Verified:` lines and `exit 0, verified` in the
  history, removal verified, a source that installs nothing not being
  trusted, and the running window's `FINISHED · VERIFIED`

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

`tests/fixtures/packages` holds two package entries whose distribution
package does not exist: **fallback-sample**, which the stand-in `flatpak`
installs, and **missing-sample**, which it only pretends to install. The
engine's unit tests also run generated package programs against harmless
commands: falling back, skipping a source whose tool is missing, stopping on
an interrupted source, and removals that leave the package behind.

## Release checks

Every push to `main` runs the Checks, Distributions and Sync sources
workflows; the last publishes the next release, tagged `vMAJOR.MINOR.PATCH`,
with `helixforge-linux-x86_64` and `SHA256SUMS`, and checks that
`helixforge --version` reports that version. `scripts/release-version.py`
picks it: the commit's own version when it already has one (a rebuild), else
one patch above the newest tag, or the workspace version in `Cargo.toml` when
that is higher. Releases before v0.1.0 were named `helixforge-<commit>`. To
check a published build by hand, without running any entry:

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
- Real Flatpak installs, Nix entries and `sudo` password prompts are not
  run by any automated test; Flathub fallback runs against a stand-in
  `flatpak`.
- The release is built for Linux x86_64 with glibc 2.34 or later only.
- The banner picture is checked against a test terminal that answers like
  Ghostty and one that answers like a Sixel terminal, with the image the
  program sends painted into the screenshot; how real terminals draw it is
  checked by eye.
