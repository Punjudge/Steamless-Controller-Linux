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
plug the controller in again.

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
* Control modes that step aside while Steam or a Steam game is running.
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

Push a tag such as `v1.25.0` to this branch. GitHub Actions
(`.github/workflows/release.yml`) builds the program and attaches a ready-to-run
`.tar.gz` with its SHA-256 to a new release. Every push to the `linux` branch is
built too, with the result under the workflow run's artifacts.

## Licence

MIT, like the original. See `LICENSE`.
