// Sample answers for working on the interface in a browser, where there is
// no Tauri side. Nothing here reads or changes the machine.

import type { AreaView, BundleContents, ChangesView, Comparison, ConfigView, EntryView, HardwareView, HomeManagerView, ItemView, Migration, MigrationExportView, PerformanceView, Prepared, RecordView, RecoveryView, RepositoryView, ReposView, ResticView, ResticNode, ResticHistory, RunEvent, TranslationView, ServicesView, StatusView, StorageView, WindowTheme } from './types';

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
        person: 'Sample',
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
      return {
        firmware: ['0 devices, 0 updatable through fwupd; no updates in the metadata fwupd has'],
        updates: [],
        disks: [{ name: 'nvme0n1', label: 'Sample SSD', detail: 'NVMe · 1.0 TB · firmware 1.0 · 38 °C', attention: false }],
        sensors: [{ name: 'coretemp Package', label: '52 °C', detail: 'critical 100', attention: false }],
        batteries: [{ name: 'BAT0', label: '80% charged, charging', detail: 'holds 91% of its design capacity · 120 cycles · Li-ion', attention: false }],
        pci: [],
        usb: [],
      } satisfies HardwareView;
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
        downloads: [args.complete ? '41 package files to download, every dependency.' : '7 package files to download, with the dependencies this machine lacks.'],
      } satisfies BundleContents;
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
        size: '48.2 MB',
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
    case 'read_services': {
      const unit = (name: string, description: string, enablement: string, active: string, sub: string, user: boolean, kind = 'service') => {
        const switchable = ['enabled', 'disabled', 'enabled-runtime', 'indirect', 'linked', 'alias'].includes(enablement);
        return {
          name,
          description,
          user,
          kind,
          state: active,
          detail: `${enablement} · ${description}`,
          enablement,
          now: sub === active ? active : `${active} (${sub})`,
          note: switchable ? null : 'It is not enabled or disabled itself: another unit or a request starts it.',
          running: active === 'active',
          failed: active === 'failed',
          enabled: enablement === 'enabled',
          switchable,
        };
      };
      return {
        systemd: true,
        units: [
          unit('bluetooth.service', 'Bluetooth service', 'enabled', 'active', 'running', false),
          unit('cups.service', 'CUPS Scheduler', 'disabled', 'inactive', 'dead', false),
          unit('logrotate.service', 'Rotate log files', 'static', 'failed', 'failed', false),
          unit('sddm.service', 'Simple Desktop Display Manager', 'enabled', 'active', 'running', false),
          unit('fstrim.timer', 'Discard unused filesystem blocks once a week', 'enabled', 'active', 'waiting', false, 'timer'),
          unit('pipewire.service', 'PipeWire Multimedia Service', 'indirect', 'active', 'running', true),
        ],
        autostart: [
          {
            file: 'org.kde.discover.notifier.desktop',
            name: 'Discover',
            comment: 'Update notifier',
            exec: '/usr/lib/DiscoverNotifier --check-delay 20',
            enabled: true,
            here: true,
            state: 'on',
            runs: '/usr/lib/DiscoverNotifier --check-delay 20',
            from: 'the system',
            at_login: 'started',
          },
        ],
      } satisfies ServicesView;
    }
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
        filesystems: [{ path: '/', text: '588 GB free of 1.0 TB', meter: { percent: 41, full: false } }],
        items: [
          { id: 'package-cache', title: 'Package cache', detail: 'Downloaded packages pacman keeps: the two newest versions of each installed package stay.', packages: [], size: '2.0 GB', said: 'frees 34.2 MB', offered: true },
          { id: 'unused-packages', title: 'Unused packages', detail: 'Packages nothing needs now. Kernels are never among them.', packages: ['libfoo', 'python-old'], size: '184 MB', said: 'frees 184 MB', offered: true },
          { id: 'trash', title: 'The trash', detail: 'Files you deleted to the trash, deleted for good.', packages: [], size: '0 B', said: 'the trash is empty', offered: false },
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
      const comparison: Comparison = {
        before: { id: '1b7c', booted: 1_792_500_000, total: '31.9 s' },
        total: '+3.0 s',
        sign: 1,
        changes: [
          { name: 'Services', now: '8.4 s', how: '+3.0 s', slower: true },
          { name: 'NetworkManager-wait-online.service', now: '4.3 s', how: '+3.0 s', slower: true },
        ],
        kernel: null,
        runs: [{ at: 1_792_550_000, ran: 'Docker' }],
      };
      return {
        boot: {
          id: '3f2a',
          booted: 1_792_600_000,
          kernel: '7.2.11-1-garuda',
          total: '34.9 s',
          parts: [
            { name: 'Firmware', took: 10_458, shown: '10.5 s', tone: 'firmware' },
            { name: 'Boot loader', took: 12_297, shown: '12.3 s', tone: 'loader' },
            { name: 'Kernel', took: 1065, shown: '1.1 s', tone: 'kernel' },
            { name: 'Initial RAM disk', took: 2661, shown: '2.7 s', tone: 'initrd' },
            { name: 'Services', took: 8394, shown: '8.4 s', tone: 'services' },
          ],
          waited: [
            { name: 'NetworkManager-wait-online.service', took: '4.3 s' },
            { name: 'plymouth-quit.service', took: '2.3 s' },
          ],
          slowest: [
            { name: 'plocate-updatedb.service', took: '4.2 s' },
            { name: 'NetworkManager-wait-online.service', took: '4.3 s' },
          ],
        },
        no_boot: null,
        previous: comparison,
        earlier: [comparison.before],
        now: [
          { term: 'Load', text: '0.52 0.61 0.70', note: '(1, 5 and 15 minutes) on 16 processors', meter: null },
          { term: 'Memory', text: '9.2 GB free of 16.0 GB', note: '', meter: { percent: 43, full: false } },
          { term: 'Swap', text: '120 MB used of 8.0 GB (zram)', note: '', meter: null },
          { term: 'Waiting, last minute', text: 'processor 0%, memory 0%, disk 1%', note: '', meter: null },
          { term: 'Processor', text: 'governor powersave, power profile balanced', note: '', meter: null },
          { term: 'Most memory', text: 'firefox 2.4 GB', note: '', meter: null },
        ],
        hints: [
          {
            title: 'The boot waited 4.3 s for NetworkManager-wait-online.service',
            detail: 'It holds back what asks for a working network until one is up.',
            command: 'helix-toolbox services disable NetworkManager-wait-online.service',
            unit: 'NetworkManager-wait-online.service',
            entry: null,
          },
        ],
      } satisfies PerformanceView;
    }
    case 'compare_boots':
      return (await call('read_performance') as PerformanceView).previous;
    case 'read_recovery':
      return {
        snapshots: 'Snapper',
        checks: [
          { id: 'snapshots', title: 'System snapshots', health: 'good', health_label: 'OK', summary: 'Snapper is set up; listing its snapshots needs root', evidence: ['The root file system is btrfs'], advice: 'helix-toolbox recovery --check-snapshots lists them and looks inside the newest, reading only.', repair: null, entry: null },
          { id: 'second-kernel', title: 'A second kernel', health: 'good', health_label: 'OK', summary: '2 kernels can be started: linux-lts, linux-zen', evidence: [], advice: null, repair: null, entry: null },
          { id: 'own-files', title: 'Your own files', health: 'notice', health_label: 'Notice', summary: 'Nothing Helix Toolbox knows of backs up your home folder', evidence: [], advice: 'A backup program such as Pika Backup, Déjà Dup, Borg or restic, copying to another disk, keeps your own files when the disk does not.', repair: null, entry: null },
        ],
      } satisfies RecoveryView;
    case 'read_repositories':
      return {
        roots: ['/home/me/github', '/home/me/dotfiles'],
        depth: 3,
        origin: 'default',
        repositories: [
          { name: 'helix-toolbox', path: '/home/me/github/helix-toolbox', branch: 'main', changed: 2, upstream: 'origin/main', ahead: 1, behind: 0, last: 'Helix Toolbox 1.19.0', last_at: 1_792_600_000, origin: 'https://github.com/me/helix-toolbox.git', problem: null, favorite: true, group: '' },
          { name: 'dotfiles', path: '/home/me/dotfiles', branch: 'main', changed: 0, upstream: 'origin/main', ahead: 0, behind: 0, last: 'Ghostty: a smaller font', last_at: 1_792_400_000, origin: 'git@github.com:me/dotfiles.git', problem: null, favorite: false, group: '' },
          { name: 'scratch', path: '/home/me/github/scratch', branch: null, changed: 1, upstream: null, ahead: null, behind: null, last: 'Try things', last_at: 1_790_000_000, origin: null, problem: null, favorite: false, group: '' },
        ],
      } satisfies ReposView;
    case 'read_repository':
      return {
        summary: { name: 'helix-toolbox', path: (args.path as string) ?? '/home/me/github/helix-toolbox', branch: 'main', changed: 2, upstream: 'origin/main', ahead: 1, behind: 0, last: 'Helix Toolbox 1.19.0', last_at: 1_792_600_000, origin: 'https://github.com/me/helix-toolbox.git', problem: null },
        changes: [
          { path: 'README.md', original: null, state: 'modified', staged: true, unstaged: false, conflict: false },
          { path: 'notes.txt', original: null, state: 'new', staged: false, unstaged: true, conflict: false },
        ],
        branches: ['main', 'topic'],
        remotes: ['origin'],
        stashes: [{ id: 'a1b2c3d4e5f6', name: 'stash@{0}', subject: 'On main: Halfway there' }],
        log: [
          { hash: 'b27349a', subject: 'Helix Toolbox 1.19.0', at: 1_792_600_000 },
          { hash: '04b540e', subject: 'Helix Toolbox 1.18.0', at: 1_792_500_000 },
        ],
        head: 'b27349a0000000000000000000000000000000000',
        branch_ref: 'refs/heads/main',
        busy: null,
        push_to: ['origin', 'refs/heads/main'],
        staged_stat: ' README.md | 2 +-\n',
        favorite: true,
        tasks: [
          { id: 'serve', name: 'npm run serve', command: 'npm run serve', shell: 'direct' },
          { id: 'cargo-test', name: 'cargo test', command: 'cargo test', shell: 'direct', detected: true },
        ],
      } satisfies RepositoryView;
    case 'repository_diff':
      return { text: '--- a/README.md\n+++ b/README.md\n@@ -1 +1 @@\n-Old line\n+New line\n', truncated: false };
    case 'prepare_repository_change':
    case 'prepare_task':
      return {
        kind: 'ready',
        risk: 'low',
        privileged: false,
        header: ['Risk: low'],
        lines: [{ name: 'Commit 1 staged file to main', detail: "cd /home/me/github/helix-toolbox && git --literal-pathspecs commit -m 'Change the README'" }],
      } satisfies Prepared;
    case 'save_workspace':
      return null;
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
          { name: 'com.spotify.Client', detail: 'sudo flatpak install -y flathub com.spotify.Client' },
          { name: 'htop', detail: 'sudo pacman -S --needed htop' },
        ],
      } satisfies Prepared;
    case 'read_configs':
      return [
        {
          kind: 'ghostty',
          label: 'Ghostty',
          problem: null,
          path: '/home/me/.config/ghostty/config',
          live: '/home/me/.config/ghostty/config',
          origin: 'the live file',
          managed: false,
          flake: null,
          exists: true,
          revision: '5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03',
          content: 'font-family = JetBrains Mono\nfont-size = 12\ntheme = catppuccin-mocha\n',
          checked: { state: 'valid' },
          fields: [
            { key: 'font-family', label: 'Font', hint: 'a font name', value: 'JetBrains Mono' },
            { key: 'font-size', label: 'Font size', hint: 'points, such as 12', value: '12' },
            { key: 'theme', label: 'Theme', hint: 'a theme name', value: 'catppuccin-mocha' },
          ],
          backups: [{ backup: '/home/me/.config/ghostty/config.helix-toolbox-backup-20261021220004-4120', taken: '2026-10-21 22:00:04' }],
        },
        {
          kind: 'fastfetch',
          label: 'Fastfetch',
          problem: null,
          path: '/home/me/dotfiles/configs/fastfetch/config.jsonc',
          live: '/home/me/.config/fastfetch/config.jsonc',
          origin: 'your dotfiles',
          managed: true,
          flake: '/home/me/dotfiles',
          exists: true,
          revision: '2ed6a3a00460219dd6d364e9556828467b0f39d52508d70f3dad19688ef8f9d0',
          content: '{\n    "logo": { "source": "arch" },\n    "modules": ["title", "os", "memory"]\n}\n',
          checked: { state: 'valid' },
          fields: [
            { key: 'logo.source', label: 'Logo', hint: 'a distribution name or a file', value: 'arch' },
            { key: 'logo.type', label: 'Logo type', hint: 'auto, small, kitty…', value: null },
            { key: 'modules', label: 'Modules', hint: 'their names in order', value: 'title, os, memory' },
          ],
          backups: [],
        },
      ] satisfies ConfigView[];
    case 'validate_config':
      return String(args.text).includes('bogus') ? { state: 'invalid', message: 'Ghostty rejected it: unknown field bogus' } : { state: 'valid' };
    case 'set_config_field':
      return { content: `${String(args.text).trimEnd()}\n${String(args.key)} = ${String(args.value)}\n`, fields: [] };
    case 'prepare_config_save':
    case 'prepare_config_restore':
      return {
        kind: 'ready',
        risk: 'low',
        privileged: false,
        header: ['Risk: low'],
        lines: [{ name: "Save Ghostty's configuration", detail: 'the new text written over /home/me/.config/ghostty/config (the current one kept as a backup first)' }],
      } satisfies Prepared;
    case 'read_restic':
      return {
        configured: true,
        repository: '/run/media/me/Backup/restic',
        origin: "Helix Toolbox's settings",
        password: 'the file /home/me/.config/restic/password',
        paths: ['/home/me'],
        excludes: ['/home/me/.cache', '/home/me/.local/share/Trash'],
        problem: null,
        snapshots: [
          { id: 'b6f77f6d0a1b2c3d', short_id: 'b6f77f6d', time: '2026-10-21T22:00:04+02:00', hostname: 't15p', paths: ['/home/me'] },
          { id: 'aa0d9cfe9e8d7c6b', short_id: 'aa0d9cfe', time: '2026-10-20T22:00:02+02:00', hostname: 't15p', paths: ['/home/me'] },
        ],
        timer: 'every day',
        restores: true,
        baselines: [{ id: '1792600000000', path: '/home/me/Documents/taxes.ods', sha256: '5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03', recorded: 1_792_600_000 }],
        home: '/home/me',
      } satisfies ResticView;
    case 'restic_files':
      return [
        { name: 'Documents', type: 'dir', path: '/home/me/Documents' },
        { name: 'notes.txt', type: 'file', path: '/home/me/notes.txt', size: 1200 },
      ] satisfies ResticNode[];
    case 'restic_history':
      return [
        {
          path: '/home/me/notes.txt',
          missing_from: 0,
          versions: [
            { snapshot: 'b6f77f6d0a1b2c3d', short_id: 'b6f77f6d', time: '2026-10-21T22:00:04+02:00', hostname: 't15p', size: 1200, mtime: null, change: 'changed' },
            { snapshot: 'aa0d9cfe9e8d7c6b', short_id: 'aa0d9cfe', time: '2026-10-20T22:00:02+02:00', hostname: 't15p', size: 1100, mtime: null, change: 'first' },
          ],
        },
      ] satisfies ResticHistory[];
    case 'prepare_restic':
      return {
        kind: 'ready',
        risk: 'low',
        privileged: false,
        header: ['Risk: low'],
        lines: [{ name: 'Restore from the restic repository', detail: "mkdir '/home/me/restored'; restic restore b6f77f6d --target /home/me/restored --overwrite never --verify" }],
      } satisfies Prepared;
    case 'record_baseline':
      return { id: '1792600000001', path: String(args.path), sha256: '5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03', recorded: 1_792_600_001 };
    case 'set_restic_timer':
      return args.on ? 'The backup runs every day.' : 'The timed backup is off.';
    case 'read_home_manager':
      return {
        installed: true,
        version: '26.11-pre',
        config: { folder: '/home/me/dotfiles/home-manager', flake: true, profile: 'me', origin: 'settings' },
        problem: null,
        generations: [
          { id: 201, built: '2026-10-20 12:01', path: '/nix/store/aaa-home-manager-generation', current: true },
          { id: 200, built: '2026-10-16 13:23', path: '/nix/store/bbb-home-manager-generation', current: false },
        ],
        repository: { name: 'dotfiles', path: '/home/me/dotfiles', branch: 'main', changed: 0, upstream: 'origin/main', ahead: 0, behind: 2, last: 'Ghostty: a smaller font', last_at: 1_792_400_000, origin: 'git@github.com:me/dotfiles.git', problem: null },
        links: [
          { name: 'Ghostty', path: '/home/me/.config/ghostty/config', target: '/nix/store/xyz-home-manager-files/.config/ghostty/config', managed: true },
          { name: 'Fastfetch', path: '/home/me/.config/fastfetch/config.jsonc', target: null, managed: false },
        ],
        reference: '/home/me/dotfiles/home-manager#me',
        origin: "Helix Toolbox's settings",
      } satisfies HomeManagerView;
    case 'prepare_home_manager':
      return {
        kind: 'ready',
        risk: args.action === 'build' ? 'low' : 'medium',
        privileged: false,
        header: [`Risk: ${args.action === 'build' ? 'low' : 'medium'}`],
        lines: [{ name: 'Switch to the Home Manager configuration', detail: "home-manager switch --flake '/home/me/dotfiles/home-manager#me'" }],
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
    case 'close_run':
      // Every pretend run finishes.
      return { remaining: [], read_again: ['statuses', 'updates', 'tools', 'history'], not_recorded: null };
    case 'read_settings':
    case 'save_settings':
      return { keep_going: args.keepGoing ?? false, snapshots: args.snapshots ?? true, snapshot_tool: 'Snapper', path: '/home/me/.config/helix-toolbox/settings.toml', theme, notifications, health, drift: watched, baselines: [['fresh-install', 'Fresh install']], restic_timer: null, restic_ready: true };
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
