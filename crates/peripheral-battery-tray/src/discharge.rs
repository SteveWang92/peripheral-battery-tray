//! Remaining battery time, estimated from how fast each device has drained since it was last
//! charged. The record survives restarts in
//! `%LOCALAPPDATA%\peripheral-battery-tray\history.toml`.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// The drop, in percentage points, before an estimate is given: a percentage moves in whole
/// points, so a smaller drop says too little about the rate.
const MIN_DROP: u8 = 5;
/// A rise this small while not charging is a wobble in the reading, not a charge.
const WOBBLE: u8 = 2;

/// One device's discharge since its last charge.
#[derive(Clone, Copy, Serialize, Deserialize)]
struct Discharge {
    /// The lowest percentage seen.
    last: u8,
    /// When the percentage first fell, as Unix seconds, and to what. Time at the full
    /// percentage right after a charge does not count, since a battery shows full for a
    /// while before its first point goes.
    first_drop: Option<(i64, u8)>,
}

/// Every device's discharge, by device.
#[derive(Default, Serialize, Deserialize)]
pub struct History {
    devices: BTreeMap<String, Discharge>,
    /// Set when a record changed since the last write.
    #[serde(skip)]
    changed: bool,
}

fn path() -> Option<PathBuf> {
    let base = PathBuf::from(std::env::var_os("LOCALAPPDATA")?);
    Some(base.join("peripheral-battery-tray").join("history.toml"))
}

impl History {
    /// The saved history; a missing or unreadable file starts an empty one.
    pub fn load() -> History {
        path()
            .and_then(|path| fs::read_to_string(path).ok())
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Records a reading taken at `now` (Unix seconds) and returns the device's estimated
    /// time left.
    pub fn record(
        &mut self,
        device: &str,
        percent: u8,
        charging: bool,
        now: i64,
    ) -> Option<Duration> {
        let before = self.devices.get(device).copied();
        let after = match before {
            _ if charging => None,
            None => Some(Discharge {
                last: percent,
                first_drop: None,
            }),
            Some(d) if percent > d.last.saturating_add(WOBBLE) => Some(Discharge {
                last: percent,
                first_drop: None,
            }),
            Some(d) if percent < d.last => Some(Discharge {
                last: percent,
                first_drop: d.first_drop.or(Some((now, percent))),
            }),
            Some(d) => Some(d),
        };
        if before.map(|d| (d.last, d.first_drop)) != after.map(|d| (d.last, d.first_drop)) {
            match after {
                Some(d) => self.devices.insert(device.to_owned(), d),
                None => self.devices.remove(device),
            };
            self.changed = true;
        }
        let (since, from) = after?.first_drop?;
        let drop = from.saturating_sub(percent);
        let elapsed = now - since;
        if drop < MIN_DROP || elapsed <= 0 {
            return None;
        }
        let seconds = elapsed as f64 / f64::from(drop) * f64::from(percent);
        Some(Duration::from_secs_f64(seconds))
    }

    /// Writes the history if a record changed; a failure leaves it to the next change.
    pub fn save(&mut self) {
        if !std::mem::take(&mut self.changed) {
            return;
        }
        let Some(path) = path() else {
            return;
        };
        let Ok(text) = toml::to_string(self) else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(path, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 3600;

    fn history() -> History {
        History::default()
    }

    #[test]
    fn estimates_from_the_drain_since_the_first_drop() {
        let mut h = history();
        // Full for a while after charging, then 2 points an hour.
        assert_eq!(h.record("d", 100, false, 0), None);
        assert_eq!(h.record("d", 100, false, 5 * HOUR), None);
        assert_eq!(h.record("d", 98, false, 6 * HOUR), None);
        let left = h.record("d", 88, false, 11 * HOUR).unwrap();
        assert_eq!(left.as_secs() as i64, 44 * HOUR);
    }

    #[test]
    fn charging_starts_over() {
        let mut h = history();
        h.record("d", 60, false, 0);
        h.record("d", 50, false, HOUR);
        h.record("d", 40, false, 2 * HOUR);
        assert_eq!(h.record("d", 45, true, 3 * HOUR), None);
        assert_eq!(h.record("d", 80, false, 4 * HOUR), None);
        assert_eq!(h.record("d", 79, false, 5 * HOUR), None);
    }

    #[test]
    fn a_rise_beyond_a_wobble_counts_as_a_charge() {
        let mut h = history();
        h.record("d", 60, false, 0);
        h.record("d", 50, false, HOUR);
        assert!(h.record("d", 40, false, 2 * HOUR).is_some());
        assert!(h.record("d", 42, false, 3 * HOUR).is_some());
        assert_eq!(h.record("d", 90, false, 4 * HOUR), None);
    }
}
