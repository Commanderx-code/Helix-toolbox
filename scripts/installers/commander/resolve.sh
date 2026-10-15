#!/bin/sh -e
# shellcheck source=scripts/installers/commander/common.sh
. ./common.sh
mode=${1:-}
case "$mode" in native | container) ;; *) exit 1 ;; esac
checkDistro
case "$DTYPE" in
    fedora | arch | garuda | endeavouros | cachyos | manjaro) ;;
    *)
        case " $DTYPE ${ID_LIKE:-} " in
            *' debian '* | *' ubuntu '*)
                [ "$mode" = container ] || {
                    printf '%s\n' 'Use the Davincibox option on Debian/Ubuntu. Native setup currently supports Fedora/Arch only.' >&2
                    exit 1
                } ;;
            *) printf '%s\n' 'Unsupported distro: use Fedora, Arch, or Debian/Ubuntu-family Davincibox.' >&2; exit 1 ;;
        esac ;;
esac
[ "$(uname -m)" = x86_64 ] || { printf '%s\n' 'Resolve Linux requires x86_64.' >&2; exit 1; }
commander_init
[ -n "${DISPLAY:-}" ] || { printf '%s\n' 'Start from a desktop session with X11 or XWayland available.' >&2; exit 1; }
checkCommandRequirements 'python3'
python3 ./resolve-launcher.py --check
printf '%s\n' 'Download and extract the Linux Free or Studio installer from:' \
    'https://www.blackmagicdesign.com/products/davinciresolve' \
    'Studio requires your own license. GPU drivers must already work on the host.' \
    'No installer can guarantee support for every GPU or Linux codec.'
printf '%s' 'Full path to the extracted DaVinci_Resolve_*_Linux.run file (no quotes): '
read -r installer
installer=$(realpath -- "$installer")
if [ ! -f "$installer" ] || [ ! -r "$installer" ]; then
    printf '%s\n' 'Installer file not found.' >&2
    exit 1
fi
case "${installer##*/}" in DaVinci_Resolve_*_Linux.run) ;; *)
    printf '%s\n' 'Select the official extracted Linux .run installer, not the ZIP.' >&2; exit 1 ;;
esac
printf '%s\n' 'GPU: 1) NVIDIA proprietary driver  2) AMD ROCm  3) Intel OpenCL'
read -r gpu
case "$gpu" in
    1) if ! command_exists nvidia-smi || ! nvidia-smi >/dev/null; then
        printf '%s\n' 'Install a compatible NVIDIA host driver, reboot, and retry.' >&2; exit 1;
    fi ;;
    2 | 3) ;;
    *) printf '%s\n' 'Cancelled.'; exit 1 ;;
esac
if [ "$mode" = native ] && [ -e "/opt/resolve" ]; then
    printf '%s\n' 'An existing native Resolve installation was found. Back up projects and use its package manager/uninstaller before installing again.' >&2
    exit 1
fi
if [ "$mode" = container ] && command_exists podman && podman container exists commander-resolve; then
    printf '%s\n' 'The commander-resolve container already exists. It has been preserved; see the Resolve guide for maintenance.' >&2
    exit 1
