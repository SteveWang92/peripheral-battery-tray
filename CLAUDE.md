# peripheral-battery-tray contributor guidance

This is the authoritative guidance for working in peripheral-battery-tray, for a coding
agent and a human contributor alike. The maintainer's own cross-project rules live outside
this repository; where they and the rules below differ, the rules below win.

## What this is

A lightweight Windows tray battery monitor for wireless peripherals of several brands,
starting with ASUS ROG and Logitech. It is a Cargo workspace:

| Crate | Role |
| --- | --- |
| `crates/logitech-battery` | Logitech device discovery and battery queries over HID++. No UI, so other applications can reuse it. |
| `crates/rog-battery` | ASUS ROG device discovery and battery queries over HID. No UI, so other applications can reuse it. |
| `crates/peripheral-battery-tray` | The tray application, plus the `--once` command-line mode that prints every detected device's battery and exits. |

## Project exceptions

- Larger planned work is a GitHub issue. Smaller items that wait on hardware, observation
  or an upstream release live in the maintainer's untracked `BACKLOG.local.md`. Read both
  before starting work when they are present.
- The repository is public. Only the maintainer decides its visibility; never change it.

## Device rules

- Read-only: send query commands only. Never write a device setting, firmware value,
  lighting state, receiver register or notification flag.
- The Logitech protocol facts come from Solaar (GPL-2.0), the ROG ones from G-Helper
  (GPL-3.0). This project is an independent MIT implementation: reuse the facts (IDs,
  command bytes, reply offsets), never their code.
- A device that is not tested on real hardware is marked untested wherever it is listed.

## Repository conventions

- Line endings are LF everywhere (`.gitattributes`); formatting is rustfmt with the
  settings in `rustfmt.toml`.
- The root `.gitignore` is an allowlist: a new top-level file or directory needs its own
  `!/name` entry, or Git skips it silently.
- Local-only files carry `.local` in their name and are covered by the repository-wide
  `*.local` and `*.local.*` rules; do not add one-off ignore entries.
- Keep tracked content free of machine-specific paths, account details and anything else
  personal to one contributor.
- Documents and comments describe the current state only; history belongs in commits and
  `CHANGELOG.md`.

## Tray rules

- Battery is shown only through the tray icon, the card a left click on it shows, the
  native right-click menu and Windows toasts. There is no main window or settings window,
  and no native tooltip: the shell shows it only after a hover delay no program can
  shorten, and cuts it at 128 characters.
- The card (`popup.rs`) is a GDI-drawn popup that closes when it loses activation. Its
  window procedure runs inside Win32 calls the tray makes while holding its own state, so
  the card keeps its state apart from the tray's.
- Icons are drawn in code, never stored as image files: tray icons at runtime for the
  taskbar's DPI and theme (`icon.rs`), the application icon from `app_icon.rs`, which
  `build.rs` embeds in the executable with its version information and the tray writes out
  for toasts.
- Native menus close after a click. A `WH_MSGFILTER` hook in `tray.rs` forwards a click on
  a command item as its `WM_COMMAND` and swallows it, so the menu stays open; menu items
  are therefore updated in place while it shows, and anything that replaces the menu or
  an icon waits until it closes. A language change rebuilds the menu after it closes and
  reopens it from the same icon.
- The shell knows a tray icon by the executable's path and the icon's number, and keeps
  its "always show" setting under them. Icons are numbered in creation order, so as long
  as none is recreated, every run numbers them the same. Never recreate an icon to change
  it: give the existing one a new image, tooltip or menu, or it appears as a new program in
  the taskbar settings. Icons carry no GUID: the shell binds a GUID to one executable
  path and refuses it while a stale registration holds it.
- A menu shown again by the tray itself goes through `TrackPopupMenu` directly, never
  `TrayIcon::show_menu`, which holds the icon borrowed for as long as the menu is open;
  the menu hook updating that icon would then panic and end the process.
- A panic is written to `%LOCALAPPDATA%\peripheral-battery-tray\crash.log`; the tray has no
  console to show it.
- HID queries run on the poller thread, never on the thread that owns the tray.
- Settings live in `%APPDATA%\peripheral-battery-tray\config.toml`. The file may be edited by
  hand, so it is validated on load; an invalid file means defaults plus one toast, and the
  file is left alone until a setting changes.
- Autostart is the per-user `Run` registry value, the single source of truth for that
  setting; it is not duplicated in the config file. A value pointing at another copy's
  path counts as off.
- Every string the tray shows lives in `strings.rs`, in English and Simplified Chinese,
  except the language names, each written in its own language, and the program's name.
  The `--once` output and its errors are English only.
- The battery log is diagnostic: off by default, switched from the menu, written to
  `%LOCALAPPDATA%\peripheral-battery-tray\battery.log`.
- The card's time-left estimate comes from each device's drain since its last charge, which
  the poller keeps in `%LOCALAPPDATA%\peripheral-battery-tray\history.toml` so it survives
  restarts (`discharge.rs`).

## Development

- `dev` is the default branch and receives all work; `main` receives only releases.
- A contribution is a pull request into `dev`. CI (`.github/workflows/ci.yml`) runs the
  [checks](#checks) on it, and the maintainer squash-merges it under one Conventional
  Commit title (`feat:`, `fix:`, `refactor:`, `chore:`, `build:`, `docs:`).
- Record each user-facing change under `## [Unreleased]` in `CHANGELOG.md` in the same
  commit that makes it.

## Releases

Only the maintainer releases, with [changedeck](https://github.com/SteveWang92/changedeck):

1. `changedeck prep` sets the version in every file `changedeck.json` lists (a new crate's
   `Cargo.toml` and `Cargo.lock` entry go there), dates the changelog section, pushes
   `dev` and opens the `dev` → `main` release pull request.
2. The release pull request is reviewed and CI passes on it; fixes are ordinary commits on
   `dev`, then `prep` again. `changedeck reversion X.Y.Z` changes the prepared version.
3. `changedeck ship` merges it with a merge commit, tags `vX.Y.Z` on `main`, publishes the
   GitHub release and fast-forwards `dev`.
4. Publishing the release runs `.github/workflows/release.yml`, which builds
   `peripheral-battery-tray.exe` from the tag and attaches it under that unversioned name,
   so an update replaced at the same path keeps its autostart entry.

## Checks

Run before committing; CI runs the same on pull requests:

```sh
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo build --locked --workspace --release
```
