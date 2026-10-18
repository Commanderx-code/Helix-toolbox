#!/usr/bin/env python3
"""Render portable configuration; user-file writes always retain a backup."""
import configparser
import datetime
import io
import json
import os
from pathlib import Path
import re
import shutil
import shlex
import sys
import tempfile


def check_destination(path):
    for candidate in (path, *path.parents):
        if candidate.is_symlink():
            raise ValueError(f'{candidate} is symlink-managed; update its owner instead.')


def deploy(source, destination, text=None):
    check_destination(destination)
    destination.parent.mkdir(parents=True, exist_ok=True)
    backup = None
    if destination.exists():
        stamp = datetime.datetime.now().strftime('%Y%m%d%H%M%S%f')
        backup = destination.with_name(destination.name + '.commander-backup-' + stamp)
        destination.rename(backup)
        print(f'Backup: {backup}')
    try:
        if text is not None:
            destination.write_text(text)
        elif source.is_dir():
            shutil.copytree(source, destination)
        else:
            shutil.copy2(source, destination)
    except Exception:
        if destination.is_dir():
            shutil.rmtree(destination)
        elif destination.exists():
            destination.unlink()
        if backup:
            backup.rename(destination)
        raise
    print(f'Installed: {destination}')


def render_topgrade(text, distro):
    # Source templates refer to a particular workstation. Let Home Manager use
    # the target's own default configuration and keep pinned sources immutable.
    text = re.sub(r'home_manager_arguments\s*=\s*\[.*?\]', '', text, flags=re.S)
    text = re.sub(r'repos\s*=\s*\[.*?\]', 'repos = []', text, flags=re.S)
    if distro != 'garuda':
        text = text.replace('arch_package_manager = "garuda_update"', 'arch_package_manager = "autodetect"')
    text = text.replace('assume_yes = true', 'assume_yes = false')
    text = text.replace("# Don't ask for confirmation before each update.", '# Ask before updates in the portable toolbox profile.')
    text = text.replace("# Use Garuda's normal system update mechanism.", '# Use Garuda updates on Garuda; autodetect elsewhere.')
    text = text.replace('# Automatically update the dotfiles repository.', '# Add your own editable repositories here; pinned toolbox sources stay unchanged.')
    text = text.replace('# Home Manager flake.', '# Home Manager uses its normal configuration on this machine.')
    if '@HOME_MANAGER_FLAKE@' in text or '@DOTFILES_DIRECTORY@' in text:
        raise ValueError('Unresolved Topgrade template placeholders.')
    return text


def render_grub(text, theme='/usr/share/grub/themes/commander/theme.txt',
                background='/usr/share/grub/themes/commander/background.png'):
    # Change only appearance, never transplant kernel, timeout or disk settings.
    text = re.sub(r'^\s*(?:export\s+)?GRUB_(?:THEME|BACKGROUND|TERMINAL_OUTPUT)=.*\n?', '', text, flags=re.M)
    if theme and re.search(r'^\s*(?:export\s+)?GRUB_TERMINAL=', text, re.M):
        raise ValueError('GRUB_TERMINAL overrides graphics output. Configure this console/serial setting explicitly before applying a graphical theme.')
    output = 'gfxterm' if theme else 'console'
    return (text.rstrip() + '\nGRUB_THEME=' + json.dumps(theme)
            + '\nGRUB_BACKGROUND=' + json.dumps(background)
            + '\nGRUB_TERMINAL_OUTPUT=' + json.dumps(output) + '\n')


CTT_GRUB_THEMES = {'ctt-cyberre': 'CyberRe', 'ctt-cyberpunk': 'Cyberpunk', 'ctt-shodan': 'Shodan', 'ctt-fallout': 'fallout', 'ctt-dedsec': 'dedsec', 'ctt-minegrub': 'minegrub', 'ctt-bsol': 'bsol'}


