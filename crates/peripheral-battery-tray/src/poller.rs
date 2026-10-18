//! Battery polling on a background thread, so a device that is slow to answer never blocks
//! the tray. Each round's results go to the tray as a [`Snapshot`] and, when logging is on,
//! to the log file.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use logitech_battery::HidApi;

const ASUS_VID: u16 = 0x0B05;
const LOGITECH_VID: u16 = 0x046D;

/// A drop larger than this between two readings, while not charging, is held back until the
/// next reading confirms it: a G502 X reported 50% for one reading after being unplugged
/// at 94%.
const SUDDEN_DROP: u8 = 20;

pub enum Command {
    Refresh,
    Interval(Duration),
    Logging(bool),
}

/// How a device reaches the computer, under its brand.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Connection {
    Rog(rog_battery::Connection),
    Logitech(logitech_battery::Connection),
}

/// The result of one battery query, whichever brand answered it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reading {
    Battery {
        percent: u8,
        charging: bool,
        /// Derived from a voltage or a coarse level rather than reported by the device.
        estimated: bool,
    },
    /// The device is asleep, off or out of range.
    Asleep,
}

impl From<rog_battery::Reading> for Reading {
    fn from(reading: rog_battery::Reading) -> Self {
        match reading {
            rog_battery::Reading::Battery { percent, charging } => Reading::Battery {
                percent,
                charging,
                estimated: false,
            },
            rog_battery::Reading::Asleep => Reading::Asleep,
        }
    }
}

impl From<logitech_battery::Reading> for Reading {
    fn from(reading: logitech_battery::Reading) -> Self {
        match reading {
            logitech_battery::Reading::Battery {
                percent,
                charging,
                estimated,
            } => Reading::Battery {
                percent,
                charging,
                estimated,
            },
            logitech_battery::Reading::Asleep => Reading::Asleep,
        }
    }
}

pub struct DeviceStatus {
    pub name: String,
    pub connection: Connection,
    /// How many devices before this one in the snapshot share its name and connection, so
    /// identical devices keep apart.
    pub instance: usize,
    /// A failed query counts as not responding; its error goes to the log.
    pub reading: Reading,
}

/// Identifies one device across snapshots.
pub type DeviceKey = (String, Connection, usize);

impl DeviceStatus {
    pub fn key(&self) -> DeviceKey {
        (self.name.clone(), self.connection, self.instance)
    }
}

pub struct Snapshot {
    pub devices: Vec<DeviceStatus>,
    pub at: DateTime<Local>,
}

/// The log file, in the per-user local application data folder.
pub fn log_path() -> Option<PathBuf> {
    let base = PathBuf::from(std::env::var_os("LOCALAPPDATA")?);
    Some(base.join("peripheral-battery-tray").join("battery.log"))
}

/// Starts polling. `notify` runs on the polling thread after each snapshot is sent, to wake
/// the tray's message loop. The thread ends when the returned sender is dropped.
pub fn spawn(
    api: HidApi,
    interval: Duration,
    logging: bool,
    snapshots: Sender<Snapshot>,
    notify: impl Fn() + Send + 'static,
) -> Sender<Command> {
    let (commands, inbox) = mpsc::channel();
    thread::spawn(move || {
        Poller {
            api,
            rog: Vec::new(),
            rog_paths: Vec::new(),
            logitech: Vec::new(),
            logitech_paths: Vec::new(),
            shown: HashMap::new(),
            doubted: HashSet::new(),
            log: None,
        }
        .run(interval, logging, inbox, snapshots, notify)
    });
    commands
}

struct Poller {
    api: HidApi,
    rog: Vec<rog_battery::Device>,
    /// The ASUS HID paths present at the last ROG discovery.
    rog_paths: Vec<String>,
    logitech: Vec<logitech_battery::Device>,
    /// The Logitech HID paths present at the last Logitech discovery.
    logitech_paths: Vec<String>,
    /// Each device's last battery percentage passed on to the tray.
    shown: HashMap<DeviceKey, u8>,
    /// Devices whose last reading was a sudden drop that was held back.
    doubted: HashSet<DeviceKey>,
    log: Option<File>,
}

