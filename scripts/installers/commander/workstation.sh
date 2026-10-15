#!/bin/sh -e
# shellcheck source=scripts/installers/commander/common.sh
. ./common.sh
commander_init
if [ "$PACKAGER" != pacman ]; then
    printf '%s\n' 'The complete workstation setup is for Arch-based systems such as Garuda; its script installs with pacman. Other systems take the pieces from the Dotfiles, Myfish, Grub Theme, Boot Splash and SDDM Theme entries.' >&2
    exit 1
fi
command_exists git || install_packages git
# The checkout stays yours: scripts/setup writes machine.json into it and
# hm-rebuild builds from it later, so it is a normal clone, not a pinned source.
checkout=${COMMANDER_DOTFILES_DIR:-$HOME/github/projects/dotfiles}
repository=https://github.com/Commanderx-code/dotfiles.git
if [ -d "$checkout/.git" ]; then
    [ -f "$checkout/scripts/setup" ] || { printf '%s is a Git checkout without scripts/setup; point COMMANDER_DOTFILES_DIR at a dotfiles checkout or move it aside.\n' "$checkout" >&2; exit 1; }
    plan="update the checkout at $checkout with git pull --ff-only"
elif [ -e "$checkout" ]; then
    printf '%s exists but is not a Git checkout; move it aside or set COMMANDER_DOTFILES_DIR.\n' "$checkout" >&2
    exit 1
else
    plan="clone $repository to $checkout"
fi
confirm "This will $plan, then start its guided setup (scripts/setup). The setup asks for your name and Git identity, whether to install Nix and Home Manager, your backup drive, and which parts to install (everyday apps, SilentSDDM login screen, the GRUB or Limine boot menu with AppArmor, the boot splash, the UFW firewall), shows its plan and waits for its own APPLY. It installs system packages with pacman and asks for your password when it needs it."
if [ -d "$checkout/.git" ]; then
    git -C "$checkout" pull --ff-only || { printf '%s\n' 'The checkout could not be fast-forwarded; commit or stash its changes, update it, then rerun.' >&2; exit 1; }
else
    mkdir -p "$(dirname "$checkout")"
    git clone "$repository" "$checkout"
fi
cd "$checkout"
exec bash scripts/setup
