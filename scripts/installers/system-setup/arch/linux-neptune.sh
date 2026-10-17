#!/bin/sh -e

. ../../common-script.sh

setUpRepos() {
    if ! grep -q "^\s*\[jupiter-staging\]" /etc/pacman.conf; then
        printf "%b\n" "${CYAN}Adding jupiter-staging to pacman repositories...${RC}"
        echo "[jupiter-staging]" | "$ESCALATION_TOOL" tee -a /etc/pacman.conf
        echo "Server = https://steamdeck-packages.steamos.cloud/archlinux-mirror/\$repo/os/\$arch" | "$ESCALATION_TOOL" tee -a /etc/pacman.conf
        echo "SigLevel = Never" | "$ESCALATION_TOOL" tee -a /etc/pacman.conf
    fi
    if ! grep -q "^\s*\[holo-staging\]" /etc/pacman.conf; then
        printf "%b\n" "${CYAN}Adding holo-staging to pacman repositories...${RC}"
        echo "[holo-staging]" | "$ESCALATION_TOOL" tee -a /etc/pacman.conf
        echo "Server = https://steamdeck-packages.steamos.cloud/archlinux-mirror/\$repo/os/\$arch" | "$ESCALATION_TOOL" tee -a /etc/pacman.conf
        echo "SigLevel = Never" | "$ESCALATION_TOOL" tee -a /etc/pacman.conf
    fi
}

# The newest Neptune kernel that is not a release candidate. Plain
# linux-neptune is Valve's 5.13 from 2021: it cannot mount the btrfs file
# systems made today, and GRUB lists it first, so the machine did not boot.
latestKernel() {
    "$PACKAGER" -Sl jupiter-staging | awk '$2 ~ /^linux-neptune-[0-9]+$/ && $3 !~ /rc/ { print $3, $2 }' \
        | sort -V | tail -n 1 | cut -d ' ' -f 2
}

installKernel() {
    if ! "$PACKAGER" -Q | grep -q "\blinux-neptune"; then
        "$ESCALATION_TOOL" "$PACKAGER" -Syyu --noconfirm
        kernel=$(latestKernel)
        if [ -z "$kernel" ]; then
            printf "%b\n" "${RED}jupiter-staging offers no linux-neptune kernel.${RC}"
            exit 1
        fi
        printf "%b\n" "${CYAN}Installing $kernel...${RC}"
        "$ESCALATION_TOOL" "$PACKAGER" -S --noconfirm "$kernel" "$kernel-headers" steamdeck-dsp jupiter-staging/alsa-ucm-conf
        "$ESCALATION_TOOL" mkinitcpio -P
    else
        printf "%b\n" "${GREEN}linux-neptune detected. Skipping installation.${RC}"
    fi

    if [ -f /etc/default/grub ]; then
        printf "%b\n" "${CYAN}Updating GRUB...${RC}"
    if ! grep -q '^UPDATEDEFAULT=' /etc/default/grub; then
        echo 'UPDATEDEFAULT=yes' | "$ESCALATION_TOOL" tee -a /etc/default/grub
    else
        "$ESCALATION_TOOL" sed -i 's/^UPDATEDEFAULT=.*/UPDATEDEFAULT=yes/' /etc/default/grub
    fi
    if [ -f /boot/grub/grub.cfg ]; then
        "$ESCALATION_TOOL" grub-mkconfig -o /boot/grub/grub.cfg
    else
        printf "%b\n" "${RED}GRUB configuration file not found. Run grub-mkconfig manually.${RC}"
    fi
    else
        printf "%b\n" "${RED}GRUB not detected. Manually set your bootloader to use linux-neptune.${RC}"
    fi
}

# The speaker amplifier's firmware. Valve's old firmware package put it at
# the top of the firmware folder, where the driver does not look; that
# package is gone, and linux-firmware-cirrus has it in cirrus/ already.
copyFirmwareFiles() {
    if [ -f /usr/lib/firmware/cs35l41-dsp1-spk-prot.bin ]; then
        printf "%b\n" "${CYAN}Copying firmware files...${RC}"
        "$ESCALATION_TOOL" mkdir -p /usr/lib/firmware/cirrus
        for file in cs35l41-dsp1-spk-cali.bin cs35l41-dsp1-spk-cali.wmfw cs35l41-dsp1-spk-prot.bin cs35l41-dsp1-spk-prot.wmfw; do
            if [ -f "/usr/lib/firmware/$file" ]; then
                "$ESCALATION_TOOL" cp "/usr/lib/firmware/$file" /usr/lib/firmware/cirrus/
            fi
        done
    elif ! ls /usr/lib/firmware/cirrus/cs35l41-dsp1-spk-prot.* >/dev/null 2>&1; then
        printf "%b\n" "${CYAN}Installing the speaker firmware...${RC}"
        "$ESCALATION_TOOL" "$PACKAGER" -S --needed --noconfirm linux-firmware-cirrus
    fi
}

checkEnv
checkEscalationTool
setUpRepos
installKernel
copyFirmwareFiles
