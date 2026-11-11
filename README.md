# Steamless Controller for Linux

Use the 2026 Steam Controller (wireless puck, cable or Bluetooth) as a normal
Xbox 360 or PlayStation-style gamepad, mouse and keyboard on Linux, **without
Steam running**. A Linux version of
[SteamlessController](https://github.com/ddeverill/SteamlessController) for
Windows, rewritten in Rust with an egui window and a system tray icon. Made and
tested on CachyOS with KDE Plasma (Wayland).

## Install

1. Download `steamless-controller-…-x86_64-linux.tar.gz` from
   [Releases](https://github.com/Punjudge/SteamlessController-Linux/releases).
2. Unpack it and run `./install.sh` in the unpacked folder. It puts the program
   in `~/.local/bin` and adds it to the application menu. `./install.sh --uninstall`
   removes it again.

Your login needs access to the controller (`/dev/hidraw*`) and to `/dev/uinput`.
Steam's device rules give you both: install `steam-devices` (or `steam`), then
plug the controller in again. Without Steam, install
`packaging/udev/70-steamless-controller.rules` to `/usr/lib/udev/rules.d/` instead.

**Arch Linux / CachyOS:** the releases are also a signed pacman repository,
so `pacman -Syu` keeps the program up to date like any other package.

1. Trust the signing key once (fingerprint
   `FC91 5DEA DAFC F2D6 E4C5  47FA 50BE 4488 E968 01E4`):

   ```sh
   curl -fL -o /tmp/steamless-controller.asc https://raw.githubusercontent.com/Punjudge/SteamlessController-Linux/main/packaging/arch/steamless-controller.asc
   sudo pacman-key --add /tmp/steamless-controller.asc
   sudo pacman-key --lsign-key FC915DEADAFCF2D6E4C547FA50BE4488E96801E4
   ```

2. Add the repository to the end of `/etc/pacman.conf`:

   ```ini
   [steamless-controller]
   Server = https://github.com/Punjudge/SteamlessController-Linux/releases/latest/download
   ```

3. Install: `sudo pacman -Syu steamless-controller-bin`

It installs the program, the menu entry and the device rule system-wide, and
`sudo pacman -R steamless-controller-bin` removes it. With the key trusted, one
package can also be installed without the repository:
`sudo pacman -U https://github.com/Punjudge/SteamlessController-Linux/releases/latest/download/steamless-controller-bin-x86_64.pkg.tar.zst`
(pacman fetches its `.sig` beside it). To build the package yourself, or once
it is on the AUR, see `packaging/aur/README.md`.

## What it does

* Takes over the trackpads and back buttons from the controller's built-in
  mouse/keyboard mode and does the same job itself, but configurable per game.
  Out of the box it still works like a mouse: the right trackpad moves the
  pointer (press to left-click), the left trackpad scrolls, and the upper back
  buttons left-click. When the app lets go (Steamless Mode off, or the app
  quits), the controller's own mouse mode comes back.
* Creates a virtual Xbox 360 pad (or a PlayStation-style pad with touchpad), plus
  a virtual mouse and keyboard, with rumble from games sent back to the controller.
* Per-game profiles for trackpads, back buttons, two-stage triggers, stick
  deadzones and rumble strength.
* Back buttons, trackpad clicks and directions can also send the **Guide / PS
  button** or the **PlayStation touchpad click** (asked for upstream in
  ddeverill/SteamlessController#121).
* Control modes that step aside while Steam or a Steam game is running. A game
  profile stays active for as long as its program runs, even when another window
  is in front (Wayland has no "focused window"). Upstream #100 describes
  the problem this avoids.
* Tray menu, start with login, event log, and one-click RetroArch controller
  profiles.

## Build from source

```sh
cargo build --release
./target/release/steamless-controller
```

Options: `--hidden` (start in the tray), `--page customize`,
`--screenshot <file.png>` (draw the window, save it and quit), `--version`.

## Making a release

1. Raise `version` in `Cargo.toml` and write a `## X.Y.Z` section at the top of
   `CHANGELOG.md`: what changed, in plain words, for the people using it.
2. Commit, push, then push a tag such as `v1.25.2` to this branch.

GitHub Actions (`.github/workflows/release.yml`) builds the program and
publishes a release with the changelog section as its notes (a tag without a
section is refused), a ready-to-run `.tar.gz` with its SHA-256, and the signed
pacman package and repository. Every push to the `main` branch is built too,
with the result under the workflow run's artifacts.

## Licence

MIT, like the original. See `LICENSE`.
