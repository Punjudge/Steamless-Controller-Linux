# Arch Linux / CachyOS packages (AUR)

CachyOS uses Arch packages, and programs like this one are shared through the
**AUR** (Arch User Repository). CachyOS's own repositories are curated by the
CachyOS team; a package usually reaches them only after it is on the AUR and
someone asks for it.

Two packages live here, each folder ready to be its own AUR repository:

| Folder | What it does |
| --- | --- |
| `steamless-controller` | Builds from the tagged source on GitHub (needs `cargo`). Runs the tests. |
| `steamless-controller-bin` | Repackages the ready-built `.tar.gz` from GitHub Releases. Fast, no compiler. |

Both install `/usr/bin/steamless-controller`, the menu entry, the docs, the
licence and a udev rule (`70-steamless-controller.rules`, the same as
`../udev/`), so the controller and `/dev/uinput` work for the person at the
seat **without Steam or steam-devices**.

## Without the AUR

AUR sign-ups are sometimes closed. Until there is an account, a package can
still reach people:

* **From the release (signed repository)**: the `pacman` job in
  `.github/workflows/release.yml` builds
  `steamless-controller-bin-X.Y.Z-1-x86_64.pkg.tar.zst`, a copy under the fixed
  name `steamless-controller-bin-x86_64.pkg.tar.zst`, signs both, and with
  `../arch/sign-repo.sh` makes the repository files `steamless-controller.db`
  and `.files` (each also as `.tar.gz`, all signed). Everything is attached to
  the release, so `.../releases/latest/download` works as a pacman `Server`. The
  main README has the lines for people to trust the key and add the repository.
  * The signing key's public half is `../arch/steamless-controller.asc`
    (fingerprint `FC915DEADAFCF2D6E4C547FA50BE4488E96801E4`). The secret half
    is the repository secret `PACMAN_SIGNING_KEY` in GitHub Actions, with a
    copy only on the maintainer's computer.
  * New key (lost or leaked): make one, replace the `.asc` file and the secret,
    change the fingerprint in the main README, and tell people to
    `pacman-key --lsign-key` the new one (and `pacman-key --delete` the old).
    `sign-repo.sh` refuses to sign when the secret does not match the `.asc`.
* **By hand**: `packaging/arch/make-pkg.sh <release.tar.gz> <version> <dir>`
  builds the same package from a release tarball.
* **From this folder**: copy `steamless-controller` or
  `steamless-controller-bin` to an empty directory and run `makepkg -si`.

## Try it locally

Copy one folder to an empty directory and run:

```sh
makepkg -si        # build and install
pacman -R steamless-controller   # (or steamless-controller-bin) to remove
```

Plug the controller in again after the first install so the udev rule applies.

## Publish on the AUR (once)

1. Create an account on <https://aur.archlinux.org> and add your SSH public key
   under *My Account*.
2. For each package:

   ```sh
   git clone ssh://aur@aur.archlinux.org/steamless-controller-bin.git ~/aur/steamless-controller-bin
   cp packaging/aur/steamless-controller-bin/{PKGBUILD,.SRCINFO,steamless-controller.install,70-steamless-controller.rules} \
      ~/aur/steamless-controller-bin/
   cd ~/aur/steamless-controller-bin
   git add PKGBUILD .SRCINFO steamless-controller.install 70-steamless-controller.rules
   git commit -m "Initial upload: 1.25.0"
   git push
   ```

   (Cloning a name nobody has taken yet gives an empty repository; the first
   push creates the package.)

Afterwards anyone can install with `paru -S steamless-controller-bin` or
`yay -S steamless-controller-bin`.

## New version

1. Tag and release as described in the main README (`vX.Y.Z`).
2. In both PKGBUILDs set `pkgver=X.Y.Z`, `pkgrel=1`, and update
   `sha256sums` (`updpkgsums` from `pacman-contrib`, or `sha256sum` on the
   downloaded files; the `-bin` one is in the release's `.sha256` file).
3. `makepkg --printsrcinfo > .SRCINFO`, build once with `makepkg`, then commit
   and push to each AUR repository.

If the udev rule changes, copy `../udev/70-steamless-controller.rules` into
both folders again and update its checksum.
