// Sample answers for working on the interface in a browser, where there is
// no Tauri side. Nothing here reads or changes the machine.

import type { AreaView, ChangesView, EntryView, ItemView, Prepared, RecordView, RunEvent, StatusView, WindowTheme } from './types';

const entry = (id: string, name: string, description: string, flags = 'I', extra: Partial<EntryView> = {}): ItemView => ({
  type: 'entry',
  id,
  name,
  description,
  flags,
  kind: 'package',
  queueable: true,
  interactive: false,
  removable: (extra.kind ?? 'package') === 'package',
  yours: false,
  runs: [],
  ...extra,
});

const group = (id: string, name: string, description: string, items: ItemView[]): ItemView => ({
  type: 'group',
  id,
  name,
  description,
  items,
});

const AREAS: AreaView[] = [
  {
    id: 'applications',
    name: 'Applications',
    items: [
      group('developer', 'Developer Tools', 'Editors, version control and command-line helpers.', [
        entry('git', 'Git', 'Distributed version control.'),
        entry('neovim', 'Neovim', 'Hyperextensible Vim-based text editor.'),
        entry('vscode', 'VS Code', 'Code editing, redefined.', 'I'),
        entry('zed', 'Zed', 'A high-performance, multiplayer code editor.'),
        entry('ripgrep', 'Ripgrep', 'Recursively searches directories for a regex pattern.'),
        entry('fzf', 'fzf', 'A command-line fuzzy finder.'),
        entry('jetbrains', 'JetBrains Toolbox', 'Installs and updates JetBrains IDEs.', 'I', {
          kind: 'script',
          interactive: true,
          queueable: false,
        }),
      ]),
      group('browsers', 'Web Browsers', 'Browsers from the distribution or Flathub.', [
        entry('firefox', 'Firefox', 'Fast, private and free web browser.'),
        entry('brave', 'Brave', 'Privacy-focused browser that blocks ads and trackers.', 'FI'),
        entry('chromium', 'Chromium', 'The open-source project behind Chrome.'),
      ]),
      group('multimedia', 'Multimedia', 'Players, editors and streaming tools.', [
        entry('mpv', 'mpv', 'A free, open-source and cross-platform media player.'),
        entry('obs', 'OBS Studio', 'Video recording and live streaming.', 'FI'),
      ]),
    ],
  },
  {
    id: 'configurations',
    name: 'Configurations',
    items: [
      entry('dotfiles', 'Dotfiles', 'Apply the dotfiles with Home Manager.', 'FM', { kind: 'nix' }),
      entry('fish', 'Fish shell setup', 'Myfish: the fish configuration, prompt and functions.', 'FM', { kind: 'script' }),
      entry('kernel-params', 'Kernel parameters', 'Tune boot parameters for this machine.', 'K', {
        kind: 'script',
        interactive: true,
        queueable: false,
      }),
    ],
  },
  {
    id: 'maintenance',
    name: 'Maintenance',
    items: [
      entry('mirrors', 'Refresh mirrors', 'Rank the fastest package mirrors.', 'MP', { kind: 'script' }),
      entry('cache', 'Clean package cache', 'Keep the last two versions of each package.', 'MP', { kind: 'command' }),
      entry('report', 'System report', 'Write a report of this machine for a bug report.', '', { kind: 'native' }),
    ],
  },
  {
    id: 'profiles',
    name: 'Profiles',
    items: [
      group('your-profiles', 'Your profiles', 'Saved in /home/me/.config/helix-toolbox/profiles', [
        entry('profile-my-workstation', 'My workstation', 'Your profile: git, neovim, ripgrep, firefox.', '', {
          kind: 'profile',
          yours: true,
          runs: ['git', 'neovim', 'ripgrep', 'firefox'],
        }),
      ]),
      entry('developer-basics', 'Developer basics', 'Git, Neovim, Ripgrep and fzf.', '', { kind: 'profile', runs: ['git', 'neovim', 'ripgrep', 'fzf'] }),
    ],
  },
];

