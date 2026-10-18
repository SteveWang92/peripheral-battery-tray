//! Battery queries for ASUS ROG wireless peripherals over HID.
//!
//! Only query commands are ever sent; nothing here writes a device setting.

use std::ffi::{CStr, CString};
use std::fmt;
use std::time::{Duration, Instant};

use hidapi::DeviceInfo;
pub use hidapi::{HidApi, HidError};

const ASUS_VID: u16 = 0x0B05;
const OMNI_PID: u16 = 0x1ACE;

/// How long a device gets to answer one command before it counts as not responding.
const REPLY_TIMEOUT: Duration = Duration::from_millis(500);

const CMD_GET: u8 = 0x12;
const SUB_PROBE: u8 = 0x00;
const SUB_KEYBOARD_BATTERY: u8 = 0x01;
const SUB_MOUSE_BATTERY: u8 = 0x07;

/// The OMNI receiver's paired-device list, read on its first collection: product IDs as
/// little-endian pairs in four-byte slots from byte 5, ended by a zero ID.
const OMNI_PAIRED: [u8; 3] = [0x01, 0xA0, 0x00];
const OMNI_PAIRED_COLLECTION: &str = "col01";

/// How a device reaches the computer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Connection {
    OmniReceiver,
    Receiver,
    Wired,
}

impl fmt::Display for Connection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Connection::OmniReceiver => "OMNI receiver",
            Connection::Receiver => "2.4 GHz receiver",
            Connection::Wired => "wired",
        })
    }
}

