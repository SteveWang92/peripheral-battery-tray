# Changelog

All notable changes to peripheral-battery-tray are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Each release is tagged `vX.Y.Z`
on `main`, with the matching section below as its notes.

## [Unreleased]

### Added

- `--once` prints the battery of each ROG and Logitech device found.
- Tray icon per device showing battery digits, coloured when low or charging.
- A sleeping or switched-off device shows its last battery reading, greyed out.
- Grey crossed-out battery icon when no supported device is found.
- Clicking an icon shows device, battery, charging state and last update time.
- Right-click menu for refresh, low-battery threshold, refresh interval and merged icon.
- Optional battery log, switched on from the tray menu.
- Toasts when a device runs low and, optionally, when charging completes.
- Settings are saved and restored across restarts.
- Starts with Windows by default, switchable from the menu.
- English and Simplified Chinese, following the system language by default.

[Unreleased]: https://github.com/SteveWang92/peripheral-battery-tray/commits/dev