def prepare_grub_theme(repo, destination, variant, resolution='1080p'):
    if variant == 'commander':
        raise ValueError('The Commander theme comes from the grub-theme-garuda-dr460nized package.')
    if variant == 'cachyos':
        source = repo / 'grub/themes/cachyos'
    elif variant == 'catppuccin':
        source = repo / 'src/catppuccin-mocha-grub-theme'
    elif variant == 'dracula':
        source = repo / 'dracula'
    elif variant in CTT_GRUB_THEMES:
        source = repo / 'themes' / CTT_GRUB_THEMES[variant]
    elif variant in ('tela', 'vimix'):
        if resolution not in ('1080p', '2k', '4k'):
            raise ValueError('Unsupported theme resolution.')
        destination.mkdir(parents=True)
        for font in (repo / 'common').glob('*.pf2'):
            shutil.copy2(font, destination / font.name)
        template = (repo / f'config/theme-{resolution}.txt').read_text()
        # Upstream references terminal_box textures that it does not ship.
        template = re.sub(r'^terminal-box:.*\n?', '', template, flags=re.M)
        (destination / 'theme.txt').write_text(template)
        shutil.copy2(repo / f'backgrounds/{resolution}/background-{variant}.jpg', destination / 'background.jpg')
        shutil.copytree(repo / f'assets/assets-color/icons-{resolution}', destination / 'icons')
        for image in (repo / f'assets/assets-select/select-{resolution}').glob('*.png'):
            shutil.copy2(image, destination / image.name)
        shutil.copy2(repo / f'assets/info-{resolution}.png', destination / 'info.png')
        source = None
    else:
        raise ValueError('Unknown GRUB theme.')
    if source:
        shutil.copytree(source, destination)
    # GRUB generators discover fonts beside theme.txt, not in nested folders.
    if variant == 'ctt-shodan':
        for font in (destination / 'fonts').glob('*.pf2'):
            shutil.copy2(font, destination / font.name)
    if (repo / 'LICENSE').is_file():
        shutil.copy2(repo / 'LICENSE', destination / 'LICENSE')
    if not (destination / 'theme.txt').is_file():
        raise ValueError('Theme is missing theme.txt.')



def update_words(current, action, words):
    if action == 'add':
        return current + [word for word in words if word not in current]
    if action == 'remove':
        return [word for word in current if word not in words]
    if action == 'set':
        keys = {word.split('=', 1)[0] for word in words if '=' in word}
        return [word for word in current
                if word not in words and not ('=' in word and word.split('=', 1)[0] in keys)] + list(words)
    raise ValueError('Unknown kernel argument action.')


def render_kernel_cmdline(text, action, words):
    # Add or drop words in GRUB_CMDLINE_LINUX_DEFAULT; every other setting stays.
    pattern = re.compile(r'^([ \t]*(?:export[ \t]+)?GRUB_CMDLINE_LINUX_DEFAULT=)(.*)$', re.M)
    matches = list(pattern.finditer(text))
    current = []
    if matches:
        try:
            current = ' '.join(shlex.split(matches[-1].group(2), comments=True)).split()
        except ValueError as error:
            raise ValueError(f'Cannot read GRUB_CMDLINE_LINUX_DEFAULT: {error}')
    updated = update_words(current, action, words)
    value = json.dumps(' '.join(updated), ensure_ascii=False)
    if matches:
        match = matches[-1]
        return text[:match.start()] + match.group(1) + value + text[match.end():]
    line = 'GRUB_CMDLINE_LINUX_DEFAULT=' + value + '\n'
    return text.rstrip('\n') + '\n' + line if text.strip() else line


LIMINE_MARKER = '# Boot menu appearance from dotfiles (restore-limine).'
LIMINE_KEY = re.compile(r'^[ \t]*([a-z_]+)[ \t]*:')
LIMINE_CMDLINE = re.compile(r'^([ \t]*KERNEL_CMDLINE\[(?:"default"|default)\]\+?=)(.*)$', re.M)