const STATUSES: Record<string, StatusView> = {
  git: { kind: 'installed', label: 'Installed', detail: 'pacman 2.56.0-1' },
  neovim: { kind: 'installed', label: 'Installed', detail: 'pacman 0.12.6-1' },
  ripgrep: { kind: 'update', label: 'Update', detail: 'pacman 15.2.0-1 → 15.3.0-1' },
  fzf: { kind: 'managed', label: 'Managed', detail: 'home-manager 0.74.4' },
  vscode: { kind: 'absent', label: 'Not installed', detail: '' },
  zed: { kind: 'installed', label: 'Installed', detail: 'pacman 1.24.3-1' },
  firefox: { kind: 'installed', label: 'Installed', detail: 'pacman 151.0-1' },
  brave: { kind: 'absent', label: 'Not installed', detail: '' },
  chromium: { kind: 'absent', label: 'Not installed', detail: 'would replace ungoogled-chromium' },
  mpv: { kind: 'installed', label: 'Installed', detail: 'pacman 0.41.0-2' },
  obs: { kind: 'absent', label: 'Not installed', detail: '' },
};

let pinned = ['neovim', 'ripgrep'];

const RUNS: RecordView[] = [
  { date: '2026-10-18 21:04', unix_timestamp: 0, names: ['Ripgrep', 'fzf'], success: true, result: 'exit 0, verified', log: null },
  { date: '2026-10-17 09:41', unix_timestamp: 0, names: ['OBS Studio'], success: false, result: 'exit 1', log: null },
  { date: '2026-10-15 18:12', unix_timestamp: 0, names: ['Refresh mirrors'], success: true, result: 'exit 0', log: null },
];

function find(id: string, items: ItemView[] = AREAS.flatMap((area) => area.items)): EntryView | undefined {
  for (const item of items) {
    if (item.type === 'entry' && item.id === id) return item;
    if (item.type === 'group') {
      const found = find(id, item.items);
      if (found) return found;
    }
  }
}

let planned: EntryView[] = [];
let theme: WindowTheme = 'system';

