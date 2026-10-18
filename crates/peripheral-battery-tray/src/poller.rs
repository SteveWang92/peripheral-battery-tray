//! Battery polling on a background thread, so a device that is slow to answer never blocks
//! the tray. Each round's results go to the tray as a [`Snapshot`] and, when logging is on,
//! to the log file.

use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use logitech_battery::{HidApi, HidError};

use crate::discharge::History;

const ASUS_VID: u16 = 0x0B05;

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
    /// Tells identical devices apart: the HID path for ROG, the receiver path and slot for
    /// Logitech.
    pub id: String,
    /// A failed query counts as not responding; its error goes to the log.
    pub reading: Reading,
    /// The estimated time left on battery, once the drain since the last charge allows one.
    pub remaining: Option<Duration>,
}

/// Identifies one device across snapshots.
pub type DeviceKey = (String, Connection);

impl DeviceStatus {
    pub fn key(&self) -> DeviceKey {
        (self.id.clone(), self.connection)
    }

    /// What alerts are kept per. A ROG device charging over its cable appears as a wired
    /// entry beside its receiver entry, so ROG alerts go by model; identical ROG devices
    /// therefore share theirs.
    pub fn alert_key(&self) -> &str {
        match self.connection {
            Connection::Rog(_) => &self.name,
            Connection::Logitech(_) => &self.id,
        }
    }
}

/// A discovered device of any brand.
#[derive(Clone, Copy)]
pub enum Device<'a> {
    Rog(&'a rog_battery::Device),
    Logitech(&'a logitech_battery::Device),
}

/// Every device of every brand, one brand after another.
pub fn every_device<'a>(
    rog: &'a [rog_battery::Device],
    logitech: &'a [logitech_battery::Device],
) -> impl Iterator<Item = Device<'a>> {
    rog.iter()
        .map(Device::Rog)
        .chain(logitech.iter().map(Device::Logitech))
}

/// The outcome of one battery query.
pub struct Query {
    pub status: DeviceStatus,
    /// The device, its connection and its reading or error, as `--once` prints it and the
    /// log records it.
    pub line: String,
    pub failed: bool,
}

impl Device<'_> {
    pub fn query(self, api: &HidApi) -> Query {
        match self {
            Device::Rog(device) => {
                let result = device.read_battery(api);
                let connection = device.connection();
                describe(
                    device.name().to_owned(),
                    device.id(),
                    connection,
                    Connection::Rog(connection),
                    result,
                )
            }
            Device::Logitech(device) => {
                let result = device.read_battery(api);
                // Read after the query: a device's first answer replaces its shortened name.
                let connection = device.connection();
                describe(
                    device.name(),
                    device.id(),
                    connection,
                    Connection::Logitech(connection),
                    result,
                )
            }
        }
    }
}

fn describe<R: fmt::Display + Into<Reading>>(
    name: String,
    id: String,
    shown: impl fmt::Display,
    connection: Connection,
    result: Result<R, HidError>,
) -> Query {
    let text = match &result {
        Ok(reading) => reading.to_string(),
        Err(error) => format!("query failed: {error}"),
    };
    Query {
        line: format!("{name} ({shown}): {text}"),
        failed: result.is_err(),
        status: DeviceStatus {
            name,
            id,
            connection,
            reading: result.map_or(Reading::Asleep, Into::into),
            remaining: None,
        },
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
            logitech_pairings: Vec::new(),
            shown: HashMap::new(),
            doubted: HashSet::new(),
            history: History::load(),
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
    /// The receiver pairings present at the last Logitech discovery.
    logitech_pairings: Vec<(CString, u8)>,
    /// Each device's last battery percentage passed on to the tray.
    shown: HashMap<DeviceKey, u8>,
    /// Devices whose last reading was a sudden drop that was held back.
    doubted: HashSet<DeviceKey>,
    history: History,
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
        // Discovery probes every device over the air, so each brand's runs only when its
        // set of HID interfaces or receiver pairings changes, or one of its queries failed;
        // otherwise a round sends one battery query per device, plus a pairing read per
        // receiver slot that the receiver answers itself.
        if let Err(error) = self.api.refresh_devices() {
            self.log_line(&format!("device refresh failed: {error}"));
        }
        let rog_paths = self.paths(ASUS_VID);
        if self.rog.is_empty() || rog_paths != self.rog_paths {
            self.rog = rog_battery::discover(&self.api);
            self.rog_paths = rog_paths;
        }
        // A device paired to a receiver already plugged in changes no HID path. Pairings
        // that cannot be read count as unchanged.
        let pairings = logitech_battery::pairings(&self.api);
        if let Err(error) = &pairings {
            self.log_line(&format!("receiver pairing read failed: {error}"));
        }
        let paired_anew = pairings
            .as_ref()
            .is_ok_and(|pairings| *pairings != self.logitech_pairings);
        if self.logitech.is_empty() || paired_anew {
            self.logitech = logitech_battery::discover(&self.api);
        }
        if let Ok(pairings) = pairings {
            self.logitech_pairings = pairings;
        }
        if self.rog.is_empty() && self.logitech.is_empty() {
            self.log_line("no supported device found");
        }

        let mut queries = Vec::with_capacity(self.rog.len() + self.logitech.len());
        for device in every_device(&self.rog, &self.logitech) {
            let started = Instant::now();
            let query = device.query(&self.api);
            queries.push((query, started.elapsed()));
        }
        let at = Local::now();
        let mut devices = Vec::with_capacity(queries.len());
        for (query, elapsed) in queries {
            self.log_line(&format!("{}  [{} ms]", query.line, elapsed.as_millis()));
            // The brand's devices are discovered again next round.
            if query.failed {
                match query.status.connection {
                    Connection::Rog(_) => self.rog.clear(),
                    Connection::Logitech(_) => self.logitech.clear(),
                }
            }
            let mut status = self.steady(query.status);
            // A percentage derived from a voltage or a coarse level moves too unevenly to
            // time.
            if let Reading::Battery {
                percent,
                charging,
                estimated: false,
            } = status.reading
            {
                let device = format!("{:?} {}", status.connection, status.id);
                status.remaining = self
                    .history
                    .record(&device, percent, charging, at.timestamp());
            }
            devices.push(status);
        }
        self.history.save();
        Snapshot { devices, at }
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