def render_limine_appearance(current, fragment):
    # Merge the dotfiles key: value lines into the global section of limine.conf,
    # which ends at the first "/" entry; the entries stay exactly as they are.
    settings = []
    for line in fragment.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith('#'):
            continue
        match = LIMINE_KEY.match(stripped)
        if not match:
            raise ValueError(f'Not a Limine setting: {stripped}')
        settings.append((match.group(1), stripped))
    keys = {key for key, _ in settings}
    lines = current.splitlines()
    end = next((index for index, line in enumerate(lines) if line.lstrip().startswith('/')), len(lines))
    kept = []
    # A previous look's block, from the marker to the first blank line, goes
    # whole: settings the new look does not set must not stay behind.
    previous = False
    for line in lines[:end]:
        if line.strip() == LIMINE_MARKER:
            previous = True
            continue
        if previous and line.strip():
            continue
        previous = False
        match = LIMINE_KEY.match(line)
        if match and match.group(1) in keys:
            continue
        kept.append(line)
    while kept and not kept[-1].strip():
        kept.pop()
    block = ([''] if kept else []) + [LIMINE_MARKER] + [line for _, line in settings]
    entries = lines[end:]
    if entries:
        block.append('')
    return '\n'.join(kept + block + entries).rstrip('\n') + '\n'


def render_limine_cmdline(text, action, words):
    # The entry tool reads KERNEL_CMDLINE[default] from /etc/default/limine.
    matches = list(LIMINE_CMDLINE.finditer(text))
    if not matches:
        raise ValueError('No KERNEL_CMDLINE[default] line in /etc/default/limine; add one with the current kernel parameters first.')
    match = matches[-1]
    try:
        current = ' '.join(shlex.split(match.group(2), comments=True)).split()
    except ValueError as error:
        raise ValueError(f'Cannot read KERNEL_CMDLINE[default]: {error}')
    updated = update_words(current, action, words)
    if any('"' in word for word in updated):
        raise ValueError('Kernel parameters cannot contain quotes.')
    return text[:match.start()] + match.group(1) + '"' + ' '.join(updated) + '"' + text[match.end():]


def render_mkinitcpio_hooks(text, action):
    # Plymouth must run right after udev or systemd in the initramfs, before
    # hooks such as encrypt that may ask for input.
    pattern = re.compile(r'^([ \t]*HOOKS=)(?:\((.*?)\)|"(.*?)"|\'(.*?)\'|(\S*))[ \t]*$', re.M)
    matches = list(pattern.finditer(text))
    if not matches:
        raise ValueError('mkinitcpio.conf has no HOOKS line.')
    match = matches[-1]
    hooks = next(group for group in match.groups()[1:] if group is not None).split()
    if action == 'add':
        if 'plymouth' not in hooks:
            for anchor in ('systemd', 'udev', 'base'):
                if anchor in hooks:
                    hooks.insert(hooks.index(anchor) + 1, 'plymouth')
                    break
            else:
                hooks.insert(0, 'plymouth')
    elif action == 'remove':
        hooks = [hook for hook in hooks if hook != 'plymouth']
    else:
        raise ValueError('Unknown hook action.')
    return text[:match.start()] + match.group(1) + '(' + ' '.join(hooks) + ')' + text[match.end():]


def render_sddm(text):
    parser = configparser.ConfigParser(interpolation=None, strict=False)
    parser.optionxform = str
    parser.read_string(text)
    for section in ('Theme', 'General'):
        if not parser.has_section(section):
            parser.add_section(section)
    parser['Theme']['Current'] = 'silent'
    parser['General']['InputMethod'] = 'qtvirtualkeyboard'
    # Preserve unrelated greeter variables such as renderer settings.
    env = parser['General'].get('GreeterEnvironment', '').split(',')
    replacements = {
        'QML2_IMPORT_PATH': '/usr/share/sddm/themes/silent/components/',
        'QT_IM_MODULE': 'qtvirtualkeyboard', 'QML_XHR_ALLOW_FILE_READ': '1',
    }
    env = [item for item in env if item and item.split('=', 1)[0] not in replacements]
    env.extend(f'{key}={value}' for key, value in replacements.items())
    parser['General']['GreeterEnvironment'] = ','.join(env)
    output = io.StringIO()
    parser.write(output, space_around_delimiters=False)
    return output.getvalue()