export async function call(command: string, args: Record<string, unknown> = {}): Promise<unknown> {
  await new Promise((resolve) => setTimeout(resolve, 60));
  switch (command) {
    case 'about':
      return {
        version: '1.11.0',
        command_line: true,
        first_run: false,
        theme,
        overview: {
          rows: [
            ['Distribution', 'Sample Linux'],
            ['Package manager', 'pacman'],
            ['Desktop', 'KDE'],
            ['Shell', 'fish'],
            ['Kernel', '7.2.10-1-sample'],
            ['CPU', '11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz'],
            ['Memory', '62.5 GiB'],
            ['Root disk', '932G, 559G free'],
          ],
        },
      };
    case 'areas':
      return AREAS;
    case 'statuses':
      await new Promise((resolve) => setTimeout(resolve, 600));
      return STATUSES;
    case 'favorites':
      return pinned;
    case 'pin': {
      const id = args.id as string;
      pinned = pinned.filter((other) => other !== id);
      if (args.pinned) pinned.push(id);
      return pinned;
    }
    case 'prepare': {
      planned = (args.ids as string[]).map((id) => find(id)).filter((e): e is EntryView => !!e);
      const result: Prepared = {
        kind: 'ready',
        risk: 'medium',
        privileged: true,
        header: [
          'Risk: medium · asks for your password',
          'Warning: about 412 MiB more on /, with 559 GiB free',
          ...(planned.length > 1 ? ['After a failed step: stop (Settings changes it)'] : []),
        ],
        lines: planned.map((entry) => ({
          name: entry.name,
          detail:
            entry.kind === 'package'
              ? `sudo pacman -S --needed ${entry.id}`
              : `/bin/sh -e ${entry.id}.sh  (in scripts/installers)`,
        })),
      };
      return result;
    }
    case 'changes':
    case 'download_sources': {
      const downloaded = command === 'download_sources';
      const result: ChangesView = {
        download: !downloaded,
        steps: planned.map((entry) => ({
          name: entry.name,
          changes:
            entry.kind === 'package'
              ? [
                  { certainty: 'known', text: `installs ${entry.id} 15.2.0-1 from extra (pacman's own answer)`, diff: null },
                  { certainty: 'expected', text: 'takes effect at once', diff: null },
                ]
              : [
                  {
                    certainty: downloaded ? 'known' : 'unknown',
                    text: downloaded
                      ? 'replaces ~/.config/fish/config.fish'
                      : 'writes files from the pinned dotfiles source, which is not downloaded yet',
                    diff: downloaded
                      ? {
                          path: '~/.config/fish/config.fish',
                          changed: true,
                          rows: [
                            { before: 'set -g fish_greeting', after: 'set -g fish_greeting', tag: 'same' },
                            { before: 'set -gx EDITOR nano', after: 'set -gx EDITOR nvim', tag: 'changed' },
                            { before: null, after: 'starship init fish | source', tag: 'added' },
                            { before: 'alias ls="ls --color"', after: 'alias ls="ls --color"', tag: 'same' },
                            null,
                            { before: 'fish_add_path ~/bin', after: null, tag: 'removed' },
                          ],
                        }
                      : null,
                  },
                  { certainty: 'expected', text: 'may restart a unit it chooses (seen in its script)', diff: null },
                ],
        })),
      };
      return result;
    }
    case 'runs':
      return RUNS;
    case 'save_profile':
      return `${args.name} (${(args.ids as string[]).length} entries) is saved.`;
    case 'import_profile':
      return 'Workstation (4 entries) is imported as /home/me/.config/helix-toolbox/profiles/workstation.toml. It is under Profiles, Your profiles.';
    case 'export_profile':
      return `${find(args.id as string)?.name} is exported to /home/me/${(args.id as string).replace('profile-', '')}.toml.`;
    case 'copy_text':
      await navigator.clipboard?.writeText(args.text as string).catch(() => undefined);
      return undefined;
    case 'bug_report':
      await new Promise((resolve) => setTimeout(resolve, 900));
      return [
        '### Helix Toolbox report',
        '',
        '- Helix Toolbox: 1.13.0',
        '- Installed from: package',
        '- Distribution: Garuda Linux (garuda, like arch)',
        '- Package manager: pacman (Arch family)',
        '',
        '### The window',
        '',
        '- Helix Toolbox window: 1.13.0',
        '- Installed from: package',
        '- WebKitGTK: 2.50.4',
        '- Desktop: KDE, wayland session',
        '- Theme: system',
      ].join('\n');
    case 'delete_profile':
      return `${find(args.id as string)?.name} is deleted.`;
    case 'doctor':
      return [
        { id: 'network', title: 'Network', health: 'good', health_label: 'OK', summary: 'Connected through wlp0s20f3', evidence: [], advice: null, repair: null, entry: null },
        { id: 'logrotate', title: 'Service logrotate.service', health: 'problem', health_label: 'Problem', summary: 'The system service logrotate.service failed', evidence: ['systemctl is-failed logrotate.service: failed'], advice: 'Look at journalctl -u logrotate.service', repair: { label: 'Restart logrotate.service', commands: ['sudo systemctl restart logrotate.service'] }, entry: null },
        { id: 'disk', title: 'Disk space', health: 'good', health_label: 'OK', summary: '558.5 GiB free of 931.2 GiB on /', evidence: [], advice: null, repair: null, entry: null },
      ];
    case 'updates':
      return {
        online: args.online,
        channels: [
          { id: 'pacman', name: 'Arch packages', count: 58, summary: '58 updates', pending: ['linux-zen', 'mesa', 'firefox', 'kwin', 'plasma-workspace', 'systemd', 'glibc', 'python', 'qt6-base', 'pipewire', 'nvidia-utils', 'libinput', 'curl', 'git'].map((name, index) => ({ name, from: `1.${index}.0`, to: `1.${index}.1` })), note: '', command_line: 'sudo pacman -Syu', caveat: null },
          { id: 'flatpak', name: 'Flatpak (system)', count: 2, summary: '2 updates', pending: [], note: '', command_line: 'flatpak update -y', caveat: null },
          { id: 'helixforge', name: 'Helix Toolbox', count: 0, summary: 'up to date', pending: [], note: '1.8.0; it fetches the latest release each time it starts', command_line: '', caveat: null },
        ],
      };
    case 'list_backups':
      return { snapshots: { tool: 'Snapper', restore: 'snapper list shows them; snapper undochange N puts one back.' }, backups: [{ index: 0, original: '/home/me/.config/fish/config.fish', backup: '/home/me/.config/fish/config.fish.helix-toolbox-backup-20261018', taken: '2026-10-18 21:04', system: false, directory: false }] };
    case 'drift_targets':
      return [{ key: 'baseline:laptop', kind: 'baseline', name: 'Laptop', detail: 'saved 2026-10-01 on Sample Linux: 42 entries, 12 configuration paths' }];
    case 'drift_check':
      return { report: { against: 'Laptop', matched: 40, differences: [{ kind: 'MissingEntry', subject: 'ripgrep', was: 'pacman 15.2.0', now: null, entry: 'ripgrep', note: '' }], unknown: [] }, restore: ['ripgrep'], unavailable: [] };
    case 'save_baseline':
      return `${args.name} is saved.`;
    case 'reliability':
      return { summary: 'The record covers 6 distributions and 9 virtual machines.', problems: [], rows: [{ id: 'git', name: 'Git', level: 'Tested in containers', explain: 'Installed and verified in distribution containers.', assessment: { level: 'ContainerTested', systems: ['Arch', 'Debian'], verified: '2026-10-19', evidence: ['archlinux: verified'], here: null, critical: false } }] };
    case 'trust':
      return `${args.id}\n\nInstaller   built in\nValidation  Tested in containers`;
    case 'read_hardware':
      return { firmware: { available: true, devices: [], updates: [], note: null }, disks: [{ name: 'nvme0n1', model: 'Sample SSD', bytes: 1000204886016, kind: 'NVMe', firmware: '1.0', celsius: 38 }], sensors: [{ chip: 'coretemp', label: 'Package', celsius: 52, high: 100, critical: 100 }], batteries: [{ name: 'BAT0', status: 'Charging', capacity: 80, health: 91, cycles: 120, model: null, technology: 'Li-ion' }], pci: [], usb: [] };
    case 'read_desktop':
      return { desktop: { kind: 'kde', name: 'KDE Plasma 6.8.0', session: 'Wayland', display_manager: { name: 'SDDM', files: [], theme: 'breeze' }, appearance: [{ key: 'theme', label: 'Global theme', value: 'Breeze Dark' }], owners: [], blocked: null }, looks: [], suggestions: [] };
    case 'save_look':
      return `${args.name} is saved.`;
    case 'setup_start':
      return {
        machine: [
          ['System', 'Garuda Linux'],
          ['Packages', 'pacman, with paru, flatpak'],
          ['Desktop', 'KDE Plasma 6.8.0 on Wayland'],
          ['Graphics', 'NVIDIA'],
          ['Machine', 'a laptop'],
        ],
        workstations: [
          { id: 'developer', name: 'Developer', description: 'Git, a fast search and fuzzy finder, Neovim and VS Code, Docker, a terminal and a diff tool' },
          { id: 'gaming', name: 'Gaming', description: 'Steam, Lutris and Heroic, WINE and the Proton tools, Discord, and the drivers this machine needs' },
          { id: 'custom', name: 'Custom', description: 'Every workstation\'s entries, to pick from' },
        ],
      };
    case 'setup_choose':
      return [
        { id: 'git', name: 'Git', available: true, note: 'installed (pacman 2.47.0)', ticked: false },
        { id: 'neovim', name: 'Neovim', available: true, note: null, ticked: args.id !== 'custom' },
        { id: 'vscode', name: 'VS Code', available: true, note: null, ticked: args.id !== 'custom' },
        { id: 'jetbrains', name: 'JetBrains Toolbox', available: true, note: 'runs on its own, outside the Install Plan', ticked: args.id !== 'custom' },
        { id: 'docker', name: 'Docker', available: false, note: 'not offered on this machine', ticked: false },
      ];
    case 'setup_done':
      return undefined;
    case 'bundle_contents':
      return {
        entries: (args.ids as string[]).map((id) => {
          const known = find(id);
          const offline = known?.kind === 'package';
          return { name: known?.name ?? id, offline, network: offline ? null : 'a script, which downloads as it runs', flatpak: null };
        }),
        files: args.complete ? 41 : 7,
        dnf: [],
        zypper: [],
        flatpaks: [],
      };
    case 'prepare_bundle_create':
      return {
        kind: 'ready',
        risk: 'low',
        privileged: false,
        header: ['Risk: low', 'Note: It downloads the packages into the folder and installs nothing on this machine.'],
        lines: [{ name: `Make a bundle in ${args.directory}`, detail: `/usr/bin/helix-toolbox bundle create ${args.directory} ${(args.ids as string[]).join(' ')}` }],
      } satisfies Prepared;
    case 'bundle_check':
      if (!String(args.directory).includes('bundle')) throw new Error(`${args.directory} is not a Helix Toolbox bundle: it has no bundle.toml`);
      return {
        distribution: 'Sample Linux',
        architecture: 'x86_64',
        version: '1.12.0',
        release: true,
        created: '2026-10-20 18:30',
        complete: false,
        files: 9,
        bytes: 48_200_000,
        flatpaks: [],
        entries: [
          { name: 'Ripgrep', offline: true, network: null, flatpak: null },
          { name: 'Neovim', offline: true, network: null, flatpak: null },
        ],
      };
    case 'prepare_bundle_install':
      return {
        kind: 'ready',
        risk: 'medium',
        privileged: true,
        header: ['Risk: medium · asks for your password', 'Note: The packages are checked against the bundle\'s checksums and signatures first.'],
        lines: [{ name: 'Install the bundle\'s packages', detail: 'sudo pacman -U --needed packages/*.pkg.tar.zst' }],
      } satisfies Prepared;
    case 'prepare_remove': {
      planned = (args.ids as string[]).map((id) => find(id)).filter((e): e is EntryView => !!e);
      const result: Prepared = {
        kind: 'ready',
        risk: 'medium',
        privileged: true,
        header: ['Risk: medium · asks for your password'],
        lines: planned.map((entry) => ({ name: `Remove ${entry.name}`, detail: `sudo pacman -R ${entry.id}` })),
      };
      return result;
    }
    case 'save_theme':
      theme = args.theme as WindowTheme;
      return call('read_settings');
    case 'read_settings':
    case 'save_settings':
      return { keep_going: args.keepGoing ?? false, snapshots: args.snapshots ?? true, snapshot_tool: 'Snapper', path: '/home/me/.config/helix-toolbox/settings.toml', theme };
    default:
      return undefined;
  }
}

