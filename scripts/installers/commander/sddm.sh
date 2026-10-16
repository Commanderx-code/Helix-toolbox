#!/bin/sh -e
# shellcheck source=scripts/installers/commander/common.sh
. ./common.sh
commander_init
checkCommandRequirements 'sddm sddm-greeter-qt6 python3'
if [ -z "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]; then
    printf '%s\n' 'Run this installer from a graphical desktop session for the theme preview.' >&2
    exit 1
fi
# The package manager knows the version; sddm --version crashes outside a
# graphical session with SDDM 0.21 and Qt 6.12, so it is only the fallback.
case "$PACKAGER" in
    pacman) sddm_version=$(pacman -Q sddm 2>/dev/null || true) ;;
    apt-get | nala) sddm_version=$(dpkg-query -W -f='${Version}' sddm 2>/dev/null || true) ;;
    dnf) sddm_version=$(rpm -q --qf '%{VERSION}' sddm 2>/dev/null || true) ;;
    *) sddm_version='' ;;
esac
[ -n "$sddm_version" ] || sddm_version=$(sddm --version 2>/dev/null || true)
python3 - "$sddm_version" <<'PY'
import re, sys
version = re.search(r'(\d+)\.(\d+)\.(\d+)', sys.argv[1])
if not version or tuple(map(int, version.groups())) < (0, 21, 0):
    sys.exit('SilentSDDM requires SDDM 0.21.0 or newer.')
PY
# SilentSDDM at its upstream location, which the dotfiles snippets point at.
theme=/usr/share/sddm/themes/silent
config=/etc/sddm.conf.d/zz-commander-theme.conf
fonts=/usr/local/share/fonts/silent-sddm
# A package may own the theme already (the AUR sddm-silent-theme packages);
# then only the dotfiles presets go on top, as restore-sddm does.
if [ "$PACKAGER" = pacman ] && pacman -Qo "$theme" >/dev/null 2>&1; then
    theme_origin="the installed $(pacman -Qoq "$theme" 2>/dev/null | head -n 1) package"
else
    theme_origin='SilentSDDM 1.5.1 from its pinned source'
fi
# Only a link names a display manager: readlink -f returns the path itself
# when there is none, which would be taken for one.
display_manager=''
if [ -L /etc/systemd/system/display-manager.service ]; then
    display_manager=$(basename "$(readlink -f /etc/systemd/system/display-manager.service)")
fi
switch_note=''
if [ "$display_manager" != sddm.service ]; then
    if [ -n "$display_manager" ]; then
        switch_note=" Your login screen is run by $display_manager, which ignores SDDM themes; after the preview you can switch it to SDDM."
    else
        switch_note=' No login screen service is enabled; after the preview you can enable SDDM.'
    fi
fi
confirm "Install $theme_origin with the Kratos login preset and the other presets, backgrounds and settings from dotfiles, plus Qt6 dependencies and fonts. The existing theme and configuration are backed up; the current session is not restarted.$switch_note"
command_exists git || install_packages git
case "$PACKAGER" in
    pacman) install_packages qt6-svg qt6-virtualkeyboard qt6-multimedia qt6-multimedia-ffmpeg qt6-declarative qt6-imageformats ;;
    dnf) install_packages qt6-qtsvg qt6-qtvirtualkeyboard qt6-qtmultimedia qt6-qtdeclarative qt6-qtimageformats ;;
    apt-get | nala) install_packages libqt6svg6 qml6-module-qtquick qml6-module-qtquick-controls qml6-module-qtquick-layouts qml6-module-qtquick-window qml6-module-qtquick-templates qml6-module-qtqml-workerscript qml6-module-qtquick-effects qml6-module-qtquick-virtualkeyboard qml6-module-qtmultimedia qt6-image-formats-plugins ;;
