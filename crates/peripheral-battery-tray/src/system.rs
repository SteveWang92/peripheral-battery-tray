//! Per-user Windows integration: starting with Windows and showing toasts.

use std::fs;
use std::path::PathBuf;

use tauri_winrt_notification::Toast;
use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};

use crate::app_icon;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "peripheral-battery-tray";

/// The AppUserModelID toasts are sent under. Registering it under `AppUserModelId` lets an
/// unpackaged program show toasts with its own name, without a Start menu shortcut.
const APP_ID: &str = "peripheral-battery-tray";

fn run_command() -> Option<String> {
    let exe: PathBuf = std::env::current_exe().ok()?;
    Some(format!("\"{}\"", exe.display()))
}

/// Whether Windows starts this copy of the program at sign-in. An entry left behind by a
/// copy at another path does not count.
pub fn autostart_enabled() -> bool {
    let entry = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(RUN_KEY, KEY_READ)
        .and_then(|key| key.get_value::<String, _>(RUN_VALUE));
    matches!((entry, run_command()), (Ok(entry), Some(command)) if entry == command)
}

pub fn set_autostart(on: bool) {
    let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_WRITE)
    else {
        return;
    };
    if on {
        if let Some(command) = run_command() {
            let _ = key.set_value(RUN_VALUE, &command);
        }
    } else {
        let _ = key.delete_value(RUN_VALUE);
    }
}

/// Registers the toast identity, with the application icon written beside the battery log
/// so the notification header can show it.
pub fn register_toasts() {
    let path = format!(r"Software\Classes\AppUserModelId\{APP_ID}");
    let Ok((key, _)) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(path) else {
        return;
    };
    let _ = key.set_value("DisplayName", &"peripheral-battery-tray");
    if let Some(icon) = write_icon() {
        let _ = key.set_value("IconUri", &icon.display().to_string());
    }
}

fn write_icon() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("peripheral-battery-tray");
    let path = dir.join("icon.ico");
    let ico = app_icon::ico();
    if fs::read(&path).ok().as_deref() != Some(ico.as_slice()) {
        fs::create_dir_all(&dir).ok()?;
        fs::write(&path, ico).ok()?;
    }
    Some(path)
}

pub fn toast(title: &str, body: &str) {
    let _ = Toast::new(APP_ID).title(title).text1(body).show();
}