/** A pretend run of what was last prepared. */
export async function run(onEvent: (event: RunEvent) => void) {
  const steps = planned.map((entry, index) => ({ index, id: entry.id, name: entry.name }));
  let offset = 0;
  const output = (text: string) => {
    const terminal = text.replace(/\n/g, '\r\n');
    onEvent({ event: 'output', offset, data: btoa(terminal) });
    offset += terminal.length;
  };
  const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
  onEvent({ event: 'run_started', entries: steps.map(({ id, name }) => ({ id, name })), steps });
  for (const step of steps) {
    onEvent({ event: 'step_started', step });
    output(`\x1b[1;36m==> ${step.name}\x1b[0m\n`);
    output(`resolving dependencies...\nlooking for conflicting packages...\n\n`);
    output(`Packages (1) ${step.id}-1.0-1\n\nTotal Installed Size:  4.20 MiB\n\n`);
    await wait(500);
    output(`(1/1) installing ${step.id}                       [####################] 100%\n`);
    await wait(300);
    onEvent({ event: 'step_completed', step });
    onEvent({ event: 'progress', ended: step.index + 1, total: steps.length });
  }
  onEvent({
    event: 'completed',
    outcome: { kind: 'success' },
    description: 'exit 0, verified',
    succeeded: true,
    verified: true,
    steps: steps.map((step) => ({ ...step, state: 'done' as const })),
    checks: steps.map((step) => ({ name: step.name, passed: true, detail: `pacman lists ${step.id}` })),
  });
}