def load_palette(repo):
    # rice.json names the palette in use; its JSON file holds every color.
    name = 'eldritch'
    selection = repo / 'home-manager/rice.json'
    if selection.is_file():
        name = json.loads(selection.read_text()).get('theme', name)
    if not re.fullmatch(r'[a-z0-9-]+', name):
        raise ValueError(f'Unexpected palette name: {name}')
    return json.loads((repo / 'configs/themes' / f'{name}.json').read_text()), name


def render_ghostty_theme(palette, name):
    terminal, colors = palette['terminal'], palette['colors']
    lines = [f'# Generated from configs/themes/{name}.json by Commander Toolbox.']
    lines += [f'palette = {index}={color}' for index, color in enumerate(terminal['ansi'])]
    lines += [f'background = {colors["base"]}', f'foreground = {colors["text"]}',
              f'cursor-color = {terminal["cursor"]}', f'cursor-text = {terminal["cursorText"]}',
              f'selection-background = {terminal["selection"]}',
              f'selection-foreground = {terminal["selectionText"]}']
    return '\n'.join(lines) + '\n'


def render_ghostty_css(palette, name):
    return (f'/* Generated from configs/themes/{name}.json: opaque tab/title bar; the terminal keeps its transparency. */\n'
            'headerbar,\ntoolbarview > .top-bar {\n'
            f'    background-color: {palette["colors"]["mantle"]};\n'
            '    background-image: none;\n}\n')


def ghostty_targets(repo, source):
    palette, name = load_palette(repo)
    return {'config': source / 'config', 'spotatui.conf': source / 'spotatui.conf',
            'themes/commander': render_ghostty_theme(palette, name),
            'topbar.css': render_ghostty_css(palette, name)}


def install_dotfiles(repo, component, config, data):
    source = repo / 'configs' / component
    if component == 'konsole':
        for name in ('Garuda.profile', 'Sweet.colorscheme'):
            check_destination(data / 'konsole' / name)
        for name in ('Garuda.profile', 'Sweet.colorscheme'):
            deploy(source / name, data / 'konsole' / name)
        print('Select the Garuda profile in Konsole. Its font must be installed separately.')
    elif component == 'starship':
        deploy(source / 'starship.toml', config / 'starship.toml')
    elif component == 'ghostty':
        targets = ghostty_targets(repo, source)
        for target in targets:
            check_destination(config / 'ghostty' / target)
        for target, content in targets.items():
            if isinstance(content, Path):
                deploy(content, config / 'ghostty' / target)
            else:
                deploy(None, config / 'ghostty' / target, content)
        if (config / 'ghostty/config.ghostty').exists():
            print('Ghostty also reads config.ghostty; merge or remove it so the preset applies.')
        print('The preset uses MesloLGS Nerd Font; install it, or change font-family in the config, and reload with Ctrl+Shift+,.')
    elif component == 'nvim':
        deploy(source, config / component)
    elif component == 'fastfetch':
        install_fastfetch(source, config)
    else:
        raise ValueError('Unsupported dotfiles component.')



def fastfetch_shell_config(home, config, shell, launcher):
    target = nala_alias_target(home, config, shell)
    if shell == 'fish':
        target = config / 'fish/conf.d/zz-commander-fastfetch.fish'
    check_destination(target)
    text = target.read_text() if target.exists() else ''
    start = '# BEGIN Commander Linutil Fastfetch PNG'
    end = '# END Commander Linutil Fastfetch PNG'
    if text.count(start) != text.count(end) or text.count(start) > 1:
        raise ValueError('Malformed Commander Fastfetch block.')
    if start in text:
        if text.index(end) < text.index(start):
            raise ValueError('Malformed Commander Fastfetch block.')
        text = text[:text.index(start)] + text[text.index(end) + len(end):].lstrip('\n')
    command = shlex.quote(str(launcher))
    if shell == 'fish':
        block = f'function fastfetch --wraps fastfetch\n    command {command} $argv\nend'
    else:
        block = 'alias fastfetch=' + shlex.quote(command)
    return target, text.rstrip() + '\n\n' + start + '\n' + block + '\n' + end + '\n'


