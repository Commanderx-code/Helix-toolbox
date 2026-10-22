// Sample answers for working on the interface in a browser, where there is
// no Tauri side. Nothing here reads or changes the machine.

import type { AreaView, ChangesView, EntryView, ItemView, Migration, MigrationExportView, PerformanceView, Prepared, RecordView, RecoveryView, RunEvent, TranslationView, ServicesView, StatusView, StorageView, WindowTheme } from './types';

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
let notifications = false;
let health = false;
let watched: string | null = null;

export async function call(command: string, args: Record<string, unknown> = {}): Promise<unknown> {
  await new Promise((resolve) => setTimeout(resolve, 60));
  switch (command) {
    case 'about':
      return {
        version: '1.11.0',
        command_line: true,
        first_run: false,
        theme,
        opens: null,
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
    case 'check_entry':
      return `Everything that can be checked here is as it should be.\n\nVerified: ${find(args.id as string)?.name}: pacman lists it`;
    case 'hardware_report':
      return {
        text: [
          'A report that Git worked on this machine, on 2026-10-21.',
          '',
          'System: Garuda Linux, KDE Plasma 6.5.0 (wayland), Lenovo ThinkPad T15p Gen 2i, NVIDIA GeForce GTX 1650 Mobile / Max-Q (nvidia)',
          'Kernel: 7.2.11-1-garuda, x86_64',
          'Helix Toolbox: 1.15.0',
          '',
          'The record it asks for, in catalog/validation.toml:',
          '',
          '[[hardware]]',
          'entry = "git"',
          'system = "Garuda Linux, KDE Plasma 6.5.0 (wayland), Lenovo ThinkPad T15p Gen 2i, NVIDIA GeForce GTX 1650 Mobile / Max-Q (nvidia)"',
          'version = "1.15.0"',
          'date = "2026-10-21"',
        ].join('\n'),
        url: 'https://github.com/Commanderx-code/Helix-toolbox/issues/new?template=hardware_report.yml',
      };
    case 'open_issue':
      return undefined;
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
    case 'read_services':
      return {
        systemd: true,
        session: true,
        units: [
          { name: 'bluetooth.service', description: 'Bluetooth service', enablement: 'enabled', active: 'active', sub: 'running', user: false },
          { name: 'cups.service', description: 'CUPS Scheduler', enablement: 'disabled', active: 'inactive', sub: 'dead', user: false },
          { name: 'logrotate.service', description: 'Rotate log files', enablement: 'static', active: 'failed', sub: 'failed', user: false },
          { name: 'sddm.service', description: 'Simple Desktop Display Manager', enablement: 'enabled', active: 'active', sub: 'running', user: false },
          { name: 'fstrim.timer', description: 'Discard unused filesystem blocks once a week', enablement: 'enabled', active: 'active', sub: 'waiting', user: false },
          { name: 'pipewire.service', description: 'PipeWire Multimedia Service', enablement: 'indirect', active: 'active', sub: 'running', user: true },
        ],
        autostart: [
          { file: 'org.kde.discover.notifier.desktop', name: 'Discover', comment: 'Update notifier', exec: '/usr/lib/DiscoverNotifier --check-delay 20', enabled: true, here: true, system: true, own: null },
        ],
      } satisfies ServicesView;
    case 'service_details':
      return args.name === 'sddm.service'
        ? { needed_by: ['graphical.target'], consequence: 'It runs the login screen: stopped, your desktop session ends now; disabled, the next boot stops at a text console, where you log in and enable it again.' }
        : { needed_by: [], consequence: null };
    case 'prepare_service':
    case 'prepare_autostart':
      return {
        kind: 'ready',
        risk: args.name === 'sddm.service' ? 'high' : 'medium',
        privileged: !args.user,
        header: ['Risk: medium · asks for your password'],
        lines: [{ name: `${String(args.verb ?? (args.on ? 'start' : 'stop'))} ${String(args.name ?? args.file)}`, detail: `sudo systemctl ${String(args.verb)} ${String(args.name)}` }],
      } satisfies Prepared;
    case 'read_storage':
      return {
        filesystems: [{ path: '/', free: 588_000_000_000, total: 1_000_000_000_000 }],
        items: [
          { id: 'package-cache', title: 'Package cache', detail: 'Downloaded packages pacman keeps: the two newest versions of each installed package stay.', size: 2_000_000_000, frees: 34_200_000, packages: [], not_offered: null },
          { id: 'unused-packages', title: 'Unused packages', detail: 'Packages nothing needs now. Kernels are never among them.', size: 184_000_000, frees: 184_000_000, packages: ['libfoo', 'python-old'], not_offered: null },
          { id: 'trash', title: 'The trash', detail: 'Files you deleted to the trash, deleted for good.', size: 0, frees: 0, packages: [], not_offered: 'the trash is empty' },
        ],
        kept: [{ title: 'Kernels', detail: '2 installed: the kernels the machine can boot are never removed here.', size: null }],
      } satisfies StorageView;
    case 'prepare_clean':
      return {
        kind: 'ready',
        risk: 'low',
        privileged: true,
        header: ['Risk: low · asks for your password'],
        lines: [{ name: `Clean: ${String(args.id)}`, detail: 'sudo paccache -rk2; then sudo paccache -ruk0' }],
      } satisfies Prepared;
    case 'read_performance': {
      const boot = (id: string, booted: number, userspace: number, waited: number) => ({
        id,
        booted,
        kernel: '7.2.11-1-garuda',
        phases: { firmware: 10_458, loader: 12_297, kernel: 1065, initrd: 2661, userspace, total: 26_481 + userspace },
        slowest: [
          { name: 'plocate-updatedb.service', took: 4183 },
          { name: 'NetworkManager-wait-online.service', took: waited },
        ],
        chain: [
          { name: 'graphical.target', at: userspace, took: null },
          { name: 'NetworkManager-wait-online.service', at: 1003, took: waited },
          { name: 'plymouth-quit.service', at: 3010, took: 2344 },
        ],
      });
      const now = boot('3f2a', 1_792_600_000, 8394, 4344);
      const before = boot('1b7c', 1_792_500_000, 5394, 1344);
      const comparison = {
        before,
        after: now,
        phases: [
          { name: 'Whole boot', before: before.phases.total, after: now.phases.total },
          { name: 'Services', before: 5394, after: 8394 },
        ],
        units: [{ name: 'NetworkManager-wait-online.service', before: 1344, after: 4344 }],
        runs: [{ at: 1_792_550_000, names: ['Docker'], success: true }],
      };
      return {
        found: {
          boot: now,
          no_boot: null,
          now: {
            cpus: 16,
            load: [0.52, 0.61, 0.7],
            memory_total: 16_000_000_000,
            memory_available: 9_200_000_000,
            swaps: [{ name: '/dev/zram0', zram: true, size: 8_000_000_000, used: 120_000_000 }],
            pressure: { cpu: 0, memory: 0, io: 1.2 },
            governor: 'powersave',
            power_profile: 'balanced',
            heaviest: [{ pid: 2001, name: 'firefox', memory: 2_400_000_000 }],
          },
          previous: comparison,
          recorded: 2,
          hints: [
            {
              title: 'The boot waited 4.3 s for NetworkManager-wait-online.service',
              detail: 'It holds back what asks for a working network until one is up.',
              command: 'helix-toolbox services disable NetworkManager-wait-online.service',
              unit: 'NetworkManager-wait-online.service',
              entry: null,
            },
          ],
        },
        boots: [now, before],
      } satisfies PerformanceView;
    }
    case 'compare_boots':
      return (await call('read_performance') as PerformanceView).found.previous;
    case 'read_recovery':
      return {
        snapshots: 'Snapper',
        checks: [
          { id: 'snapshots', title: 'System snapshots', health: 'good', health_label: 'OK', summary: 'Snapper is set up; listing its snapshots needs root', evidence: ['The root file system is btrfs'], advice: 'helix-toolbox recovery --check-snapshots lists them and looks inside the newest, reading only.', repair: null, entry: null },
          { id: 'second-kernel', title: 'A second kernel', health: 'good', health_label: 'OK', summary: '2 kernels can be started: linux-lts, linux-zen', evidence: [], advice: null, repair: null, entry: null },
          { id: 'own-files', title: 'Your own files', health: 'notice', health_label: 'Notice', summary: 'Nothing Helix Toolbox knows of backs up your home folder', evidence: [], advice: 'A backup program such as Pika Backup, Déjà Dup, Borg or restic, copying to another disk, keeps your own files when the disk does not.', repair: null, entry: null },
        ],
      } satisfies RecoveryView;
    case 'read_migration_export':
    case 'export_migration': {
      const migration: Migration = {
        version: 1,
        from: 'Garuda Linux',
        family: 'arch',
        architecture: 'x86_64',
        made: 1_792_000_000,
        entries: ['git', 'neovim', 'ripgrep', 'firefox'],
        flatpaks: ['com.spotify.Client', 'org.gimp.GIMP'],
        packages: ['htop', 'yt-dlp', 'paru'],
      };
      if (command === 'export_migration') {
        return `4 entries, 2 Flatpaks and 3 packages of yours are written to ${args.path ?? '/home/me/helix-toolbox-migration-2026-10-22.toml'}.`;
      }
      return { migration, file_name: 'helix-toolbox-migration-2026-10-22.toml', packages_known: true } satisfies MigrationExportView;
    }
    case 'open_migration':
      return {
        path: (args.path as string | null) ?? '/home/me/helix-toolbox-migration-2026-10-14.toml',
        migration: { version: 1, from: 'Fedora Linux 43', family: 'fedora', architecture: 'x86_64', made: 1_792_000_000, entries: [], flatpaks: [], packages: [] },
        same_family: false,
        items: [
          { kind: 'entry', id: 'kitty', name: 'Kitty', standing: 'install', detail: 'its script' },
          { kind: 'flatpak', id: 'com.spotify.Client', name: 'com.spotify.Client', standing: 'install', detail: 'Flathub' },
          { kind: 'entry', id: 'htop', name: 'htop', standing: 'install', detail: 'pacman; for htop there' },
          { kind: 'entry', id: 'git', name: 'Git', standing: 'here', detail: 'pacman lists git' },
          { kind: 'entry', id: 'vlc', name: 'vlc', standing: 'unavailable', detail: 'not in the catalog here' },
          { kind: 'package', id: 'python3-requests', name: 'python3-requests', standing: 'yours', detail: 'named as on Fedora Linux 43; look for what Garuda Linux calls it' },
          { kind: 'package', id: 'tree', name: 'tree', standing: 'yours', detail: "pacman's repositories here have a package of this name; check it is the same" },
        ],
        install: 3,
        here: 1,
        unavailable: 1,
        yours: 2,
      } satisfies TranslationView;
    case 'prepare_migration':
      return {
        kind: 'ready',
        risk: 'medium',
        privileged: true,
        header: ['Risk: medium · asks for your password'],
        lines: [
          { name: 'Kitty', detail: '/bin/sh -e kitty-setup.sh' },
          { name: 'com.spotify.Client', detail: 'sudo flatpak install flathub com.spotify.Client' },
          { name: 'htop', detail: 'sudo pacman -S --needed htop' },
        ],
      } satisfies Prepared;
    case 'prepare_snapshot_check':
      return {
        kind: 'ready',
        risk: 'low',
        privileged: true,
        header: ['Risk: low · asks for your password', 'Note: It only reads: nothing is rolled back, taken or removed.'],
        lines: [{ name: "Look at Snapper's snapshots", detail: 'sudo snapper -c root list; sudo btrfs subvolume show /.snapshots/<newest>/snapshot' }],
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
    case 'save_watch':
      notifications = args.updates as boolean;
      health = args.health as boolean;
      watched = (args.drift as string | null) ?? null;
      return notifications || health || watched
        ? 'Helix Toolbox checks every 6 hours, and soon after you log in, and shows a notification when something is new; a click on it opens the screen here. It never updates, repairs or puts back anything itself.'
        : 'Notifications are off.';
    case 'read_settings':
    case 'save_settings':
      return { keep_going: args.keepGoing ?? false, snapshots: args.snapshots ?? true, snapshot_tool: 'Snapper', path: '/home/me/.config/helix-toolbox/settings.toml', theme, notifications, health, drift: watched, baselines: [['fresh-install', 'Fresh install']] };
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
    checks: steps.map((step) => ({ name: step.name, passed: true, detail: `pacman lists ${step.id}`, unsure: false, later: false })),
  });
}
