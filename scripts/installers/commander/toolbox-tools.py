"""Interactive maintenance tools. Never require root for read-only operations."""
import datetime
import importlib.util
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

from configure import check_destination, deploy


def run(args, *, check=True, capture=False, cwd=None, timeout=None):
    return subprocess.run(args, check=check, text=True, cwd=cwd, timeout=timeout,
                          stdout=subprocess.PIPE if capture else None,
                          stderr=subprocess.STDOUT if capture else None)


def confirm(message, token='APPLY'):
    print(message)
    if input(f'Type {token} to continue: ').strip() != token:
        raise ValueError('Cancelled; no further changes made.')


def state_dir():
    return Path(os.environ.get('XDG_STATE_HOME', Path.home() / '.local/state')) / 'helixforge'


def safe_text(text):
    # Prevent log output from injecting terminal control sequences when viewed.
    text = re.sub(r'\x1b\][^\x07]*(?:\x07|\x1b\\)', '', text)
    text = re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]', '', text)
    return ''.join(c for c in text if c in '\n\t' or (ord(c) >= 32 and not 127 <= ord(c) <= 159))


def diagnostics():
    reports = ['Commander Toolbox diagnostics (read-only)', Path('/etc/os-release').read_text()]
    commands = [
        ['uname', '-srmo'], ['systemd-detect-virt'], ['df', '-h', '/', str(Path.home())],
        ['lspci', '-nnk'], ['nvidia-smi', '--query-gpu=name,driver_version,memory.total', '--format=csv'],
        ['clinfo', '-l'], ['vulkaninfo', '--summary'], ['mokutil', '--sb-state'],
    ]
    for args in commands:
        reports.append('\n$ ' + ' '.join(args))
        if not shutil.which(args[0]):
            reports.append('Not installed; skipped.')
            continue
        try:
            result = run(args, check=False, capture=True, timeout=15)
            reports.append(safe_text(result.stdout))
            reports.append(f'Exit status: {result.returncode}')
        except subprocess.TimeoutExpired:
            reports.append('Timed out after 15 seconds.')
    reports.append('\nGraphics device access:')
    for device in sorted(Path('/dev/dri').glob('renderD*')) + [Path('/dev/kfd')]:
        if device.exists():
            reports.append(f'{device}: read={os.access(device, os.R_OK)} write={os.access(device, os.W_OK)}')
    reports.append('A Virtio/VMware/VirtualBox adapter does not establish Resolve GPU support.\n'
                   'Missing diagnostic tools or successful checks do not prove application compatibility.')
    report = '\n'.join(reports)
    print(report)
    if input('Save this report to your private toolbox state directory? [y/N]: ').lower() == 'y':
        directory = state_dir() / 'diagnostics'
        check_destination(directory)
        directory.mkdir(parents=True, exist_ok=True, mode=0o700)
        path = directory / (datetime.datetime.now().strftime('%Y%m%d-%H%M%S-%f') + '.txt')
        with path.open('x') as handle:
            path.chmod(0o600)
            handle.write(report)
        print(f'Saved: {path}')


BACKUP = re.compile(r'^(.+)\.commander-backup-[0-9][0-9-]*$')


def discover_backups(home=None):
    home = Path(home or Path.home())
    roots = [Path(os.environ.get('XDG_CONFIG_HOME', home / '.config')),
             home / '.local/bin', Path(os.environ.get('XDG_DATA_HOME', home / '.local/share'))]
    found = [p for p in home.iterdir() if BACKUP.fullmatch(p.name) and not p.is_symlink()]
    for root in roots:
        if not root.is_dir() or root.is_symlink():
            continue
        for directory, dirs, files in os.walk(root, followlinks=False):
            base = Path(directory)
            for name in dirs + files:
                path = base / name
                if BACKUP.fullmatch(name) and not path.is_symlink():
                    found.append(path)
            dirs[:] = [d for d in dirs if not BACKUP.fullmatch(d) and d not in
                       ('.git', 'node_modules', 'sources', 'Trash', 'containers', 'Steam')
                       and not (base / d).is_symlink()]
    return sorted(set(found), key=lambda p: p.stat().st_mtime, reverse=True)


def restore_backup(source):
    source = Path(source)
    match = BACKUP.fullmatch(source.name)
    if not match:
        raise ValueError('Not a Commander backup.')
    check_destination(source)
    destination = source.with_name(match[1])
    check_destination(destination)
    # Never follow links inside a backup into another configuration owner.
    if source.is_dir():
        for directory, dirs, files in os.walk(source, followlinks=False):
            if any((Path(directory) / name).is_symlink() for name in dirs + files):
                raise ValueError('Backup contains symlinks; restore it through its configuration owner.')
    deploy(source, destination)
    return destination