def fastfetch_settings(text):
    settings = json.loads(text)
    # Keep the layout; the launcher chooses the distro PNG at runtime.
    settings['logo'] = {'type': 'none', 'width': 24}
    return json.dumps(settings, ensure_ascii=False, indent=4) + '\n'


def install_fastfetch(source, config):
    destination = config / 'fastfetch'
    launcher = Path.home() / '.local/bin/fastfetch'
    check_destination(destination)
    check_destination(launcher)
    shell = Path(os.environ.get('SHELL', '/bin/bash')).name
    shell_config, shell_text = fastfetch_shell_config(Path.home(), config, shell, launcher)
    check_destination(shell_config)
    resources = Path(__file__).resolve().parent
    # Prepare the entire config before replacing an existing installation.
    with tempfile.TemporaryDirectory(prefix='commander-fastfetch-') as directory:
        stage = Path(directory) / 'fastfetch'
        shutil.copytree(source, stage)
        logos = stage / 'commander-logos'
        shutil.copytree(resources / 'fastfetch-logos', logos)
        shutil.copy2(source / 'visuals/dr460nized-fastfetch.png', logos / 'garuda.png')
        path = stage / 'config.jsonc'
        path.write_text(fastfetch_settings(path.read_text()))
        deploy(stage, destination)
    deploy(resources / 'fastfetch-distro.py', launcher)
    launcher.chmod(0o755)
    if not shell_config.exists() or shell_config.read_text() != shell_text:
        deploy(None, shell_config, shell_text)
    print('Installed Fastfetch PNG launcher: ' + str(launcher))
    print('Open a new terminal to activate the fastfetch shell shortcut.')
    print('PNG logos: Kitty/Ghostty, Konsole/WezTerm/iTerm, or Sixel terminals. Other terminals show details without a logo.')


def files_under(folder):
    return {path.relative_to(folder).as_posix(): path
            for path in sorted(folder.rglob('*')) if path.is_file()}


def dotfiles_targets(repo, component, config, data):
    """What install_dotfiles writes: destination -> a file, a folder of
    files (relative path -> file or text) or text."""
    source = repo / 'configs' / component
    if component == 'konsole':
        return {data / 'konsole' / name: source / name for name in ('Garuda.profile', 'Sweet.colorscheme')}
    if component == 'starship':
        return {config / 'starship.toml': source / 'starship.toml'}
    if component == 'ghostty':
        return {config / 'ghostty' / target: content
                for target, content in ghostty_targets(repo, source).items()}
    if component == 'nvim':
        return {config / 'nvim': files_under(source)}
    if component == 'fastfetch':
        resources = Path(__file__).resolve().parent
        folder = files_under(source)
        folder.update({'commander-logos/' + name: path
                       for name, path in files_under(resources / 'fastfetch-logos').items()})
        folder['commander-logos/garuda.png'] = source / 'visuals/dr460nized-fastfetch.png'
        folder['config.jsonc'] = fastfetch_settings((source / 'config.jsonc').read_text())
        launcher = Path.home() / '.local/bin/fastfetch'
        shell = Path(os.environ.get('SHELL', '/bin/bash')).name
        shell_config, shell_text = fastfetch_shell_config(Path.home(), config, shell, launcher)
        targets = {config / 'fastfetch': folder, launcher: resources / 'fastfetch-distro.py'}
        if not shell_config.exists() or shell_config.read_text() != shell_text:
            targets[shell_config] = shell_text
        return targets
    raise ValueError('Unsupported dotfiles component.')


