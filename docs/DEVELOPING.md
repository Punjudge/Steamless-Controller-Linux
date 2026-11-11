# Developer notes

The technical side of Steamless Controller for Linux: how it is built, how
releases and packages are made, and what it needs from the system. The README
is for the people using it.

The program is written in Rust with an egui window and a StatusNotifierItem
tray icon. How it matches the Windows original, value by value with addresses
in the original program, is in [MECHANICS.md](../MECHANICS.md).

## Build from source

```sh
cargo build --release
./target/release/steamless-controller
```

Options: `--hidden` (start in the tray), `--page customize`,
`--screenshot <file.png>` (draw the window, save it and quit, never touching
the controller), `--version`.

`cargo test` runs the checks of values and rules against the original.

## Device access

The program reads the controller through `/dev/hidraw*` and creates its
virtual gamepad, mouse and keyboard through `/dev/uinput`. The login at the
seat needs access to both:

* Steam's device rules (`steam-devices`, or `steam`) give it, or
* `packaging/udev/70-steamless-controller.rules`, the rule the pacman package
  installs: copy it to `/usr/lib/udev/rules.d/`, run
  `sudo udevadm control --reload` and plug the controller in again.

## Making a release

1. Raise `version` in `Cargo.toml` and write a `## X.Y.Z` section at the top of
   `CHANGELOG.md`: what changed, in plain words, for the people using it.
2. Commit, push to `main`, then push a tag such as `v1.25.4`.

GitHub Actions (`.github/workflows/release.yml`) then:

* builds the program and publishes a release whose notes are the changelog
  section (`packaging/release-notes.sh`; a tag without a section is refused),
* attaches a ready-to-run `.tar.gz` with its SHA-256,
* builds the pacman package in an Arch container (`packaging/arch/make-pkg.sh`),
  signs it and makes the pacman repository (`packaging/arch/sign-repo.sh`).

Every push to `main` is built too, with the result under the workflow run's
artifacts.

## The pacman repository and its signature

Each release holds a small pacman repository named `steamless-controller`:
the package `steamless-controller-bin-X.Y.Z-1-x86_64.pkg.tar.zst`, a copy under
the fixed name `steamless-controller-bin-x86_64.pkg.tar.zst`, the database
files `steamless-controller.db` and `.files`, and a `.sig` beside each.
`https://github.com/Punjudge/Steamless-Controller-Linux/releases/latest/download`
is therefore a pacman `Server`. With the key trusted, a single package also
installs straight from its address:
`sudo pacman -U https://github.com/Punjudge/Steamless-Controller-Linux/releases/latest/download/steamless-controller-bin-x86_64.pkg.tar.zst`.

* Public key: `packaging/arch/steamless-controller.asc`, fingerprint
  `FC91 5DEA DAFC F2D6 E4C5  47FA 50BE 4488 E968 01E4`.
* The secret key is the repository secret `PACMAN_SIGNING_KEY` in GitHub
  Actions. `sign-repo.sh` refuses to sign when it does not match the `.asc`.
* Replacing the key is described in `packaging/aur/README.md`.

## AUR

PKGBUILDs for building from source and for repackaging the release are in
`packaging/aur/`, with how to publish and update them.