fi
confirm "Install Resolve ($mode), dependencies and a backed-up user launcher. The supplied Blackmagic installer will run unattended with license acceptance; continue only if you have read and accept its license. Container builds download several GB."
case "$mode" in
    native)
        case "$PACKAGER" in
            dnf) install_packages alsa-lib apr apr-util dbus-libs fontconfig freetype glib2 gdk-pixbuf2 \
                bzip2-libs expat libX11 libXScrnSaver libXcomposite libXdamage libXext libXrender \
                libdrm libpng libuuid libxcb pango libglvnd-opengl libglvnd-egl libglvnd-glx libICE libSM libxcrypt-compat libXcursor libXfixes libXi \
                libXinerama libxkbcommon-x11 libXrandr libXtst libXxf86vm mesa-libGLU mtdev nss \
                ocl-icd pulseaudio-libs librsvg2 xcb-util xcb-util-cursor xcb-util-image \
                xcb-util-keysyms xcb-util-renderutil xcb-util-wm zlib-ng-compat clinfo ;;
            pacman) install_packages alsa-lib apr apr-util dbus fontconfig freetype2 glib2 gdk-pixbuf2 \
                bzip2 expat libx11 libxss libxcomposite libxdamage libxext libxrender \
                libdrm libpng util-linux-libs libxcb pango libglvnd libice libsm libxcrypt-compat libxcursor libxfixes libxi libxinerama \
                libxkbcommon-x11 libxrandr libxtst libxxf86vm glu mtdev nss ocl-icd libpulse \
                librsvg xcb-util xcb-util-cursor xcb-util-image xcb-util-keysyms xcb-util-renderutil \
                xcb-util-wm zlib clinfo ;;
            *) exit 1 ;;
        esac
        case "$gpu:$PACKAGER" in
            1:pacman) install_packages opencl-nvidia ;;
            1:dnf) install_packages 'libcuda.so.1()(64bit)' 'libnvidia-opencl.so.1()(64bit)' ;;
            2:dnf) install_packages rocm-opencl ;;
            2:pacman) install_packages rocm-opencl-runtime ;;
            3:*) install_packages intel-compute-runtime ;;
        esac
        devices=$(clinfo -l)
        printf '%s\n' "$devices"
        printf '%s\n' "$devices" | grep -q 'Device #' || {
            printf '%s\n' 'No OpenCL device found. Resolve needs a supported GPU and driver; installation stopped.' >&2
            exit 1
        }
        ;;
    container)
        case "$PACKAGER" in
            apt-get | nala)
                "$ESCALATION_TOOL" apt-get update
                install_packages podman distrobox git uidmap slirp4netns fuse-overlayfs
                if [ "$gpu" = 2 ]; then
                    if apt-cache show rocm-podman-support >/dev/null 2>&1; then
                        install_packages rocm-podman-support
                    else
                        printf '%s\n' 'ROCm container helper unavailable in these repositories. If GPU access fails, check /dev/kfd and render/video group access.'
                    fi
                fi ;;
            *) install_packages podman distrobox git ;;
        esac
        if [ "$gpu" = 1 ]; then
            if ! command_exists nvidia-ctk; then
                if [ "$PACKAGER" = dnf ] && [ ! -e /etc/yum.repos.d/nvidia-container-toolkit.repo ]; then
                    confirm 'Add the official NVIDIA Container Toolkit RPM repository and install its signed packages?'
                    repo_file=$(mktemp)
                    trap 'rm -f -- "$repo_file"' 0
                    curl -fLsS https://nvidia.github.io/libnvidia-container/stable/rpm/nvidia-container-toolkit.repo -o "$repo_file"
                    grep -q '^gpgcheck=1' "$repo_file"
                    "$ESCALATION_TOOL" install -m 644 "$repo_file" /etc/yum.repos.d/nvidia-container-toolkit.repo
                    rm -f -- "$repo_file"
                    trap - 0
                fi
                case "$PACKAGER" in
                    apt-get | nala)
                        # shellcheck source=scripts/installers/commander/resolve-apt.sh
                        . ./resolve-apt.sh
                        resolve_apt_nvidia ;;
                esac
                install_packages nvidia-container-toolkit
            fi
            nvidia-ctk cdi list | grep -q 'nvidia.com/gpu=' || {
                printf '%s\n' 'NVIDIA CDI devices are missing. Configure NVIDIA Container Toolkit CDI and retry.' >&2; exit 1;
            }
        fi
        fetch_source davincibox
        target=davincibox-opencl
        [ "$gpu" != 1 ] || target=davincibox
        podman build --target "$target" -t localhost/commander-resolve:reviewed "$source_dir"
        export DBX_CONTAINER_MANAGER=podman
        if [ "$gpu" = 1 ]; then
            distrobox create --yes --name commander-resolve --image localhost/commander-resolve:reviewed \
                --additional-flags '--device nvidia.com/gpu=all'
        else
            distrobox create --yes --name commander-resolve --image localhost/commander-resolve:reviewed
        fi
        ;;
esac
# Use a shared-home directory: host /tmp is not the container's /tmp.
work=$(mktemp -d "$HOME/.commander-resolve.XXXXXX")
trap 'rm -rf -- "$work"' 0
trap 'exit 130' INT
trap 'exit 143' TERM
cp -- "$installer" "$work/installer.run"
chmod u+x "$work/installer.run"
(cd "$work" && ./installer.run --appimage-extract >/dev/null)
[ -f "$work/squashfs-root/AppRun" ] || { printf '%s\n' 'Installer extraction failed.' >&2; exit 1; }
if [ "$mode" = native ]; then
    "$ESCALATION_TOOL" env QT_QPA_PLATFORM=minimal SKIP_PACKAGE_CHECK=1 "$work/squashfs-root/AppRun" -i -a -y
    [ -x "/opt/resolve/bin/resolve" ] || { printf '%s\n' 'Resolve installation did not complete.' >&2; exit 1; }
else
    distrobox enter --name commander-resolve -- sudo env QT_QPA_PLATFORM=minimal SKIP_PACKAGE_CHECK=1 \
        "$work/squashfs-root/AppRun" -i -a -y
    distrobox enter --name commander-resolve -- test -x "/opt/resolve/bin/resolve"
fi
python3 ./resolve-launcher.py --runtime > "$work/runtime.sh"
if [ "$mode" = native ]; then
    sh "$work/runtime.sh" --check
else
    distrobox enter --name commander-resolve -- sh "$work/runtime.sh" --check
fi
python3 ./resolve-launcher.py "$mode"
printf '%s\n' 'Installed. Launch DaVinci Resolve (Commander) from the application menu.' \
    'If GPU initialization fails, see docs/resolve.md. Host drivers, GPU support and codecs remain hardware/version dependent.'