def preview_dotfiles(repo, component, config, data):
    """What installing a dotfiles component would change, file by file,
    without writing anything: each file's state and its new text (None when
    it is not text). Files only in a replaced folder stay in its backup."""
    files, folders = [], []

    def compare(destination, content):
        new = content.encode() if isinstance(content, str) else content.read_bytes()
        try:
            text = new.decode()
        except UnicodeDecodeError:
            text = None
        if not destination.is_file():
            state = 'new'
        else:
            state = 'same' if destination.read_bytes() == new else 'changed'
        files.append({'path': str(destination), 'state': state, 'text': text})

    for destination, content in dotfiles_targets(repo, component, config, data).items():
        check_destination(destination)
        if isinstance(content, dict):
            folders.append(str(destination))
            for name, inner in content.items():
                compare(destination / name, inner)
            if destination.is_dir():
                for name in files_under(destination):
                    if name not in content:
                        files.append({'path': str(destination / name), 'state': 'removed', 'text': None})
        else:
            compare(destination, content)
    return {'files': files, 'folders': folders}


NALA_START = '# BEGIN Commander Linutil Nala alias'
NALA_END = '# END Commander Linutil Nala alias'


def nala_alias_target(home, config, shell):
    if shell == 'bash':
        return home / '.bashrc'
    if shell == 'zsh':
        return Path(os.environ.get('ZDOTDIR', str(home))) / '.zshrc'
    if shell == 'fish':
        return config / 'fish/conf.d/zz-commander-nala.fish'
    raise ValueError('Nala alias setup supports Bash, Zsh and Fish login shells only.')


def render_nala_alias(text, shell):
    if text.count(NALA_START) != text.count(NALA_END) or text.count(NALA_START) > 1:
        raise ValueError('Malformed Commander Nala alias block; check the shell configuration.')
    if NALA_START in text:
        start, end = text.index(NALA_START), text.index(NALA_END)
        if end < start:
            raise ValueError('Malformed Commander Nala alias block.')
        text = text[:start] + text[end + len(NALA_END):].lstrip('\n')
    if shell == 'fish':
        block = "if status is-interactive; and command -q nala\n    alias apt 'nala'\nend"
    else:
        block = "case $- in\n    *i*) if command -v nala >/dev/null 2>&1; then alias apt='nala'; fi ;;\nesac"
    return text.rstrip() + '\n\n' + NALA_START + '\n' + block + '\n' + NALA_END + '\n'


def main():
    mode, path, *args = sys.argv[1:]
    source = Path(path)
    config = Path(os.environ.get('XDG_CONFIG_HOME', str(Path.home() / '.config')))
    data = Path(os.environ.get('XDG_DATA_HOME', str(Path.home() / '.local/share')))
    if mode in ('nala-check', 'nala-alias'):
        shell = args[0]
        target = nala_alias_target(source, config, shell)
        check_destination(target)
        old = target.read_text() if target.exists() else ''
        rendered = render_nala_alias(old, shell)
        if mode == 'nala-alias' and rendered != old:
            deploy(None, target, rendered)
        elif mode == 'nala-check':
            print(f'Alias destination: {target}')
    elif mode == 'dotfiles':
        install_dotfiles(source, args[0], config, data)
    elif mode == 'dotfiles-preview':
        print(json.dumps(preview_dotfiles(source, args[0], config, data)))
    elif mode == 'topgrade':
        template = source / 'configs/topgrade/topgrade.toml'
        deploy(template, config / 'topgrade.toml', render_topgrade(template.read_text(), args[0]))
    elif mode == 'grub':
        print(render_grub(source.read_text(), *args), end='')
    elif mode == 'grub-prepare':
        prepare_grub_theme(source, Path(args[0]), args[1], args[2])
    elif mode == 'sddm':
        print(render_sddm(source.read_text()), end='')
    elif mode == 'grub-cmdline':
        print(render_kernel_cmdline(source.read_text(), args[0], args[1:]), end='')
    elif mode == 'mkinitcpio-hooks':
        print(render_mkinitcpio_hooks(source.read_text(), args[0]), end='')
    elif mode == 'limine-appearance':
        print(render_limine_appearance(source.read_text(), Path(args[0]).read_text()), end='')
    elif mode == 'limine-cmdline':
        print(render_limine_cmdline(source.read_text(), args[0], args[1:]), end='')
    else:
        raise ValueError('Unknown configuration action.')


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError) as error:
        sys.exit(str(error))