/// Which HID collections of a USB product can carry the control channel.
#[derive(Clone, Copy)]
enum Interface {
    /// Every collection of one USB interface; the probe finds the control one.
    Number(i32),
    /// One collection of one USB interface, as named in the HID path.
    Collection(i32, &'static str),
    /// Every collection on the vendor usage page `0xFF00`.
    VendorPage,
}

/// The control channel's HID interface and report shape.
#[derive(Clone, Copy)]
struct Channel {
    interface: Interface,
    report_id: u8,
    /// Output report length including the report ID byte.
    report_len: usize,
}

const KEYBOARD_MI1: Channel = Channel {
    interface: Interface::Number(1),
    report_id: 0x00,
    report_len: 65,
};
const KEYBOARD_VENDOR: Channel = Channel {
    interface: Interface::VendorPage,
    report_id: 0x00,
    report_len: 65,
};
const KEYBOARD_OMNI: Channel = Channel {
    interface: Interface::Collection(2, "col02"),
    report_id: 0x02,
    report_len: 64,
};
const MOUSE_MI0: Channel = Channel {
    interface: Interface::Number(0),
    report_id: 0x00,
    report_len: 65,
};
const MOUSE_MI0_64: Channel = Channel {
    interface: Interface::Number(0),
    report_id: 0x00,
    report_len: 64,
};
const MOUSE_MI1: Channel = Channel {
    interface: Interface::Number(1),
    report_id: 0x00,
    report_len: 65,
};
const MOUSE_MI2: Channel = Channel {
    interface: Interface::Number(2),
    report_id: 0x00,
    report_len: 65,
};
/// The mouse channel of the OMNI receiver, and of receivers built like it.
const MOUSE_COL03: Channel = Channel {
    interface: Interface::Collection(2, "col03"),
    report_id: 0x03,
    report_len: 64,
};

/// How the battery is asked for and read from the reply.
#[derive(Clone, Copy)]
enum Protocol {
    /// `12 01`: battery % at `battery`, charging when `[9] == 1`.
    Keyboard { battery: usize },
    /// `12 07`: battery at `battery` times `scale` (older mice report a 0–4 gauge),
    /// charging when `[10] != 0`. A mouse that stopped answering reports 0, not charging.
    Mouse { battery: usize, scale: u8 },
}

const KEYBOARD: Protocol = Protocol::Keyboard { battery: 6 };
/// The Falchion family reports a 0–10 gauge at `[6]` and the percentage at `[11]`.
const FALCHION: Protocol = Protocol::Keyboard { battery: 11 };
const MOUSE: Protocol = Protocol::Mouse {
    battery: 5,
    scale: 1,
};
const MOUSE_GAUGE: Protocol = Protocol::Mouse {
    battery: 5,
    scale: 25,
};
const STRIX_CARRY: Protocol = Protocol::Mouse {
    battery: 7,
    scale: 25,
};

/// One supported device over one connection. For [`Connection::OmniReceiver`], `pid` is
/// the ID the device has in the receiver's paired-device list; otherwise it is the USB
/// product ID.
struct Model {
    pid: u16,
    name: &'static str,
    connection: Connection,
    channel: Channel,
    protocol: Protocol,
}

const fn model(
    pid: u16,
    name: &'static str,
    connection: Connection,
    channel: Channel,
    protocol: Protocol,
) -> Model {
    Model {
        pid,
        name,
        connection,
        channel,
        protocol,
    }
}

use Connection::{OmniReceiver as Omni, Receiver, Wired};

/// Every model the protocol facts cover. Only the ROG Azoth over the OMNI receiver is
/// tested on real hardware; every other entry is untested.
const MODELS: &[Model] = &[
    // Keyboards
    model(0x1A85, "ROG Azoth", Omni, KEYBOARD_OMNI, KEYBOARD),
    model(0x1A85, "ROG Azoth", Receiver, KEYBOARD_MI1, KEYBOARD),
    model(0x1A83, "ROG Azoth", Wired, KEYBOARD_MI1, KEYBOARD),
    model(0x1B42, "ROG Azoth Extreme", Omni, KEYBOARD_OMNI, KEYBOARD),
    model(0x1B3F, "ROG Azoth Extreme", Wired, KEYBOARD_MI1, KEYBOARD),
    model(
        0x1CF1,
        "ROG Azoth Extreme SE",
        Omni,
        KEYBOARD_OMNI,
        KEYBOARD,
    ),
    model(
        0x1CEF,
        "ROG Azoth Extreme SE",
        Wired,
        KEYBOARD_MI1,
        KEYBOARD,
    ),
    model(0x1C25, "ROG Azoth X", Receiver, KEYBOARD_MI1, KEYBOARD),
    model(0x1C24, "ROG Azoth X", Wired, KEYBOARD_MI1, KEYBOARD),
    model(0x196B, "ROG Claymore II", Wired, KEYBOARD_VENDOR, KEYBOARD),
    model(0x193E, "ROG Falchion", Receiver, KEYBOARD_MI1, FALCHION),
    model(0x193C, "ROG Falchion", Wired, KEYBOARD_MI1, FALCHION),
    model(0x1B04, "ROG Falchion RX", Wired, KEYBOARD_VENDOR, FALCHION),
    model(
        0x1B06,
        "ROG Falchion RX Low Profile",
        Omni,
        KEYBOARD_OMNI,
        FALCHION,
    ),
    model(
        0x1AB0,
        "ROG Strix Scope II 96",
        Omni,
        KEYBOARD_OMNI,
        KEYBOARD,
    ),
    model(
        0x1AAE,
        "ROG Strix Scope II 96",
        Wired,
        KEYBOARD_MI1,
        KEYBOARD,
    ),
    model(
        0x1B7A,
        "ROG Strix Scope II 96 RX",
        Omni,
        KEYBOARD_OMNI,
        KEYBOARD,
    ),
    model(
        0x1B78,
        "ROG Strix Scope II 96 RX",
        Wired,
        KEYBOARD_MI1,
        KEYBOARD,
    ),
    model(
        0x1A07,
        "ROG Strix Scope RX TKL",
        Receiver,
        KEYBOARD_VENDOR,
        KEYBOARD,
    ),
    // Mice
    model(0x18E5, "ROG Chakram", Receiver, MOUSE_MI0, MOUSE_GAUGE),
    model(0x18E3, "ROG Chakram", Wired, MOUSE_MI0, MOUSE_GAUGE),
    model(0x1A1A, "ROG Chakram X", Receiver, MOUSE_MI0, MOUSE),
    model(0x1A18, "ROG Chakram X", Wired, MOUSE_MI0, MOUSE),
    model(
        0x18A0,
        "ROG Gladius II Wireless",
        Receiver,
        MOUSE_MI2,
        MOUSE_GAUGE,
    ),
    model(
        0x197F,
        "ROG Gladius III Wireless",
        Receiver,
        MOUSE_MI0,
        MOUSE,
    ),
    model(0x197D, "ROG Gladius III Wireless", Wired, MOUSE_MI0, MOUSE),
    model(0x1A72, "ROG Gladius III Aimpoint", Omni, MOUSE_COL03, MOUSE),
    model(
        0x1A72,
        "ROG Gladius III Aimpoint",
        Receiver,
        MOUSE_MI0,
        MOUSE,
    ),
    model(0x1A70, "ROG Gladius III Aimpoint", Wired, MOUSE_MI0, MOUSE),
    model(
        0x1B0C,
        "ROG Gladius III Aimpoint EVA-02",
        Receiver,
        MOUSE_MI0,
        MOUSE,
    ),
    model(
        0x1B0A,
        "ROG Gladius III Aimpoint EVA-02",
        Wired,
        MOUSE_MI0,
        MOUSE,
    ),
    model(
        0x1A94,
        "ROG Harpe Ace Aim Lab Edition",
        Omni,
        MOUSE_COL03,
        MOUSE,
    ),
    model(
        0x1A94,
        "ROG Harpe Ace Aim Lab Edition",
        Receiver,
        MOUSE_MI0,
        MOUSE,
    ),
    model(
        0x1A92,
        "ROG Harpe Ace Aim Lab Edition",
        Wired,
        MOUSE_MI0,
        MOUSE,
    ),
    model(0x1B68, "ROG Harpe Ace Extreme", Omni, MOUSE_COL03, MOUSE),
    model(0x1B69, "ROG Harpe Ace Extreme", Omni, MOUSE_COL03, MOUSE),
    model(0x1B67, "ROG Harpe Ace Extreme", Wired, MOUSE_MI0, MOUSE),
    model(0x1B65, "ROG Harpe Ace Mini", Omni, MOUSE_COL03, MOUSE),
    model(0x1B63, "ROG Harpe Ace Mini", Wired, MOUSE_MI0_64, MOUSE),
    // The Harpe II Extreme Edition 20 shares this receiver ID; see `display_name`.
    model(0x1AD0, "ROG Harpe II Ace", Receiver, MOUSE_COL03, MOUSE),
    model(0x1C69, "ROG Harpe II Ace", Wired, MOUSE_MI0_64, MOUSE),
    model(
        0x1960,
        "ROG Keris Wireless",
        Receiver,
        MOUSE_MI0,
        MOUSE_GAUGE,
    ),
    model(0x195E, "ROG Keris Wireless", Wired, MOUSE_MI0, MOUSE_GAUGE),
    model(
        0x1A59,
        "ROG Keris Wireless EVA Edition",
        Receiver,
        MOUSE_MI0,
        MOUSE_GAUGE,
    ),
    model(
        0x1A57,
        "ROG Keris Wireless EVA Edition",
        Wired,
        MOUSE_MI0,
        MOUSE_GAUGE,
    ),
    model(
        0x1A68,
        "ROG Keris Wireless Aimpoint",
        Omni,
        MOUSE_COL03,
        MOUSE,
    ),
    model(
        0x1A6A,
        "ROG Keris Wireless Aimpoint",
        Omni,
        MOUSE_COL03,
        MOUSE,
    ),
    model(
        0x1A68,
        "ROG Keris Wireless Aimpoint",
        Receiver,
        MOUSE_MI0,
        MOUSE,
    ),
    model(
        0x1A66,
        "ROG Keris Wireless Aimpoint",
        Wired,
        MOUSE_MI0,
        MOUSE,
    ),
    model(0x1B1A, "ROG Keris II Ace", Omni, MOUSE_COL03, MOUSE),
    model(0x1B18, "ROG Keris II Ace", Omni, MOUSE_COL03, MOUSE),
    model(0x1B16, "ROG Keris II Ace", Wired, MOUSE_MI0, MOUSE),
    model(0x1C0E, "ROG Keris II Origin", Omni, MOUSE_COL03, MOUSE),
    model(0x1C0C, "ROG Keris II Origin", Wired, MOUSE_MI0_64, MOUSE),
    model(0x1D4E, "ROG Keris II Origin KJP", Omni, MOUSE_COL03, MOUSE),
    model(
        0x1D4C,
        "ROG Keris II Origin KJP",
        Wired,
        MOUSE_MI0_64,
        MOUSE,
    ),
    model(0x1908, "ROG Pugio II", Receiver, MOUSE_MI0, MOUSE_GAUGE),
    model(0x1906, "ROG Pugio II", Wired, MOUSE_MI0, MOUSE_GAUGE),
    model(0x1979, "ROG Spatha X", Receiver, MOUSE_MI0, MOUSE),
    model(0x1977, "ROG Spatha X", Wired, MOUSE_MI0, MOUSE),
    model(0x18B4, "ROG Strix Carry", Receiver, MOUSE_MI1, STRIX_CARRY),
    model(
        0x1949,
        "ROG Strix Impact II Wireless",
        Receiver,
        MOUSE_MI0,
        MOUSE_GAUGE,
    ),
    model(
        0x1947,
        "ROG Strix Impact II Wireless",
        Wired,
        MOUSE_MI0,
        MOUSE_GAUGE,
    ),
    model(
        0x1AD7,
        "ROG Strix Impact III Wireless",
        Omni,
        MOUSE_COL03,
        MOUSE,
    ),
];

/// A device whose control channel was found.
pub struct Device {
    model: &'static Model,
    name: &'static str,
    path: CString,
}

/// The result of one battery query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reading {
    Battery {
        percent: u8,
        charging: bool,
    },
    /// The receiver or device gave no answer: the device is asleep, off or out of range.
    Asleep,
}

