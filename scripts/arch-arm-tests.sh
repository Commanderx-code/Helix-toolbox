#!/bin/bash -e

# Helix Toolbox in Arch Linux ARM, which has no image on Docker Hub: the
# Distributions workflow imports its own root file system, checked against
# its build key, and runs this in it as root from the repository, with the
# aarch64 program in bin/ and its packages in packages/.
# usage: scripts/arch-arm-tests.sh RECORD

record=${1:?usage: scripts/arch-arm-tests.sh RECORD}

# Its keyring, then the tools the tests need. The kernel and firmware are
# not upgraded: nothing boots here. pacman's download sandbox needs system
# calls a container does not allow.
sed -i '/^\[options\]/a DisableSandbox' /etc/pacman.conf
pacman-key --init
pacman-key --populate archlinuxarm
pacman -Syu --noconfirm --needed --ignore linux-aarch64,linux-firmware sudo git python which

chmod +x bin/helix-toolbox
bin/helix-toolbox system | tee system.txt
grep -q 'Package manager: .*(Arch family)' system.txt

# The catalog on this distribution, without the entries for x86_64 only.
bin/helix-toolbox list > visible.txt
bin/helix-toolbox list --all > all.txt
visible=$(grep -vE '/$' visible.txt | grep -cE '^\s')
total=$(grep -vE '/$' all.txt | grep -cE '^\s')
echo "Visible entries: $visible of $total"
test "$total" -ge 230
test "$visible" -ge 100
grep -q '^    Arch/$' visible.txt
for id in steam install-chaotic-aur-repository install-cachyos-repository-and-kernel proton-cachyos; do
    if grep -q "^ *$id " visible.txt; then
        echo "$id, for x86_64 only, shows on Arch Linux ARM" >&2
        exit 1
    fi
    grep -q "^ *$id " all.txt
done
bin/helix-toolbox plan steam | grep -q '^Blocked: Steam is for x86_64 only, and this machine is aarch64$'

# Real package installs through the engine, verified by pacman.
bin/helix-toolbox plan htop
yes | bin/helix-toolbox run htop ripgrep --yes | tee run.txt
command -v htop
command -v rg
grep -q '^Verified: htop: pacman lists htop' run.txt
grep -q '^Verified: Ripgrep: pacman lists ripgrep' run.txt
bin/helix-toolbox history --limit 1 | grep -q 'ok .*exit 0, verified'
bin/helix-toolbox status htop ripgrep | tee status.txt
grep -qE '^htop +Installed +pacman [0-9]' status.txt
bin/helix-toolbox run system-report machine-summary --yes | grep -q '^Distribution:'

# The installer tests under this distribution's shell.
python3 -B -m unittest discover -s tests/installers -t tests/installers

# The Arch package installs, works and removes cleanly.
pacman -U --noconfirm packages/helix-toolbox-*-aarch64.pkg.tar.zst
test "$(readlink -f "$(command -v helix-toolbox)")" = /usr/bin/helix-toolbox
helix-toolbox --version | grep -qE '^helix-toolbox [0-9]+\.[0-9]+\.[0-9]+$'
helix-toolbox system | grep -q '(Arch family)'
helix-toolbox report --no-checks | grep -q '^- Installed from: package$'
pacman -Ql helix-toolbox > files.txt
for file in /usr/bin/helix-toolbox /usr/share/man/man1/helix-toolbox.1.gz \
    /usr/share/bash-completion/completions/helix-toolbox \
    /usr/share/applications/helix-toolbox.desktop; do
    grep -qE "^helix-toolbox $file\$" files.txt
done
pacman -R --noconfirm helix-toolbox
hash -r
test ! -e /usr/bin/helix-toolbox

# What Helix Toolbox verified here, for the record.
bin/helix-toolbox history --json --limit 1000 > history.json
python3 -B scripts/validation-record.py record 'archlinuxarm on aarch64' history.json catalog > "$record"
cat "$record"
