//! Battery queries for Logitech wireless peripherals over HID++.
//!
//! Only query commands are ever sent; nothing here writes a device setting or a receiver
//! register.

use std::cell::{Cell, RefCell};
use std::ffi::{CStr, CString};
use std::fmt;
use std::time::{Duration, Instant};

use hidapi::{DeviceInfo, HidDevice};
pub use hidapi::{HidApi, HidError};

const LOGITECH_VID: u16 = 0x046D;

/// The USB interface that carries HID++ on every receiver.
const HIDPP_INTERFACE: i32 = 2;
const VENDOR_PAGE: u16 = 0xFF00;
/// Windows splits the HID++ interface into one collection per report length and accepts a
/// report only on the collection that declares it.
const SHORT_USAGE: u16 = 0x0001;
const LONG_USAGE: u16 = 0x0002;

/// Requests go out as short reports; replies come back short or long (`0x11`, 20 bytes).
const SHORT: u8 = 0x10;
const SHORT_LEN: usize = 7;
const LONG_LEN: usize = 20;

/// The receiver's own index; paired devices sit at indices 1 to 6 behind it.
const RECEIVER_INDEX: u8 = 0xFF;
const SLOTS: std::ops::RangeInclusive<u8> = 1..=6;

/// Marks this program's requests, so replies to other HID++ clients sharing the receiver
/// (G HUB uses `0x0D`, Solaar `0x0B`) are told apart.
const SOFTWARE_ID: u8 = 0x0C;

/// How long a device gets to answer one request before it counts as not responding. An
/// idle G502 X takes over half a second to answer its first request.
const REPLY_TIMEOUT: Duration = Duration::from_secs(2);

/// HID++ 1.0 error reply sub-ID, sent by the receiver on behalf of a slot.
const HIDPP10_ERROR: u8 = 0x8F;
/// HID++ 2.0 error reply feature index, sent by the device itself.
const HIDPP20_ERROR: u8 = 0xFF;
/// The HID++ 1.0 error a device that only speaks HID++ 1.0 gives to a HID++ 2.0 request.
const ERR_INVALID_SUB_ID: u8 = 0x01;

/// HID++ 1.0 long register read, and the receiver information register it reads. Its
/// sub-register `0x40 + slot - 1` (Bolt: `0x60 + slot`, then `0x01`) holds the shortened
/// name of the device paired in that slot, and answers while that device is switched off.
const READ_LONG_REGISTER: u8 = 0x83;
const RECEIVER_INFO: u8 = 0xB5;
const PAIRED_NAME: u8 = 0x40;
const BOLT_PAIRED_NAME: u8 = 0x60;

const ROOT: u8 = 0x00;
const ROOT_GET_FEATURE: u8 = 0;

const FEATURE_DEVICE_NAME: u16 = 0x0005;
const FEATURE_BATTERY_STATUS: u16 = 0x1000;
const FEATURE_BATTERY_VOLTAGE: u16 = 0x1001;
const FEATURE_UNIFIED_BATTERY: u16 = 0x1004;
const FEATURE_ADC_MEASUREMENT: u16 = 0x1F20;

/// How a device reaches the computer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Connection {
    Lightspeed,
    Unifying,
    Bolt,
}

impl fmt::Display for Connection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Connection::Lightspeed => "Lightspeed receiver",
            Connection::Unifying => "Unifying receiver",
            Connection::Bolt => "Bolt receiver",
        })
    }
}

const RECEIVERS: &[(u16, Connection)] = &[
    (0xC539, Connection::Lightspeed),
    (0xC53A, Connection::Lightspeed),
    (0xC53D, Connection::Lightspeed),
    (0xC53F, Connection::Lightspeed),
    (0xC541, Connection::Lightspeed),
    (0xC545, Connection::Lightspeed),
    (0xC547, Connection::Lightspeed),
    (0xC54D, Connection::Lightspeed),
    (0xC52B, Connection::Unifying),
    (0xC532, Connection::Unifying),
    (0xC548, Connection::Bolt),
];

/// The HID++ 2.0 battery features, in the order they are tried.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BatteryFeature {
    /// Function 1: `[0]` state of charge %, `[1]` level bits, `[2]` charging status.
    Unified,
    /// Function 0: `[0]` level %, `[1]` next level, `[2]` charging status.
    Status,
    /// Function 0: `[0..2]` millivolts, `[2]` flags with bit 7 set while charging.
    Voltage,
    /// Function 0: `[0..2]` millivolts, `[2]` flags: bit 0 valid, bit 1 charging.
    Adc,
}

const BATTERY_FEATURES: [(u16, BatteryFeature); 4] = [
    (FEATURE_UNIFIED_BATTERY, BatteryFeature::Unified),
    (FEATURE_BATTERY_STATUS, BatteryFeature::Status),
    (FEATURE_BATTERY_VOLTAGE, BatteryFeature::Voltage),
    (FEATURE_ADC_MEASUREMENT, BatteryFeature::Adc),
];