esac
fetch_source dotfiles
presets="$source_dir/sddm"
[ -f "$presets/configs/kratos.conf" ] || { printf '%s\n' 'The dotfiles source has no SDDM presets.' >&2; exit 1; }
backup_system "$theme"
case "$theme_origin" in
    SilentSDDM*)
        fetch_source silentsddm
        "$ESCALATION_TOOL" mkdir -p "$theme"
        # Copies belong to root with default permissions, not to the user who
        # fetched the source; the greeter reads them as a system theme.
        for item in Main.qml qmldir metadata.desktop LICENSE test.sh change_avatar.sh backgrounds components configs fonts icons; do
            "$ESCALATION_TOOL" cp -R --no-preserve=all -- "$source_dir/$item" "$theme/"
        done
        # The presets name the Red Hat fonts the theme ships.
        backup_system "$fonts"
        "$ESCALATION_TOOL" mkdir -p "$fonts"
        "$ESCALATION_TOOL" cp -R --no-preserve=all -- "$source_dir/fonts/redhat" "$source_dir/fonts/redhat-vf" "$fonts/"
        if command_exists fc-cache; then "$ESCALATION_TOOL" fc-cache -f "$fonts"; fi
        ;;
esac
# The dotfiles presets on top: the same files restore-sddm installs.
"$ESCALATION_TOOL" mkdir -p "$theme/configs" "$theme/backgrounds" "$theme/components"
for file in "$presets"/configs/*.conf "$presets"/backgrounds/*; do
    [ -f "$file" ] || continue
    "$ESCALATION_TOOL" install -m 644 -- "$file" "$theme/$(basename "$(dirname "$file")")/$(basename "$file")"
done
for file in BatteryIndicator.qml LoginScreen.qml; do
    [ -f "$presets/components/$file" ] && "$ESCALATION_TOOL" install -m 644 -- "$presets/components/$file" "$theme/components/$file"
done
"$ESCALATION_TOOL" install -m 644 -- "$presets/metadata.desktop" "$theme/metadata.desktop"
# The theme starts its background video with a play() call that Qt 6.12 ignores
# while the file is still loading; letting the player start by itself avoids that.
"$ESCALATION_TOOL" sed -i 's/^\( *\)autoPlay: false/\1autoPlay: true/' "$theme/Main.qml"
printf '%s\n' 'Preview the theme before enabling it. Close the preview window to continue.'
QML2_IMPORT_PATH="$theme/components" QT_IM_MODULE=qtvirtualkeyboard QML_XHR_ALLOW_FILE_READ=1 sddm-greeter-qt6 --test-mode --theme "$theme"
confirm 'If the preview rendered correctly, enable this theme for the next login.'
rendered=$(mktemp)
trap 'rm -f "$rendered"' 0
"$ESCALATION_TOOL" mkdir -p /etc/sddm.conf.d
# Num Lock on and the greeter environment, as dotfiles keeps them, then the
# theme choice in a late drop-in that wins over distro files such as kde_settings.conf.
for file in "$presets"/sddm-conf/10-numlock.conf "$presets"/sddm-conf/20-silent.conf; do
    [ -f "$file" ] || continue
    backup_system "/etc/sddm.conf.d/$(basename "$file")"
    "$ESCALATION_TOOL" install -m 644 -- "$file" "/etc/sddm.conf.d/$(basename "$file")"
done
backup_system "$config"
printf '%s\n' '[Theme]' 'Current=silent' > "$rendered"
"$ESCALATION_TOOL" install -m 644 "$rendered" "$config"
if [ -f /etc/sddm.conf ]; then
    backup_system /etc/sddm.conf
    python3 ./configure.py sddm /etc/sddm.conf > "$rendered"
    "$ESCALATION_TOOL" install -m 644 "$rendered" /etc/sddm.conf
fi
printf '%s\n' 'Theme enabled for the next login. No service was restarted.'
if [ "$display_manager" != sddm.service ]; then
    if [ -n "$display_manager" ]; then
        printf 'Switch the login screen from %s to SDDM at the next reboot? %s stays installed but disabled. [y/N] ' "$display_manager" "$display_manager"
    else
        printf 'Enable SDDM as the login screen at the next reboot? [y/N] '
    fi
    read -r answer
    case "$answer" in
        y | Y | yes | YES)
            if [ -n "$display_manager" ]; then
                "$ESCALATION_TOOL" systemctl disable "$display_manager"
            fi
            "$ESCALATION_TOOL" systemctl enable --force sddm.service
            printf '%s\n' 'SDDM runs the login screen from the next reboot.'
            ;;
        *) printf '%s\n' 'The theme shows once SDDM runs the login screen: systemctl enable --force sddm.service.' ;;
    esac
fi