impl fmt::Display for Reading {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Reading::Battery { percent, charging } => {
                let state = if *charging {
                    "charging"
                } else {
                    "not charging"
                };
                write!(f, "{percent}%, {state}")
            }
            Reading::Asleep => f.write_str("asleep or not responding"),
        }
    }
}

impl Device {
    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn connection(&self) -> Connection {
        self.model.connection
    }

    /// The HID path the device is queried on, which tells identical devices apart for as
    /// long as they stay in the same port.
    pub fn id(&self) -> String {
        self.path.to_string_lossy().into_owned()
    }

    pub fn read_battery(&self, api: &HidApi) -> Result<Reading, HidError> {
        let channel = &self.model.channel;
        let sub = match self.model.protocol {
            Protocol::Keyboard { .. } => SUB_KEYBOARD_BATTERY,
            Protocol::Mouse { .. } => SUB_MOUSE_BATTERY,
        };
        let reply = match query(api, &self.path, channel, [channel.report_id, CMD_GET, sub])? {
            Some(reply) if !is_no_answer(&reply) => reply,
            _ => return Ok(Reading::Asleep),
        };
        Ok(match self.model.protocol {
            Protocol::Keyboard { battery } => Reading::Battery {
                percent: reply[battery],
                charging: reply[9] == 1,
            },
            Protocol::Mouse { battery, scale } => {
                let percent = reply[battery].saturating_mul(scale).min(100);
                let charging = reply[10] != 0;
                if percent == 0 && !charging {
                    Reading::Asleep
                } else {
                    Reading::Battery { percent, charging }
                }
            }
        })
    }
}