/// A device paired to a receiver, reachable through that receiver's HID++ collections.
pub struct Device {
    connection: Connection,
    short_path: CString,
    long_path: CString,
    index: u8,
    /// The receiver's shortened name until the device first answers, then its full name.
    name: RefCell<String>,
    /// The battery feature and its index on the device, looked up on first contact and
    /// again after a query to it fails.
    battery: Cell<Option<(BatteryFeature, u8)>>,
}

/// The result of one battery query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reading {
    Battery {
        percent: u8,
        charging: bool,
        /// The percentage is derived from a voltage or a coarse level rather than reported.
        estimated: bool,
    },
    /// The receiver answered for the device: it is asleep, off or out of range.
    Asleep,
}

impl fmt::Display for Reading {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Reading::Battery {
                percent,
                charging,
                estimated,
            } => {
                let about = if *estimated { "about " } else { "" };
                let state = if *charging {
                    "charging"
                } else {
                    "not charging"
                };
                write!(f, "{about}{percent}%, {state}")
            }
            Reading::Asleep => f.write_str("asleep or not responding"),
        }
    }
}

impl Device {
    pub fn name(&self) -> String {
        self.name.borrow().clone()
    }

    pub fn connection(&self) -> Connection {
        self.connection
    }

    pub fn read_battery(&self, api: &HidApi) -> Result<Reading, HidError> {
        let channel = Channel::open(api, &self.short_path, &self.long_path)?;
        let battery = match self.battery.get() {
            Some(battery) => battery,
            None => match battery_feature(&channel, self.index)? {
                Answer::Reply(Some(battery)) => {
                    if let Some(name) = device_name(&channel, self.index)? {
                        *self.name.borrow_mut() = name;
                    }
                    battery
                }
                // A device that reports no battery has nothing to show either.
                _ => return Ok(Reading::Asleep),
            },
        };
        let (feature, feature_index) = battery;
        let function = match feature {
            BatteryFeature::Unified => 1,
            _ => 0,
        };
        let data = match channel.call(self.index, feature_index, function, &[])? {
            Answer::Reply(data) => data,
            Answer::Unreachable => return Ok(Reading::Asleep),
            Answer::Failed(_) => {
                // The feature index belongs to the firmware that was running; look it up
                // again next time.
                self.battery.set(None);
                return Ok(Reading::Asleep);
            }
        };
        self.battery.set(Some(battery));
        Ok(decode(feature, &data))
    }
}

fn decode(feature: BatteryFeature, data: &[u8]) -> Reading {
    match feature {
        BatteryFeature::Unified => {
            let (soc, level, status) = (data[0], data[1], data[2]);
            let charging = matches!(status, 1..=4);
            if soc != 0 {
                return Reading::Battery {
                    percent: soc.min(100),
                    charging,
                    estimated: false,
                };
            }
            let percent = match level {
                l if l & 8 != 0 => 100,
                l if l & 4 != 0 => 50,
                l if l & 2 != 0 => 20,
                l if l & 1 != 0 => 5,
                _ => return Reading::Asleep,
            };
            Reading::Battery {
                percent,
                charging,
                estimated: true,
            }
        }
        BatteryFeature::Status => {
            if data[0] == 0 {
                return Reading::Asleep;
            }
            Reading::Battery {
                percent: data[0].min(100),
                charging: matches!(data[2], 1..=4),
                estimated: false,
            }
        }
        BatteryFeature::Voltage => Reading::Battery {
            percent: voltage_percent(u16::from_be_bytes([data[0], data[1]])),
            charging: data[2] & 0x80 != 0,
            estimated: true,
        },
        BatteryFeature::Adc => {
            if data[2] & 0x01 == 0 {
                return Reading::Asleep;
            }
            Reading::Battery {
                percent: voltage_percent(u16::from_be_bytes([data[0], data[1]])),
                charging: data[2] & 0x02 != 0,
                estimated: true,
            }
        }
    }
}

/// A lithium cell's charge from its voltage, interpolated between measured points.
fn voltage_percent(millivolts: u16) -> u8 {
    const CURVE: [(u16, u8); 13] = [
        (4186, 100),
        (4067, 90),
        (3989, 80),
        (3922, 70),
        (3859, 60),
        (3811, 50),
        (3778, 40),
        (3751, 30),
        (3717, 20),
        (3671, 10),
        (3646, 5),
        (3579, 2),
        (3500, 0),
    ];
    if millivolts >= CURVE[0].0 {
        return 100;
    }
    for pair in CURVE.windows(2) {
        let ((high_mv, high), (low_mv, low)) = (pair[0], pair[1]);
        if millivolts >= low_mv {
            let span = u32::from(high - low) * u32::from(millivolts - low_mv);
            return low + (span / u32::from(high_mv - low_mv)) as u8;
        }
    }
    0
}

