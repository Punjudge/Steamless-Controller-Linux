# Changelog

What changed in each release of Steamless Controller for Linux. The release
build copies the section for its tag into the GitHub release notes, so write
the next one under a new `## X.Y.Z` heading before tagging.

## 1.25.3

**Event log**

* When a trackpad jumps much further in one frame than a finger can move (6000
  units, about a fifth of the pad), the log gets a `TRACKPAD:` line with the
  distance, the time since the previous frame and how hard the pad was touched.
  At most one line every 3 seconds, as in the Windows original. It helps tell a
  faulty touch reading apart from lost reports.

## 1.25.2

**Install and updates**

* The releases are now a **signed pacman repository**. Trust the key once, add
  `[steamless-controller]` to `/etc/pacman.conf`, and `pacman -Syu` keeps the
  program up to date (see "Install" in the README).
* Every package has a `.sig` signature, so
  `sudo pacman -U <address of the package>` works directly once the key is
  trusted.

**Event log**

* A sign of life every 15 minutes (`HEARTBEAT: alive …`): uptime, whether the
  controller is connected, game mode, Steam state, control mode. If a beat comes
  more than a minute late, a line says the program was blocked, and for how
  long. Time spent in suspend doesn't count. Same as the Windows original.
* When the keepalive to the controller fails, the log says how many tries have
  failed (at tries 1, 3, 15 and 60), how many reports the controller still sent,
  and when it recovers. This helps with dropouts like upstream issue #122.

## 1.25.1

**New**

* Back buttons, trackpad clicks and trackpad directions can send the
  **Guide / PS button** or the **PlayStation touchpad click** (asked for
  upstream in #121).
* Each release has a ready-made pacman package for Arch Linux and CachyOS, plus
  a device rule (udev), so it works without Steam installed.

**Changed to match the original program**

* Trackpad movement ticks are only counted as sent when the controller took
  them, so a missed tick is retried.
* Clicking a trigger past its click point buzzes at once instead of on the next
  rumble cycle.

## 1.25.0

First Linux release: the 2026 Steam Controller as an Xbox 360 or
PlayStation-style gamepad, mouse and keyboard without Steam running. Rust and
egui, with a tray icon.

* Trackpads, back buttons and the controller's mouse mode, configurable per
  game.
* Per-game profiles: two-stage triggers, stick deadzones, rumble strength.
* Rumble from games is sent back to the controller.
* Steps aside while Steam or a Steam game runs.
* Start with login, event log, RetroArch controller profiles.
* Timings and values checked against the compiled Windows 1.25 program:
  scrolling follows the desktop's wheel-lines setting, plus key repeat and log
  rotation.