/// Finds every connected supported device: USB products by probing their candidate
/// interfaces, and devices behind an OMNI receiver from its paired-device list.
pub fn discover(api: &HidApi) -> Vec<Device> {
    let asus: Vec<&DeviceInfo> = api
        .device_list()
        .filter(|info| info.vendor_id() == ASUS_VID)
        .collect();
    let mut devices = Vec::new();
    for info in &asus {
        if info.product_id() == OMNI_PID {
            devices.extend(omni_devices(api, info, &asus));
            continue;
        }
        let Some(model) = MODELS.iter().find(|m| {
            m.connection != Connection::OmniReceiver
                && m.pid == info.product_id()
                && matches(&m.channel, info)
        }) else {
            continue;
        };
        let path = info.path().to_owned();
        if probe(api, model, &path) {
            devices.push(Device {
                model,
                name: display_name(model, info),
                path,
            });
        }
    }
    // HID enumeration order can change after a replug; callers that show one entry per
    // device get the same order every time, identical devices included.
    devices.sort_by(|a, b| {
        (a.name, a.model.connection, &a.path).cmp(&(b.name, b.model.connection, &b.path))
    });
    devices
}

/// The devices one OMNI receiver lists as paired, each on its own channel of that receiver.
/// A paired device that is switched off still counts; its queries go unanswered.
fn omni_devices(api: &HidApi, paired_info: &DeviceInfo, asus: &[&DeviceInfo]) -> Vec<Device> {
    let paired_channel = Channel {
        interface: Interface::Collection(2, OMNI_PAIRED_COLLECTION),
        report_id: OMNI_PAIRED[0],
        report_len: 64,
    };
    if !matches(&paired_channel, paired_info) {
        return Vec::new();
    }
    let Ok(Some(reply)) = query(api, paired_info.path(), &paired_channel, OMNI_PAIRED) else {
        return Vec::new();
    };
    let receiver = instance(paired_info.path());
    reply[5..]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|slot| u16::from_le_bytes([slot[0], slot[1]]))
        .take_while(|&pid| pid != 0)
        .filter_map(|pid| {
            let model = MODELS
                .iter()
                .find(|m| m.connection == Connection::OmniReceiver && m.pid == pid)?;
            let info = asus.iter().find(|info| {
                info.product_id() == OMNI_PID
                    && instance(info.path()) == receiver
                    && matches(&model.channel, info)
            })?;
            Some(Device {
                model,
                name: model.name,
                path: info.path().to_owned(),
            })
        })
        .collect()
}

