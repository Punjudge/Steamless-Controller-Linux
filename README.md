# Steamless Controller for Linux

Play with the 2026 Steam Controller on Linux **without Steam running**. Games
see an ordinary Xbox or PlayStation controller, and the trackpads work as a
mouse on the desktop. It works with the wireless puck, a cable or Bluetooth.

This is a Linux version of
[SteamlessController](https://github.com/ddeverill/SteamlessController) for
Windows. It is made and tested on CachyOS with KDE Plasma.

## Install on CachyOS or Arch Linux

You do this once. Afterwards the program updates along with the rest of the
system.

**1. Trust the program's signature.** Open a terminal and paste these three
lines:

```sh
curl -fL -o /tmp/steamless-controller.asc https://raw.githubusercontent.com/Punjudge/Steamless-Controller-Linux/main/packaging/arch/steamless-controller.asc
sudo pacman-key --add /tmp/steamless-controller.asc
sudo pacman-key --lsign-key FC915DEADAFCF2D6E4C547FA50BE4488E96801E4
```

**2. Tell pacman where to find it.** Open the file with
`sudo nano /etc/pacman.conf` and add these two lines at the very end:

```ini
[steamless-controller]
Server = https://github.com/Punjudge/Steamless-Controller-Linux/releases/latest/download
```

**3. Install it:**

```sh
sudo pacman -Syu steamless-controller-bin
```

Then unplug and plug in the controller (or its puck) once. You'll find
**Steamless Controller** in the application menu.

From now on, `sudo pacman -Syu` (or your usual system update) also updates
Steamless Controller. To remove it: `sudo pacman -R steamless-controller-bin`.

## Install on other Linux systems

1. Download the newest `steamless-controller-…-x86_64-linux.tar.gz` from
   [Releases](https://github.com/Punjudge/Steamless-Controller-Linux/releases).
2. Unpack it, open the folder in a terminal and run `./install.sh`. The program
   appears in the application menu. `./install.sh --uninstall` removes it.
3. If Steam is installed, you're done. Without Steam, also install the device
   rule from the download as described in [the developer notes](https://github.com/Punjudge/Steamless-Controller-Linux/blob/main/docs/DEVELOPING.md#device-access).

## Using it

* Turn **Steamless Mode** on in the window or from the tray icon. While it is
  on, games see a normal controller.
* The trackpads work as a mouse right away: the right pad moves the pointer
  (press it to click) and the left pad scrolls.
* Under **Customize Controls** you choose what the trackpads, back buttons and
  triggers do, and you can make a separate setup for each game. A game's setup is
  used while that game is running.
* Back buttons and trackpads can also act as the **Guide / PS button** or the
  PlayStation touchpad click.
* **Control mode** decides when the program steps aside, for example while Steam
  or a Steam game is running, so the two don't fight over the controller.
* Game rumble is passed on to the controller.
* There are also start with login, an event log for troubleshooting, and
  ready-made controller setups for RetroArch.

Closing the window keeps the program running in the tray. Quit it from the tray
icon's menu.

## If something doesn't work

* **"The controller could not be opened"**: your login lacks access to the
  controller. Installing as above fixes it; unplug and plug in the controller
  afterwards.
* **"The virtual controller could not be created"** (the tray says "No virtual
  gamepad"): the program couldn't make the controller games see. Log out and in
  again after installing.
* Anything else: choose **Open Event Log** in the window or tray menu and include it
  when you
  [report a problem](https://github.com/Punjudge/Steamless-Controller-Linux/issues).

What changed in each version is in [CHANGELOG.md](CHANGELOG.md). Building it
yourself, making releases and the technical details are in
[docs/DEVELOPING.md](https://github.com/Punjudge/Steamless-Controller-Linux/blob/main/docs/DEVELOPING.md).

## Licence

MIT, like the original. See `LICENSE`.
