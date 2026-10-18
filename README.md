# peripheral-battery-tray

A lightweight Windows tray battery monitor for wireless keyboards and mice from ASUS ROG and
Logitech. It needs neither Armoury Crate nor G HUB and runs as a single portable `.exe`.

This is an independent project, not affiliated with or endorsed by ASUS or Logitech. ASUS
and ROG are trademarks of ASUSTeK Computer Inc.; Logitech, Lightspeed, Unifying and Bolt are
trademarks of Logitech.

- The tray icon shows the battery percentage as digits, drawn for the taskbar's scale and
  theme: orange-red when low, green while charging. A device that falls asleep or is
  switched off keeps its last reading in grey, or shows a grey empty battery if it has not
  been read yet. With no supported device connected, the icon is a grey battery with a
  slash through it.
- Clicking an icon shows a card with each device's battery as a percentage and a bar, its
  connection, whether it is charging and when it was last read, and a refresh button; a
  click anywhere else closes it.
- Every setting is in the right-click menu: refresh now, the low-battery threshold, the
  refresh interval, one icon per device or one merged icon showing the lowest level, a
  full-charge alert, starting with Windows, the language (English or Simplified Chinese)
  and an optional battery log. The menu stays open while settings are changed and closes
  on a click outside it.
- A Windows notification appears once when a device drops to the low-battery threshold,
  and again only after it has been charged. The full-charge notification is off by
  default.
- It only ever sends battery queries; it never changes a device setting.

## Supported devices