fn display_name(model: &Model, info: &DeviceInfo) -> &'static str {
    let extreme = info
        .product_string()
        .is_some_and(|name| name.to_ascii_uppercase().contains("EXTREME"));
    if model.pid == 0x1AD0 && extreme {
        "ROG Harpe II Extreme Edition 20"
    } else {
        model.name
    }
}

fn matches(channel: &Channel, info: &DeviceInfo) -> bool {
    match channel.interface {
        Interface::Number(number) => info.interface_number() == number,
        Interface::Collection(number, name) => {
            info.interface_number() == number && collection(info) == Some(name)
        }
        Interface::VendorPage => info.usage_page() == 0xFF00,
    }
}

/// Whether the collection at `path` is the model's control channel. A collection that
/// fails to open or answers with the wrong shape is not; the right one sits beside it in
/// the same enumeration. A device that does not answer at all is asleep behind its receiver.
fn probe(api: &HidApi, model: &Model, path: &CStr) -> bool {
    let channel = &model.channel;
    match model.protocol {
        Protocol::Keyboard { .. } => {
            let command = [channel.report_id, CMD_GET, SUB_PROBE];
            matches!(query(api, path, channel, command), Ok(Some(reply))
                if is_no_answer(&reply) || reply[5] != 0xFA)
        }
        Protocol::Mouse { .. } => {
            let command = [channel.report_id, CMD_GET, SUB_MOUSE_BATTERY];
            matches!(query(api, path, channel, command), Ok(Some(_)))
        }
    }
}

/// The collection part of a Windows HID path, such as `col02` in
/// `\\?\hid#vid_0b05&pid_1ace&mi_02&col02#...`, in lower case.
fn collection(info: &DeviceInfo) -> Option<&'static str> {
    let path = info.path().to_string_lossy().to_ascii_lowercase();
    let id = path.split('#').nth(1)?;
    ["col01", "col02", "col03"]
        .into_iter()
        .find(|name| id.ends_with(&format!("&{name}")))
}

/// The part of a Windows HID path shared by every collection of one USB interface, such
/// as `8&2a1b3c&0` in `...#8&2a1b3c&0&0001#{...}`, in lower case.
fn instance(path: &CStr) -> Option<String> {
    let path = path.to_string_lossy().to_ascii_lowercase();
    let id = path.split('#').nth(2)?;
    Some(id.rsplit_once('&').map_or(id, |(head, _)| head).to_owned())
}

/// A receiver's reply when the device behind it did not answer.
fn is_no_answer(reply: &[u8]) -> bool {
    reply[1] == 0xFF && reply[2] == 0xAA
}

/// Sends `command` (report ID, command, subcommand) and waits for its reply, returned with
/// the report ID at index 0. `None` means nothing matching arrived in time.
fn query(
    api: &HidApi,
    path: &CStr,
    channel: &Channel,
    command: [u8; 3],
) -> Result<Option<Vec<u8>>, HidError> {
    let device = api.open_path(path)?;
    let mut packet = vec![0u8; channel.report_len];
    packet[..3].copy_from_slice(&command);
    device.write(&packet)?;

    // hidapi drops the report ID from unnumbered (ID 0) reports, so the buffer starts one
    // byte in for them and every reply keeps the same offsets.
    let offset = usize::from(channel.report_id == 0);
    let mut buf = vec![0u8; channel.report_len];
    let deadline = Instant::now() + REPLY_TIMEOUT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Ok(None);
        }
        let n = device.read_timeout(&mut buf[offset..], left.as_millis() as i32)?;
        if n == 0 {
            return Ok(None);
        }
        // Other input reports can share the interface; skip anything that is not this
        // command's reply.
        let reply = &buf[..offset + n];
        if reply.len() >= 12 && (reply[1..3] == command[1..] || is_no_answer(reply)) {
            return Ok(Some(reply.to_vec()));
        }
    }
}
