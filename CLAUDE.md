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
| `crates/peripheral-battery-tray` | The tray application, plus the `--once` command-line mode that prints every detected device's battery and exits. |

ROG devices come from the `rog-battery` library of
[rog-battery-tray](https://github.com/SteveWang92/rog-battery-tray), a Git dependency
pinned to one of that repository's release tags.

## Project exceptions

- Planned work and progress live in the maintainer's untracked `PLAN.local.md`, not in
  GitHub issues. Read it before starting work when it is present; do not open issues for
  planned work.
- The repository is private. Only the maintainer decides when it becomes public; never
  change its visibility.

## Relationship to rog-battery-tray

- New features land here. rog-battery-tray is the published ROG-only program and is not
  extended in step with this one.
- `rog-battery` code is never copied here. A change it needs is made in rog-battery-tray,
  released there under a new tag, and picked up here by bumping the tag.
- A fix or feature that is low-level or general enough to suit rog-battery-tray as well is
  proposed to the maintainer, who decides whether it goes there too.

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
cargo build --locked --workspace --release
```