/// Finds every device paired to a supported receiver, including one that is switched off:
/// it reads as asleep until it answers.
pub fn discover(api: &HidApi) -> Vec<Device> {
    let logitech: Vec<&DeviceInfo> = api
        .device_list()
        .filter(|info| info.vendor_id() == LOGITECH_VID)
        .collect();
    let mut devices = Vec::new();
    for short in &logitech {
        let Some(&(_, connection)) = RECEIVERS.iter().find(|(pid, _)| *pid == short.product_id())
        else {
            continue;
        };
        if !is_collection(short, SHORT_USAGE) {
            continue;
        }
        let Some(long) = logitech.iter().find(|info| {
            info.product_id() == short.product_id()
                && is_collection(info, LONG_USAGE)
                && instance(info.path()) == instance(short.path())
        }) else {
            continue;
        };
        let Ok(channel) = Channel::open(api, short.path(), long.path()) else {
            continue;
        };
        for index in SLOTS {
            let Ok(Some(paired)) = paired_name(&channel, connection, index) else {
                continue;
            };
            // A device that answers without a battery feature, or only speaks HID++ 1.0,
            // has nothing to show; any other device is listed, and one whose probe did not
            // get through is looked up again when it is first read.
            let battery = match battery_feature(&channel, index) {
                Ok(Answer::Reply(None) | Answer::Failed(ERR_INVALID_SUB_ID)) => continue,
                Ok(Answer::Reply(Some(battery))) => Some(battery),
                _ => None,
            };
            let name = match battery {
                Some(_) => device_name(&channel, index).ok().flatten(),
                None => None,
            };
            devices.push(Device {
                connection,
                short_path: short.path().to_owned(),
                long_path: long.path().to_owned(),
                index,
                name: RefCell::new(name.unwrap_or(paired)),
                battery: Cell::new(battery),
            });
        }
    }
    // HID enumeration order can change after a replug; callers that show one entry per
    // device get the same order every time, identical devices included.
    devices.sort_by_cached_key(|device| {
        (
            device.name(),
            device.connection,
            device.short_path.clone(),
            device.index,
        )
    });
    devices
}

fn is_collection(info: &DeviceInfo, usage: u16) -> bool {
    info.interface_number() == HIDPP_INTERFACE
        && info.usage_page() == VENDOR_PAGE
        && info.usage() == usage
}

/// The part of a Windows HID path shared by every collection of one USB interface, such
/// as `8&2a1b3c&0` in `...#8&2a1b3c&0&0001#{...}`, in lower case.
fn instance(path: &CStr) -> Option<String> {
    let path = path.to_string_lossy().to_ascii_lowercase();
    let id = path.split('#').nth(2)?;
    Some(id.rsplit_once('&').map_or(id, |(head, _)| head).to_owned())
}

/// The shortened name the receiver keeps for the device paired in `slot`, or `None` for an
/// empty slot.
fn paired_name(
    channel: &Channel,
    connection: Connection,
    slot: u8,
) -> Result<Option<String>, HidError> {
    let (params, length_at) = match connection {
        Connection::Bolt => ([BOLT_PAIRED_NAME + slot, 0x01], 2),
        _ => ([PAIRED_NAME + slot - 1, 0x00], 1),
    };
    let Answer::Reply(data) = channel.read_register(params)? else {
        return Ok(None);
    };
    let length = usize::from(data[length_at]).min(data.len() - length_at - 1);
    let name = &data[length_at + 1..length_at + 1 + length];
    Ok(Some(String::from_utf8_lossy(name).trim().to_owned()))
}

/// The device's full name from feature `0x0005`, or `None` when it cannot tell.
fn device_name(channel: &Channel, index: u8) -> Result<Option<String>, HidError> {
    let Answer::Reply(Some(feature)) = feature_index(channel, index, FEATURE_DEVICE_NAME)? else {
        return Ok(None);
    };
    let Answer::Reply(count) = channel.call(index, feature, 0, &[])? else {
        return Ok(None);
    };
    let length = usize::from(count[0]);
    let mut name = Vec::with_capacity(length);
    while name.len() < length {
        let Answer::Reply(chunk) = channel.call(index, feature, 1, &[name.len() as u8])? else {
            return Ok(None);
        };
        let take = (length - name.len()).min(chunk.len());
        name.extend_from_slice(&chunk[..take]);
    }
    let name = String::from_utf8_lossy(&name)
        .trim_end_matches('\0')
        .trim()
        .to_owned();
    Ok(Some(name))
}