Each cell shows the USB ID the device or receiver appears as, and its status:
![Tested](https://img.shields.io/badge/Tested-2ea44f) on real hardware,
![Untested](https://img.shields.io/badge/Untested-dfb317) implemented but not yet tried, or
![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e). A dash means no ID is
known for that connection.

### Logitech

Any device that reports its battery over HID++ 2.0 through one of these receivers is shown,
including one switched off at the time, under the name its receiver keeps for it until it
first answers. A battery estimated from a voltage or a coarse level rather than reported as
a percentage is shown with `≈` on the card.

| Receiver | USB ID | Status |
| --- | --- | --- |
| Lightspeed | `046D:C547` | ![Tested](https://img.shields.io/badge/Tested-2ea44f) |
| Lightspeed | `046D:C539`, `C53A`, `C53D`, `C53F`, `C541`, `C545`, `C54D` | ![Untested](https://img.shields.io/badge/Untested-dfb317) |
| Unifying | `046D:C52B`, `C532` | ![Untested](https://img.shields.io/badge/Untested-dfb317) |
| Bolt | `046D:C548` | ![Untested](https://img.shields.io/badge/Untested-dfb317) |
| USB cable or Bluetooth, without a receiver | — | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |

Tested devices: G502 X LIGHTSPEED. Devices that only speak the older HID++ 1.0, and
headsets on Logitech's newer headset protocol, are not supported.

### ASUS ROG

#### Keyboards

| Model | ROG OMNI receiver | Own 2.4 GHz receiver | USB cable | Bluetooth |
| --- | --- | --- | --- | --- |
| ROG Azoth | ![0B05:1ACE Tested](https://img.shields.io/badge/0B05%3A1ACE-Tested-2ea44f) | ![0B05:1A85 Untested](https://img.shields.io/badge/0B05%3A1A85-Untested-dfb317) | ![0B05:1A83 Untested](https://img.shields.io/badge/0B05%3A1A83-Untested-dfb317) | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |
| ROG Azoth Extreme | ![0B05:1ACE Untested](https://img.shields.io/badge/0B05%3A1ACE-Untested-dfb317) | — | ![0B05:1B3F Untested](https://img.shields.io/badge/0B05%3A1B3F-Untested-dfb317) | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |
| ROG Azoth Extreme SE | ![0B05:1ACE Untested](https://img.shields.io/badge/0B05%3A1ACE-Untested-dfb317) | — | ![0B05:1CEF Untested](https://img.shields.io/badge/0B05%3A1CEF-Untested-dfb317) | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |
| ROG Azoth X | — | ![0B05:1C25 Untested](https://img.shields.io/badge/0B05%3A1C25-Untested-dfb317) | ![0B05:1C24 Untested](https://img.shields.io/badge/0B05%3A1C24-Untested-dfb317) | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |
| ROG Claymore II | — | — | ![0B05:196B Untested](https://img.shields.io/badge/0B05%3A196B-Untested-dfb317) | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |
| ROG Falchion | — | ![0B05:193E Untested](https://img.shields.io/badge/0B05%3A193E-Untested-dfb317) | ![0B05:193C Untested](https://img.shields.io/badge/0B05%3A193C-Untested-dfb317) | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |
| ROG Falchion RX | — | — | ![0B05:1B04 Untested](https://img.shields.io/badge/0B05%3A1B04-Untested-dfb317) | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |
| ROG Falchion RX Low Profile | ![0B05:1ACE Untested](https://img.shields.io/badge/0B05%3A1ACE-Untested-dfb317) | — | — | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |
| ROG Strix Scope II 96 | ![0B05:1ACE Untested](https://img.shields.io/badge/0B05%3A1ACE-Untested-dfb317) | — | ![0B05:1AAE Untested](https://img.shields.io/badge/0B05%3A1AAE-Untested-dfb317) | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |
| ROG Strix Scope II 96 RX | ![0B05:1ACE Untested](https://img.shields.io/badge/0B05%3A1ACE-Untested-dfb317) | — | ![0B05:1B78 Untested](https://img.shields.io/badge/0B05%3A1B78-Untested-dfb317) | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |
| ROG Strix Scope RX TKL | — | ![0B05:1A07 Untested](https://img.shields.io/badge/0B05%3A1A07-Untested-dfb317) | — | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |

#### Mice

| Model | ROG OMNI receiver | Own 2.4 GHz receiver | USB cable | Bluetooth |
| --- | --- | --- | --- | --- |
| ROG Chakram | — | ![0B05:18E5 Untested](https://img.shields.io/badge/0B05%3A18E5-Untested-dfb317) | ![0B05:18E3 Untested](https://img.shields.io/badge/0B05%3A18E3-Untested-dfb317) | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |
| ROG Chakram X | — | ![0B05:1A1A Untested](https://img.shields.io/badge/0B05%3A1A1A-Untested-dfb317) | ![0B05:1A18 Untested](https://img.shields.io/badge/0B05%3A1A18-Untested-dfb317) | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |
| ROG Gladius II Wireless | — | ![0B05:18A0 Untested](https://img.shields.io/badge/0B05%3A18A0-Untested-dfb317) | — | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |
| ROG Gladius III Aimpoint | ![0B05:1ACE Untested](https://img.shields.io/badge/0B05%3A1ACE-Untested-dfb317) | ![0B05:1A72 Untested](https://img.shields.io/badge/0B05%3A1A72-Untested-dfb317) | ![0B05:1A70 Untested](https://img.shields.io/badge/0B05%3A1A70-Untested-dfb317) | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |
| ROG Gladius III Aimpoint EVA-02 | — | ![0B05:1B0C Untested](https://img.shields.io/badge/0B05%3A1B0C-Untested-dfb317) | ![0B05:1B0A Untested](https://img.shields.io/badge/0B05%3A1B0A-Untested-dfb317) | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |
| ROG Gladius III Wireless | — | ![0B05:197F Untested](https://img.shields.io/badge/0B05%3A197F-Untested-dfb317) | ![0B05:197D Untested](https://img.shields.io/badge/0B05%3A197D-Untested-dfb317) | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |
| ROG Harpe Ace Aim Lab Edition | ![0B05:1ACE Untested](https://img.shields.io/badge/0B05%3A1ACE-Untested-dfb317) | ![0B05:1A94 Untested](https://img.shields.io/badge/0B05%3A1A94-Untested-dfb317) | ![0B05:1A92 Untested](https://img.shields.io/badge/0B05%3A1A92-Untested-dfb317) | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |
| ROG Harpe Ace Extreme | ![0B05:1ACE Untested](https://img.shields.io/badge/0B05%3A1ACE-Untested-dfb317) | — | ![0B05:1B67 Untested](https://img.shields.io/badge/0B05%3A1B67-Untested-dfb317) | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |
| ROG Harpe Ace Mini | ![0B05:1ACE Untested](https://img.shields.io/badge/0B05%3A1ACE-Untested-dfb317) | — | ![0B05:1B63 Untested](https://img.shields.io/badge/0B05%3A1B63-Untested-dfb317) | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |
| ROG Harpe II Ace | — | ![0B05:1AD0 Untested](https://img.shields.io/badge/0B05%3A1AD0-Untested-dfb317) | ![0B05:1C69 Untested](https://img.shields.io/badge/0B05%3A1C69-Untested-dfb317) | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |
| ROG Keris II Ace | ![0B05:1ACE Untested](https://img.shields.io/badge/0B05%3A1ACE-Untested-dfb317) | — | ![0B05:1B16 Untested](https://img.shields.io/badge/0B05%3A1B16-Untested-dfb317) | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |
| ROG Keris II Origin | ![0B05:1ACE Untested](https://img.shields.io/badge/0B05%3A1ACE-Untested-dfb317) | — | ![0B05:1C0C Untested](https://img.shields.io/badge/0B05%3A1C0C-Untested-dfb317) | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |
| ROG Keris II Origin KJP | ![0B05:1ACE Untested](https://img.shields.io/badge/0B05%3A1ACE-Untested-dfb317) | — | ![0B05:1D4C Untested](https://img.shields.io/badge/0B05%3A1D4C-Untested-dfb317) | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |
| ROG Keris Wireless | — | ![0B05:1960 Untested](https://img.shields.io/badge/0B05%3A1960-Untested-dfb317) | ![0B05:195E Untested](https://img.shields.io/badge/0B05%3A195E-Untested-dfb317) | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |
| ROG Keris Wireless Aimpoint | ![0B05:1ACE Untested](https://img.shields.io/badge/0B05%3A1ACE-Untested-dfb317) | ![0B05:1A68 Untested](https://img.shields.io/badge/0B05%3A1A68-Untested-dfb317) | ![0B05:1A66 Untested](https://img.shields.io/badge/0B05%3A1A66-Untested-dfb317) | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |
| ROG Keris Wireless EVA Edition | — | ![0B05:1A59 Untested](https://img.shields.io/badge/0B05%3A1A59-Untested-dfb317) | ![0B05:1A57 Untested](https://img.shields.io/badge/0B05%3A1A57-Untested-dfb317) | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |
| ROG Pugio II | — | ![0B05:1908 Untested](https://img.shields.io/badge/0B05%3A1908-Untested-dfb317) | ![0B05:1906 Untested](https://img.shields.io/badge/0B05%3A1906-Untested-dfb317) | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |
| ROG Spatha X | — | ![0B05:1979 Untested](https://img.shields.io/badge/0B05%3A1979-Untested-dfb317) | ![0B05:1977 Untested](https://img.shields.io/badge/0B05%3A1977-Untested-dfb317) | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |
| ROG Strix Carry | — | ![0B05:18B4 Untested](https://img.shields.io/badge/0B05%3A18B4-Untested-dfb317) | — | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |
| ROG Strix Impact II Wireless | — | ![0B05:1949 Untested](https://img.shields.io/badge/0B05%3A1949-Untested-dfb317) | ![0B05:1947 Untested](https://img.shields.io/badge/0B05%3A1947-Untested-dfb317) | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |
| ROG Strix Impact III Wireless | ![0B05:1ACE Untested](https://img.shields.io/badge/0B05%3A1ACE-Untested-dfb317) | — | — | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |

A report from an untested device, working or not, is welcome as an issue.

## Download

Requires 64-bit Windows 10 or 11. Download `peripheral-battery-tray.exe` from the
[latest release](https://github.com/SteveWang92/peripheral-battery-tray/releases/latest), put
it in a folder it can stay in, and run it. There is no installer.

The executable is unsigned, so Microsoft Defender SmartScreen may warn that it is an
unrecognized app; choose **More info**, then **Run anyway**, after checking that the
download came from this repository.

To update, quit the running copy from its menu and replace the file with the new one at the
same path; settings and "Start with Windows" carry over.

## Usage

Run `peripheral-battery-tray.exe`. Only one copy runs at a time. It can run alongside
Armoury Crate and G HUB; running it together with rog-battery-tray shows ROG devices twice,
so use one or the other.

From a terminal, `peripheral-battery-tray.exe --once` prints every detected device's
battery and exits. PowerShell and Command Prompt do not wait for a windowed program, so pipe
it there to see the output: `peripheral-battery-tray.exe --once | more`.

Settings are saved to `%APPDATA%\peripheral-battery-tray\config.toml`. The battery log, when
switched on from the menu, is written to `%LOCALAPPDATA%\peripheral-battery-tray\battery.log`.

On first run the program adds itself to the current user's startup programs
(`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`); switching off "Start with Windows"
removes that entry. To show notifications under its own name it registers
`HKCU\Software\Classes\AppUserModelId\peripheral-battery-tray` and keeps the notification
icon in `%LOCALAPPDATA%\peripheral-battery-tray\icon.ico`. If the program ever fails, the
error is recorded in `crash.log` in the same folder. Nothing else is written.

## Uninstalling

1. Switch off "Start with Windows" in the menu, then choose "Exit".
2. Delete `peripheral-battery-tray.exe` and the folders `%APPDATA%\peripheral-battery-tray`
   and `%LOCALAPPDATA%\peripheral-battery-tray`.
3. Optionally remove the notification registration from PowerShell:
   `Remove-Item HKCU:\Software\Classes\AppUserModelId\peripheral-battery-tray`.

## Building

Requires a stable Rust toolchain with the MSVC target on Windows.

```sh
cargo build --release
```

The executable is `target/release/peripheral-battery-tray.exe`.

## Credits

The Logitech HID++ protocol details come from [Solaar](https://github.com/pwr-Solaar/Solaar)
(GPL-2.0), and the ROG device IDs and protocol details, through the `rog-battery` library of
[rog-battery-tray](https://github.com/SteveWang92/rog-battery-tray), from
[G-Helper](https://github.com/seerge/g-helper) (GPL-3.0). This project is an independent
implementation and contains none of their code.

## License

[MIT](LICENSE)