impl Poller {
    fn run(
        mut self,
        mut interval: Duration,
        logging: bool,
        inbox: Receiver<Command>,
        snapshots: Sender<Snapshot>,
        notify: impl Fn(),
    ) {
        self.set_logging(logging);
        loop {
            let polled = Instant::now();
            if snapshots.send(self.poll()).is_err() {
                return;
            }
            notify();
            loop {
                let wait = (polled + interval).saturating_duration_since(Instant::now());
                match inbox.recv_timeout(wait) {
                    Ok(Command::Refresh) | Err(RecvTimeoutError::Timeout) => break,
                    Ok(Command::Interval(new)) => interval = new,
                    Ok(Command::Logging(on)) => self.set_logging(on),
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
        }
    }

    fn poll(&mut self) -> Snapshot {
        // Discovery probes every candidate interface, so each brand's runs only when its set
        // of HID interfaces changes or one of its queries failed; otherwise a round sends
        // exactly one battery query per device.
        if let Err(error) = self.api.refresh_devices() {
            self.log_line(&format!("device refresh failed: {error}"));
        }
        let rog_paths = self.paths(ASUS_VID);
        if self.rog.is_empty() || rog_paths != self.rog_paths {
            self.rog = rog_battery::discover(&self.api);
            self.rog_paths = rog_paths;
        }
        let logitech_paths = self.paths(LOGITECH_VID);
        if self.logitech.is_empty() || logitech_paths != self.logitech_paths {
            self.logitech = logitech_battery::discover(&self.api);
            self.logitech_paths = logitech_paths;
        }
        if self.rog.is_empty() && self.logitech.is_empty() {
            self.log_line("no supported device found");
        }

        let mut devices = Vec::with_capacity(self.rog.len() + self.logitech.len());
        let mut rog_failed = false;
        for i in 0..self.rog.len() {
            let device = &self.rog[i];
            let started = Instant::now();
            let result = device.read_battery(&self.api);
            let elapsed = started.elapsed().as_millis();
            let (name, connection) = (device.name(), device.connection());
            let text = match &result {
                Ok(reading) => reading.to_string(),
                Err(error) => format!("query failed: {error}"),
            };
            self.log_line(&format!("{name} ({connection}): {text}  [{elapsed} ms]"));
            rog_failed |= result.is_err();
            let status = DeviceStatus {
                name: name.to_owned(),
                connection: Connection::Rog(connection),
                instance: 0,
                reading: result.map_or(Reading::Asleep, Reading::from),
            };
            devices.push(self.steady(numbered(status, &devices)));
        }
        let mut logitech_failed = false;
        for i in 0..self.logitech.len() {
            let device = &self.logitech[i];
            let started = Instant::now();
            let result = device.read_battery(&self.api);
            let elapsed = started.elapsed().as_millis();
            // Read after the query: a device's first answer replaces its shortened name.
            let (name, connection) = (device.name(), device.connection());
            let text = match &result {
                Ok(reading) => reading.to_string(),
                Err(error) => format!("query failed: {error}"),
            };
            self.log_line(&format!("{name} ({connection}): {text}  [{elapsed} ms]"));
            logitech_failed |= result.is_err();
            let status = DeviceStatus {
                name,
                connection: Connection::Logitech(connection),
                instance: 0,
                reading: result.map_or(Reading::Asleep, Reading::from),
            };
            devices.push(self.steady(numbered(status, &devices)));
        }
        if rog_failed {
            self.rog.clear();
        }
        if logitech_failed {
            self.logitech.clear();
        }
        Snapshot {
            devices,
            at: Local::now(),
        }
    }

    /// Holds back a sudden drop until a second reading in a row shows it, passing on the
    /// last percentage meanwhile.
    fn steady(&mut self, mut status: DeviceStatus) -> DeviceStatus {
        let Reading::Battery {
            percent,
            charging,
            estimated,
        } = status.reading
        else {
            return status;
        };
        let key = status.key();
        let held = self.shown.get(&key).copied().filter(|&shown| {
            !charging && shown.saturating_sub(percent) > SUDDEN_DROP && !self.doubted.contains(&key)
        });
        if let Some(shown) = held {
            self.log_line(&format!(
                "{}: {percent}% held back, kept {shown}%",
                status.name
            ));
            self.doubted.insert(key);
            status.reading = Reading::Battery {
                percent: shown,
                charging,
                estimated,
            };
        } else {
            self.doubted.remove(&key);
            self.shown.insert(key, percent);
        }
        status
    }

    /// The sorted HID paths of one vendor's interfaces.
    fn paths(&self, vendor: u16) -> Vec<String> {
        let mut paths: Vec<String> = self
            .api
            .device_list()
            .filter(|info| info.vendor_id() == vendor)
            .map(|info| info.path().to_string_lossy().into_owned())
            .collect();
        paths.sort();
        paths
    }

    fn set_logging(&mut self, on: bool) {
        self.log = if on { open_log() } else { None };
        if on {
            self.log_line("logging started");
        }
    }

    fn log_line(&mut self, text: &str) {
        if let Some(log) = &mut self.log {
            let stamp = Local::now().format("%Y-%m-%d %H:%M:%S");
            // A failed write loses one line; the next round tries again.
            let _ = writeln!(log, "{stamp}  {text}");
        }
    }
}

fn open_log() -> Option<File> {
    let path = log_path()?;
    fs::create_dir_all(path.parent()?).ok()?;
    OpenOptions::new().create(true).append(true).open(path).ok()
}

/// Sets `status.instance` from the devices already in the snapshot.
fn numbered(mut status: DeviceStatus, before: &[DeviceStatus]) -> DeviceStatus {
    status.instance = before
        .iter()
        .filter(|d| d.name == status.name && d.connection == status.connection)
        .count();
    status
}
