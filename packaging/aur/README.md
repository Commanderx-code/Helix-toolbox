# The AUR package

`helix-toolbox-bin` for the [AUR](https://aur.archlinux.org): a split
package that builds `helix-toolbox-bin` (the command line and terminal
interface) and `helix-toolbox-gui-bin` (the window). It downloads the
release's own Arch packages, checks them against the release's SHA-256 sums
and unpacks them as they are, so what it installs is the tested build from
the releases page, file for file.

```sh
yay -S helix-toolbox-bin helix-toolbox-gui-bin    # once it is on the AUR
```

Until then, or without an AUR helper, from a clone:

```sh
cd packaging/aur
makepkg -si    # builds and installs both packages
```

Helix Toolbox's Updates screen sees `helix-toolbox-bin` and updates it with
`paru` or `yay`, the window's package with it.

The [pacman repository](../../docs/INSTALL.md#the-arch-repository) installs
the same packages and keeps them updated with the system; this is for those
who would rather take them from the AUR.

## A new release

After a release is published:

```sh
packaging/aur/update.sh 1.25.19
```

That sets `pkgver`, resets `pkgrel` to 1, takes the four SHA-256 sums from
the release's `SHA256SUMS` and, where `makepkg` is installed, writes
`.SRCINFO`. Commit both files here, then copy them into the AUR's
`helix-toolbox-bin` git repository and push there.