fn battery_feature(
    channel: &Channel,
    index: u8,
) -> Result<Answer<Option<(BatteryFeature, u8)>>, HidError> {
    for (id, feature) in BATTERY_FEATURES {
        match feature_index(channel, index, id)? {
            Answer::Reply(Some(at)) => return Ok(Answer::Reply(Some((feature, at)))),
            Answer::Reply(None) => {}
            Answer::Unreachable => return Ok(Answer::Unreachable),
            Answer::Failed(code) => return Ok(Answer::Failed(code)),
        }
    }
    Ok(Answer::Reply(None))
}

/// The index of feature `id` on the device, `None` when it does not have it.
fn feature_index(channel: &Channel, index: u8, id: u16) -> Result<Answer<Option<u8>>, HidError> {
    let [high, low] = id.to_be_bytes();
    Ok(
        match channel.call(index, ROOT, ROOT_GET_FEATURE, &[high, low])? {
            Answer::Reply(data) => Answer::Reply((data[0] != 0).then_some(data[0])),
            Answer::Unreachable => Answer::Unreachable,
            Answer::Failed(code) => Answer::Failed(code),
        },
    )
}

/// What came back for one request.
enum Answer<T> {
    Reply(T),
    /// The receiver answered that the device in that slot is off, asleep or not paired, or
    /// nothing answered in time.
    Unreachable,
    /// The device rejected the request with this HID++ 2.0 error code, or does not speak
    /// HID++ 2.0 at all (`ERR_INVALID_SUB_ID`).
    Failed(u8),
}

/// One receiver's HID++ collections, opened for a series of requests.
struct Channel {
    short: HidDevice,
    long: HidDevice,
}

impl Channel {
    fn open(api: &HidApi, short: &CStr, long: &CStr) -> Result<Self, HidError> {
        Ok(Self {
            short: api.open_path(short)?,
            long: api.open_path(long)?,
        })
    }

    /// Calls `function` of the feature at `feature` on the device in slot `index`.
    fn call(
        &self,
        index: u8,
        feature: u8,
        function: u8,
        params: &[u8],
    ) -> Result<Answer<Vec<u8>>, HidError> {
        self.request([index, feature, function << 4 | SOFTWARE_ID], params, 0)
    }

    /// Reads receiver information sub-register `params[0]`. The reply starts with that
    /// sub-register, which tells it apart from replies to reads of other sub-registers.
    fn read_register(&self, params: [u8; 2]) -> Result<Answer<Vec<u8>>, HidError> {
        self.request(
            [RECEIVER_INDEX, READ_LONG_REGISTER, RECEIVER_INFO],
            &params,
            1,
        )
    }

    /// Sends a short request, `header` being the slot index and the two bytes naming the
    /// request, and returns the reply from its fifth byte on. A reply must also repeat the
    /// first `echo` parameters.
    fn request(
        &self,
        header: [u8; 3],
        params: &[u8],
        echo: usize,
    ) -> Result<Answer<Vec<u8>>, HidError> {
        let mut packet = [0u8; SHORT_LEN];
        packet[0] = SHORT;
        packet[1..4].copy_from_slice(&header);
        packet[4..4 + params.len()].copy_from_slice(params);
        self.short.write(&packet)?;

        let deadline = Instant::now() + REPLY_TIMEOUT;
        let mut buf = [0u8; LONG_LEN];
        while !deadline.saturating_duration_since(Instant::now()).is_zero() {
            // Devices and long registers reply on the long collection; the receiver reports
            // a slot it cannot reach on the short one.
            for (device, wait) in [(&self.long, 20), (&self.short, 0)] {
                let n = device.read_timeout(&mut buf, wait)?;
                if n < SHORT_LEN || buf[1] != header[0] {
                    continue;
                }
                let reply = &buf[..n];
                match reply[2] {
                    HIDPP10_ERROR if reply[3..5] == header[1..] => {
                        return Ok(match reply[5] {
                            ERR_INVALID_SUB_ID => Answer::Failed(ERR_INVALID_SUB_ID),
                            _ => Answer::Unreachable,
                        });
                    }
                    HIDPP20_ERROR if reply[3..5] == header[1..] => {
                        return Ok(Answer::Failed(reply[5]));
                    }
                    _ if reply[2..4] == header[1..] && reply[4..4 + echo] == params[..echo] => {
                        return Ok(Answer::Reply(reply[4..].to_vec()));
                    }
                    _ => {}
                }
            }
        }
        Ok(Answer::Unreachable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voltage_maps_onto_the_discharge_curve() {
        assert_eq!(voltage_percent(4200), 100);
        assert_eq!(voltage_percent(3811), 50);
        assert_eq!(voltage_percent(3795), 45);
        assert_eq!(voltage_percent(3400), 0);
    }
}
