//! User-facing text in English and Simplified Chinese.

use crate::poller::Connection;
use serde::{Deserialize, Serialize};
use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;

/// The language setting: follow Windows, or one fixed language.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Language {
    #[default]
    System,
    English,
    #[serde(rename = "zh-CN")]
    Chinese,
}

impl Language {
    pub fn strings(self) -> &'static Strings {
        const LANG_CHINESE: u16 = 0x04;
        let chinese = match self {
            Language::System => (unsafe { GetUserDefaultUILanguage() } & 0x3FF) == LANG_CHINESE,
            Language::English => false,
            Language::Chinese => true,
        };
        if chinese { &ZH } else { &EN }
    }
}

pub struct Strings {
    pub refresh: &'static str,
    pub low_alert: &'static str,
    pub off: &'static str,
    pub interval: &'static str,
    seconds: &'static str,
    minutes: &'static str,
    pub icons: &'static str,
    pub icons_each: &'static str,
    pub icons_merged: &'static str,
    pub full_alert: &'static str,
    pub autostart: &'static str,
    pub language: &'static str,
    pub follow_system: &'static str,
    pub battery_log: &'static str,
    pub about: &'static str,
    pub quit: &'static str,
    pub no_device: &'static str,
    pub supported: &'static str,
    pub no_response: &'static str,
    pub asleep: &'static str,
    asleep_short: &'static str,
    row_asleep: &'static str,
    pub card_title: &'static str,
    charging: &'static str,
    not_charging: &'static str,
    updated: &'static str,
    receiver: &'static str,
    wired: &'static str,
    low_title: &'static str,
    low_body: &'static str,
    full_title: &'static str,
    full_body: &'static str,
    pub bad_config: &'static str,
}

/// `{}` in a template is replaced, in order, by `args`.
fn fill(template: &str, args: &[&str]) -> String {
    let mut out = String::new();
    let mut parts = template.split("{}");
    out.push_str(parts.next().unwrap_or(""));
    for (part, arg) in parts.zip(args) {
        out.push_str(arg);
        out.push_str(part);
    }
    out
}

impl Strings {
    pub fn interval_label(&self, secs: u64) -> String {
        if secs < 60 {
            fill(self.seconds, &[&secs.to_string()])
        } else {
            fill(self.minutes, &[&(secs / 60).to_string()])
        }
    }

    /// The connection's short name, for the menu.
    pub fn connection(&self, connection: Connection) -> &'static str {
        match connection {
            Connection::Rog(rog_battery::Connection::OmniReceiver) => "OMNI",
            Connection::Rog(rog_battery::Connection::Receiver) => "2.4 GHz",
            Connection::Rog(rog_battery::Connection::Wired) => self.wired,
            Connection::Logitech(logitech_battery::Connection::Lightspeed) => "Lightspeed",
            Connection::Logitech(logitech_battery::Connection::Unifying) => "Unifying",
            Connection::Logitech(logitech_battery::Connection::Bolt) => "Bolt",
        }
    }

    /// The connection in full, for the card.
    pub fn connection_detail(&self, connection: Connection) -> String {
        match connection {
            Connection::Rog(rog_battery::Connection::Wired) => self.wired.to_owned(),
            _ => fill(self.receiver, &[self.connection(connection)]),
        }
    }

    /// The card's status line under a device's battery bar.
    pub fn live_status(&self, charging: bool, time: &str) -> String {
        let state = if charging {
            self.charging
        } else {
            self.not_charging
        };
        format!("{state} · {}", self.updated_at(time))
    }

    /// The card's status line for a device asleep since its last reading at `time`.
    pub fn asleep_status(&self, time: &str) -> String {
        format!("{} · {}", self.asleep_short, self.updated_at(time))
    }

    /// A device's battery in the menu.
    pub fn row_status(&self, percent: u8, charging: bool) -> String {
        if charging {
            format!("{percent}% {}", self.charging)
        } else {
            format!("{percent}%")
        }
    }

    /// A device's battery in the menu, asleep since its last reading.
    pub fn row_asleep(&self, percent: u8) -> String {
        fill(self.row_asleep, &[&percent.to_string()])
    }

    fn updated_at(&self, time: &str) -> String {
        fill(self.updated, &[time])
    }

    pub fn low_toast(&self, device: &str, percent: u8) -> (String, String) {
        (
            fill(self.low_title, &[device]),
            fill(self.low_body, &[&percent.to_string()]),
        )
    }

    pub fn full_toast(&self, device: &str) -> (String, String) {
        (fill(self.full_title, &[device]), self.full_body.to_owned())
    }
}

static EN: Strings = Strings {
    refresh: "Refresh now",
    low_alert: "Low battery alert",
    off: "Off",
    interval: "Refresh interval",
    seconds: "{} s",
    minutes: "{} min",
    icons: "Tray icons",
    icons_each: "One per device",
    icons_merged: "One merged (shows the lowest)",
    full_alert: "Full charge alert",
    autostart: "Start with Windows",
    language: "Language",
    follow_system: "Follow system",
    battery_log: "Battery log",
    about: "About peripheral-battery-tray",
    quit: "Exit",
    no_device: "No supported device found",
    supported: "Supported devices…",
    no_response: "No response",
    asleep: "Asleep or not responding",
    asleep_short: "Asleep",
    row_asleep: "{}% asleep",
    card_title: "Device battery",
    charging: "charging",
    not_charging: "not charging",
    updated: "Updated {}",
    receiver: "{} receiver",
    wired: "wired",
    low_title: "{} battery low",
    low_body: "{}% left. Charge it soon.",
    full_title: "{} fully charged",
    full_body: "Battery at 100%.",
    bad_config: "The settings file is invalid, so default settings are in use.",
};

static ZH: Strings = Strings {
    refresh: "立即刷新",
    low_alert: "低电量提醒",
    off: "关闭",
    interval: "刷新间隔",
    seconds: "{} 秒",
    minutes: "{} 分钟",
    icons: "托盘图标",
    icons_each: "每个设备一个",
    icons_merged: "合并成一个（显示最低）",
    full_alert: "充满提醒",
    autostart: "开机自启",
    language: "语言",
    follow_system: "跟随系统",
    battery_log: "记录电量日志",
    about: "关于 peripheral-battery-tray",
    quit: "退出",
    no_device: "未找到支持的设备",
    supported: "查看支持的设备…",
    no_response: "未响应",
    asleep: "休眠或未响应",
    asleep_short: "休眠中",
    row_asleep: "{}% 休眠",
    card_title: "设备电量",
    charging: "充电中",
    not_charging: "未充电",
    updated: "{} 更新",
    receiver: "{} 接收器",
    wired: "有线",
    low_title: "{} 电量低",
    low_body: "剩余 {}%，请及时充电。",
    full_title: "{} 已充满",
    full_body: "电量 100%。",
    bad_config: "设置文件无效，当前使用默认设置。",
};
