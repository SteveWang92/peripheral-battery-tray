//! Settings saved to `%APPDATA%\peripheral-battery-tray\config.toml`, which a person may also edit
//! by hand.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::strings::Language;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// Battery percentage at or below which a device counts as low; 0 turns the alert off.
    pub low_threshold: u8,
    pub interval_secs: u64,
    pub merged_icon: bool,
    pub full_alert: bool,
    pub language: Language,
    pub battery_log: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            low_threshold: 20,
            interval_secs: 60,
            merged_icon: false,
            full_alert: false,
            language: Language::System,
            battery_log: false,
        }
    }
}

impl Settings {
    pub fn low_threshold(&self) -> Option<u8> {
        (self.low_threshold > 0).then_some(self.low_threshold)
    }
}

pub enum Loaded {
    /// No settings file yet: this is the first run.
    FirstRun,
    Saved(Settings),
    /// The file exists but cannot be used.
    Invalid,
}

fn path() -> Option<PathBuf> {
    let base = PathBuf::from(std::env::var_os("APPDATA")?);
    Some(base.join("peripheral-battery-tray").join("config.toml"))
}

pub fn load() -> Loaded {
    let Some(path) = path() else {
        return Loaded::Invalid;
    };
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Loaded::FirstRun,
        Err(_) => return Loaded::Invalid,
    };
    match toml::from_str::<Settings>(&text) {
        Ok(settings)
            if settings.low_threshold < 100 && (5..=86_400).contains(&settings.interval_secs) =>
        {
            Loaded::Saved(settings)
        }
        _ => Loaded::Invalid,
    }
}

/// Writes the settings and tells whether the file was written; a failure leaves the running
/// settings in effect for this session.
pub fn save(settings: &Settings) -> bool {
    let Some(path) = path() else {
        return false;
    };
    let Ok(text) = toml::to_string(settings) else {
        return false;
    };
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    fs::write(path, text).is_ok()
}
