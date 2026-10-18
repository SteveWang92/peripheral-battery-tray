# Changelog

All notable changes to peripheral-battery-tray are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Each release is tagged `vX.Y.Z`
on `main`, with the matching section below as its notes.

## [Unreleased]

## [0.2.0] - 2026-10-18

### Added

- The card estimates each device's time left on battery from its recent drain.

### Fixed

- Identical ROG devices each keep their own battery reading.

## [0.1.1] - 2026-10-18

### Fixed

- "Start with Windows" stays off when the settings file cannot be saved.
- Identical Logitech devices on one receiver each keep their own battery reading and alerts.
- A Logitech device paired while the tray runs appears without replugging its receiver.

## [0.1.0] - 2026-10-18

### Added

- `--once` prints the battery of each ROG and Logitech device found.
- Logitech devices on Lightspeed, Unifying and Bolt receivers, untested beyond Lightspeed.
- The card marks a battery estimated from voltage with `≈`.
- Tray icon per device showing battery digits, coloured when low or charging.
- A sleeping or switched-off device shows its last battery reading, greyed out.
- Grey crossed-out battery icon when no supported device is found.
- Clicking an icon shows each device's battery bar, connection and last update.
- Right-click menu lists each device with its connection and battery on a second line.
- Right-click menu for refresh, low-battery threshold, refresh interval and merged icon.
- A sudden battery drop is shown only once a second reading confirms it.
- Optional battery log, switched on from the tray menu.
- Toasts when a device runs low and, optionally, when charging completes.
- Settings are saved and restored across restarts.
- Starts with Windows by default, switchable from the menu.
- English and Simplified Chinese, following the system language by default.

[Unreleased]: https://github.com/SteveWang92/peripheral-battery-tray/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/SteveWang92/peripheral-battery-tray/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/SteveWang92/peripheral-battery-tray/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/SteveWang92/peripheral-battery-tray/releases/tag/v0.1.0