def backups():
    paths = discover_backups()
    if not paths:
        print('No Commander user-configuration backups found. System/boot backups are not restored here.')
        return
    for index, path in enumerate(paths, 1):
        print(f'{index}. {path}')
    selection = input('Backup number to restore (Enter cancels): ').strip()
    if not selection:
        return
    if not selection.isdigit() or not 1 <= int(selection) <= len(paths):
        raise ValueError('Invalid backup number.')
    source = paths[int(selection) - 1]
    target = source.with_name(BACKUP.fullmatch(source.name)[1])
    confirm(f'Restore {source}\nto {target}\nThe current target will receive its own backup.')
    print(f'Restored: {restore_backup(source)}')


def history():
    directory = state_dir() / 'history'
    check_destination(directory)
    if not directory.exists():
        print('No action history yet. Only actions run with the updated toolbox are recorded.')
        return
    runs = sorted((p for p in directory.iterdir() if p.is_dir() and not p.is_symlink()), reverse=True)
    for index, path in enumerate(runs, 1):
        try:
            timestamp = datetime.datetime.fromtimestamp(int(path.name.split('-')[0]) / 1_000_000_000).isoformat(' ', timespec='seconds')
        except (ValueError, OverflowError, OSError):
            timestamp = path.name
        print(f'\n{index}. {timestamp}')
        for name in sorted(path.glob('*.name')):
            status = name.with_suffix('.status')
            result = status.read_text().strip() if status.exists() else 'unknown'
            result = {'0': 'success', 'pending': 'not started', 'running': 'running or interrupted'}.get(result, f'exit {result}')
            print(f'   {safe_text(name.read_text().strip())}: {result}')
        print('   Saved log available' if (path / 'output.log').is_file() else '   No output log saved')
    choice = input('\nRun number to read its saved log (Enter closes): ').strip()
    if not choice:
        return
    if not choice.isdigit() or not 1 <= int(choice) <= len(runs):
        raise ValueError('Invalid run number.')
    log = runs[int(choice) - 1] / 'output.log'
    check_destination(log)
    if not log.is_file():
        print('No saved log. Use L after an action finishes to save its terminal output.')
        return
    print(safe_text(log.read_text(errors='replace')))


def git_command(repo, *args):
    # The checkout path is user-supplied: never let repository-local config or
    # hooks run commands (fsmonitor on status, post-merge on fast-forward).
    return ['git', '-c', 'core.fsmonitor=false', '-c', 'core.hooksPath=/dev/null', '-C', str(repo), *args]


def git(repo, *args):
    return run(git_command(repo, *args), capture=True).stdout.strip()


def resolve_maintenance():
    print('1. Native dependency check\n2. Container status and dependency check\n3. Repair native launcher\n'
          '4. Repair container launcher\n5. Remove Resolve container\n6. Native removal instructions')
    choice = input('Choose [1-6], Enter cancels: ').strip()
    if not choice:
        return
    if choice == '6':
        print('Back up/export Resolve projects first. For a native vendor install, use its uninstaller at\n'
              '/opt/resolve/installer (run its help to inspect uninstall options). For an AUR-managed install,\n'
              'use your package manager. This tool does not delete native Resolve files or project data.')
        return
    if choice not in ('1', '2', '3', '4', '5'):
        raise ValueError('Invalid choice.')
    container = choice in ('2', '4', '5')
    os.environ['DBX_CONTAINER_MANAGER'] = 'podman'
    if container:
        run(['podman', 'container', 'exists', 'commander-resolve'])
        run(['podman', 'ps', '-a', '--filter', 'name=^commander-resolve$', '--format', '{{.Names}} {{.Status}} {{.Image}}'])
        prefix = ['distrobox', 'enter', '--name', 'commander-resolve', '--']
    else:
        if not Path('/opt/resolve/bin/resolve').is_file():
            raise ValueError('Native Resolve was not found.')
        prefix = []
    if choice == '5':
        confirm('Remove commander-resolve and its container-local data? Export/back up projects first.\n'
                'Shared home files and host GPU drivers remain.', 'REMOVE commander-resolve')
        run(['distrobox', 'rm', '--force', 'commander-resolve'])
        print('Container removed. Its launcher can be restored/replaced through Config Backup Restore or launcher repair.')
        return
    spec = importlib.util.spec_from_file_location('resolve_launcher', Path(__file__).with_name('resolve-launcher.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    with tempfile.TemporaryDirectory(prefix='.resolve-check-', dir=Path.home()) as tmp:
        script = Path(tmp) / 'runtime.sh'
        script.write_text(module.NATIVE)
        run(prefix + ['sh', str(script), '--check'])
    if choice in ('3', '4'):
        confirm('Recreate the Commander Resolve launcher and desktop entry with backups?')
        module.install('container' if container else 'native')
    print('Dependency check passed. This does not prove GPU acceleration or codec support.')


if __name__ == '__main__':
    actions = {'diagnostics': diagnostics, 'backups': backups, 'history': history,
               'resolve': resolve_maintenance}
    try:
        actions[sys.argv[1]]()
    except (ValueError, OSError, subprocess.CalledProcessError, EOFError, KeyboardInterrupt) as error:
        print(f'Stopped: {error}', file=sys.stderr)
        sys.exit(1)
