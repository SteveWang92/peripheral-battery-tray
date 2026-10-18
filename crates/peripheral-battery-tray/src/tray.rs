//! The tray application: one icon per device (or one merged icon), the card a left click
//! shows, the right-click menu that holds every setting, and the battery toasts.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use chrono::{DateTime, Local};

use logitech_battery::HidApi;
use tray_icon::menu::{
    CheckMenuItem, ContextMenu, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
};
use tray_icon::{
    MouseButton, MouseButtonState, Rect, TrayIcon, TrayIconBuilder, TrayIconEvent, TrayIconId,
};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::InvalidateRect;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, EndMenu, EnumThreadWindows, GUITHREADINFO, GetClassNameW,
    GetGUIThreadInfo, GetMenuItemID, GetMenuState, GetMessageW, HMENU, MF_BYPOSITION, MF_DISABLED,
    MF_GRAYED, MF_POPUP, MF_SEPARATOR, MSG, MSGF_MENU, MenuItemFromPoint, PM_NOREMOVE,
    PeekMessageW, PostMessageW, PostQuitMessage, PostThreadMessageW, SW_SHOWNORMAL, SendMessageW,
    SetForegroundWindow, SetTimer, SetWindowsHookExW, TPM_BOTTOMALIGN, TPM_LEFTALIGN,
    TrackPopupMenu, TranslateMessage, WH_MSGFILTER, WM_APP, WM_COMMAND, WM_LBUTTONUP,
    WM_NCLBUTTONUP, WM_NULL, WM_TIMER, WindowFromPoint,
};

use crate::config::{self, Loaded, Settings};
use crate::icon::{self, Face};
use crate::poller::{self, Command, DeviceKey, DeviceStatus, Reading, Snapshot};
use crate::popup::{self, Anchor, Card, Panel, Tone};
use crate::strings::{Language, Strings};
use crate::system;
use crate::theme::{self, Look};

const PROJECT_URL: &str = "https://github.com/SteveWang92/peripheral-battery-tray";
const SUPPORTED_URL: &str =
    "https://github.com/SteveWang92/peripheral-battery-tray#supported-devices";

/// How often the taskbar's theme and DPI are checked for a change.
const LOOK_CHECK_MS: u32 = 2000;

const THRESHOLDS: [u8; 4] = [0, 10, 20, 30];
const INTERVALS: [u64; 3] = [30, 60, 300];
const LANGUAGES: [(Language, &str); 3] = [
    (Language::System, "system"),
    (Language::English, "en"),
    (Language::Chinese, "zh-CN"),
];

/// `MN_GETHMENU`: asks a menu window for the menu it shows.
const MN_GETHMENU: u32 = 0x01E1;

thread_local! {
    /// The tray, shared by the message loop and the menu hook, which both run on this thread
    /// but never at the same time.
    static TRAY: RefCell<Option<Tray>> = const { RefCell::new(None) };
}

/// What is shown for one device.
#[derive(Clone, Copy)]
enum View {
    Live {
        percent: u8,
        charging: bool,
        estimated: bool,
    },
    /// The device is asleep; its battery barely moves then, so its last reading stands.
    Asleep {
        percent: u8,
        read_at: DateTime<Local>,
    },
    /// Asleep with nothing read since the tray started.
    Unknown,
}

/// Which one-shot toasts a device can still raise.
struct Alerts {
    /// Cleared by the low-battery toast; set again once the device is seen charging.
    low_armed: bool,
    /// Set while the device charges below 100%; cleared by the full-charge toast.
    full_pending: bool,
}

/// A closed menu to show again.
struct Reopen {
    owner: HWND,
    menu: HMENU,
    x: i32,
    y: i32,
}

impl Reopen {
    /// Shows the menu the way the shell's own tray menus open, and blocks until it closes.
    fn show(self) {
        unsafe {
            // The owner must be in the foreground for a click outside to close the menu.
            SetForegroundWindow(self.owner);
            TrackPopupMenu(
                self.menu,
                TPM_BOTTOMALIGN | TPM_LEFTALIGN,
                self.x,
                self.y,
                0,
                self.owner,
                std::ptr::null(),
            );
            PostMessageW(self.owner, WM_NULL, 0, 0);
        }
    }
}

/// What a menu click asks of the open menu.
enum Outcome {
    Stay,
    Close,
    /// Close, then open the rebuilt menu in its place.
    Reopen,
}

/// The menu and the items whose check marks or text change while it exists.
struct MenuItems {
    menu: Menu,
    /// The device count it was built for; 0 is the no-device menu.
    devices: usize,
    /// Each device's name row and, greyed below it, its connection and battery.
    rows: Vec<(MenuItem, MenuItem)>,
    low: Vec<(u8, CheckMenuItem)>,
    intervals: Vec<(u64, CheckMenuItem)>,
    icons_each: CheckMenuItem,
    icons_merged: CheckMenuItem,
    full: CheckMenuItem,
    autostart: CheckMenuItem,
    languages: Vec<(Language, CheckMenuItem)>,
    log: CheckMenuItem,
}

struct Tray {
    settings: Settings,
    commands: mpsc::Sender<Command>,
    inbox: Receiver<Snapshot>,
    snapshot: Option<Snapshot>,
    /// Every icon created this run; the first `shown` are visible and the rest hidden.
    icons: Vec<TrayIcon>,
    shown: usize,
    /// What each icon's card shows, by icon position.
    cards: Vec<Card>,
    menu: Option<MenuItems>,
    look: Look,
    /// Alert state per device id: a ROG device charging over its cable appears under another
    /// connection than the receiver it drains on.
    alerts: HashMap<String, Alerts>,
    /// Each device's last battery reading and when it was taken.
    last_read: HashMap<DeviceKey, (u8, DateTime<Local>)>,
    /// Set while a menu is showing: the menu and the icons it belongs to cannot be replaced
    /// then, so such changes wait for it to close.
    menu_open: bool,
    rebuild_pending: bool,
    /// Set when the closed menu should open again, rebuilt.
    reopen: bool,
    /// The icon last right-clicked, which a reopened menu belongs to.
    last_clicked: Option<TrayIconId>,
    /// The window that owns the open menu and receives its commands.
    menu_owner: Option<HWND>,
}

pub fn run(api: HidApi) {
    system::register_toasts();
    let settings = match config::load() {
        Loaded::Saved(settings) => settings,
        Loaded::FirstRun => {
            let settings = Settings::default();
            // Without a saved file every launch is a first run, which would switch
            // autostart back on after the user turned it off.
            if config::save(&settings) {
                system::set_autostart(true);
            }
            settings
        }
        // The file is left as it is until a setting is changed from the menu.
        Loaded::Invalid => {
            let settings = Settings::default();
            system::toast(
                "peripheral-battery-tray",
                settings.language.strings().bad_config,
            );
            settings
        }
    };
    let look = Look::current();
    theme::apply_menu_theme(look.light);

    let main_thread = unsafe { GetCurrentThreadId() };
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    // Creates this thread's message queue before the poller first posts to it.
    unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE) };

    let (snapshots, inbox) = mpsc::channel();
    let commands = poller::spawn(
        api,
        Duration::from_secs(settings.interval_secs),
        settings.battery_log,
        snapshots,
        move || unsafe {
            PostThreadMessageW(main_thread, WM_APP, 0, 0);
        },
    );
    let refresh = commands.clone();
    popup::on_refresh(move || {
        let _ = refresh.send(Command::Refresh);
    });
    TRAY.set(Some(Tray {
        settings,
        commands,
        inbox,
        snapshot: None,
        icons: Vec::new(),
        shown: 0,
        cards: Vec::new(),
        menu: None,
        look,
        alerts: HashMap::new(),
        last_read: HashMap::new(),
        menu_open: false,
        rebuild_pending: false,
        reopen: false,
        last_clicked: None,
        menu_owner: None,
    }));
    unsafe {
        SetTimer(std::ptr::null_mut(), 0, LOOK_CHECK_MS, None);
        SetWindowsHookExW(
            WH_MSGFILTER,
            Some(menu_hook),
            std::ptr::null_mut(),
            main_thread,
        );
    }

    while unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) } > 0 {
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        // The dispatch above returns only once any menu it showed has closed.
        let reopen = TRAY.with_borrow_mut(|tray| {
            let tray = tray.as_mut().expect("the tray is set before the loop");
            tray.menu_open = false;
            tray.on_message(&msg);
            tray.take_reopen()
        });
        // Shown here, outside every borrow: the menu hook needs the tray, and the tray its
        // icons, while the menu is open.
        if let Some(reopen) = reopen {
            reopen.show();
        }
    }
    TRAY.take();
}

/// Runs inside the menu's own message loop. A click on a command item is handed to the menu's
/// owner as the `WM_COMMAND` it would have become, then swallowed, so the menu stays open
/// for the next change instead of closing after every click.
unsafe extern "system" fn menu_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == MSGF_MENU as i32 {
        let msg = unsafe { &*(lparam as *const MSG) };
        let swallowed = TRAY.with(|cell| {
            let Ok(mut tray) = cell.try_borrow_mut() else {
                return false;
            };
            let Some(tray) = tray.as_mut() else {
                return false;
            };
            tray.menu_open = true;
            match msg.message {
                WM_APP => {
                    tray.take_snapshots();
                    false
                }
                WM_LBUTTONUP | WM_NCLBUTTONUP => {
                    let Some(owner) = (unsafe { click_in_menu(msg) }) else {
                        return false;
                    };
                    tray.menu_owner = Some(owner);
                    let mut close = false;
                    while let Ok(event) = MenuEvent::receiver().try_recv() {
                        match tray.on_menu(event.id.as_ref()) {
                            Outcome::Stay => {}
                            Outcome::Close => close = true,
                            Outcome::Reopen => {
                                close = true;
                                tray.reopen = true;
                            }
                        }
                    }
                    if close {
                        unsafe { EndMenu() };
                    }
                    true
                }
                _ => false,
            }
        });
        if swallowed {
            return 1;
        }
    }
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

/// Sends the command of the item under a click in a menu window to the menu's owner, and
/// returns that owner. `None` leaves the click to the menu: it missed an item, or hit a
/// submenu, separator or disabled row.
unsafe fn click_in_menu(msg: &MSG) -> Option<HWND> {
    // While a menu is open the mouse is captured, so the message names the capturing window
    // rather than the menu or submenu under the cursor.
    let window = unsafe { WindowFromPoint(msg.pt) };
    if !is_menu_window(window) {
        return None;
    }
    let menu = unsafe { SendMessageW(window, MN_GETHMENU, 0, 0) } as HMENU;
    if menu.is_null() {
        return None;
    }
    let position = unsafe { MenuItemFromPoint(window, menu, msg.pt) };
    if position < 0 {
        return None;
    }
    let state = unsafe { GetMenuState(menu, position as u32, MF_BYPOSITION) };
    if state & (MF_POPUP | MF_SEPARATOR | MF_DISABLED | MF_GRAYED) != 0 {
        return None;
    }
    let mut info: GUITHREADINFO = unsafe { std::mem::zeroed() };
    info.cbSize = size_of::<GUITHREADINFO>() as u32;
    if unsafe { GetGUIThreadInfo(GetCurrentThreadId(), &mut info) } == 0 {
        return None;
    }
    let id = unsafe { GetMenuItemID(menu, position) };
    unsafe { SendMessageW(info.hwndMenuOwner, WM_COMMAND, id as usize, 0) };
    Some(info.hwndMenuOwner)
}

impl Tray {
    fn strings(&self) -> &'static Strings {
        self.settings.language.strings()
    }

    fn on_message(&mut self, msg: &MSG) {
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            match event {
                TrayIconEvent::Click {
                    id,
                    button: MouseButton::Right,
                    ..
                } => self.last_clicked = Some(id),
                TrayIconEvent::Click {
                    id,
                    rect,
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } => {
                    let slot = self.icons.iter().position(|icon| *icon.id() == id);
                    if let Some((slot, card)) = slot.and_then(|s| Some((s, self.cards.get(s)?))) {
                        let look = self.look;
                        popup::toggle(slot, card.clone(), anchor(rect), look.dpi, look.light);
                    }
                }
                _ => {}
            }
        }
        self.take_snapshots();
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            self.on_menu(event.id.as_ref());
        }
        if msg.message == WM_TIMER && msg.hwnd.is_null() {
            let look = Look::current();
            if look != self.look {
                if look.light != self.look.light {
                    theme::apply_menu_theme(look.light);
                }
                self.look = look;
                self.refresh();
            } else if self.rebuild_pending {
                self.refresh();
            }
        } else if self.rebuild_pending {
            self.refresh();
        }
    }

    /// Where the rebuilt menu opens again, once a reopen was asked for and it is in place:
    /// above the icon last right-clicked, as the shell first showed it.
    fn take_reopen(&mut self) -> Option<Reopen> {
        if !self.reopen || self.rebuild_pending {
            return None;
        }
        self.reopen = false;
        let owner = self.menu_owner?;
        let clicked = self.last_clicked.as_ref();
        let icon = self
            .icons
            .iter()
            .find(|icon| Some(icon.id()) == clicked)
            .or(self.icons.first())?;
        let rect = icon.rect()?;
        Some(Reopen {
            owner,
            menu: self.menu.as_ref()?.menu.hpopupmenu() as HMENU,
            x: rect.position.x as i32,
            y: rect.position.y as i32,
        })
    }

    fn take_snapshots(&mut self) {
        let mut latest = None;
        while let Ok(snapshot) = self.inbox.try_recv() {
            latest = Some(snapshot);
        }
        if let Some(snapshot) = latest {
            self.raise_alerts(&snapshot);
            for device in &snapshot.devices {
                if let Reading::Battery { percent, .. } = device.reading {
                    self.last_read.insert(device.key(), (percent, snapshot.at));
                }
            }
            self.snapshot = Some(snapshot);
            self.refresh();
        }
    }

    /// Applies a menu click.
    fn on_menu(&mut self, id: &str) -> Outcome {
        let (kind, value) = id.split_once(':').unwrap_or((id, ""));
        let s = &mut self.settings;
        match kind {
            "refresh" => {
                let _ = self.commands.send(Command::Refresh);
                return Outcome::Stay;
            }
            "low" => s.low_threshold = value.parse().expect("low ids carry a percentage"),
            "interval" => {
                s.interval_secs = value.parse().expect("interval ids carry seconds");
                let interval = Duration::from_secs(s.interval_secs);
                let _ = self.commands.send(Command::Interval(interval));
            }
            "icons" => s.merged_icon = value == "merged",
            "full" => s.full_alert = !s.full_alert,
            "autostart" => {
                system::set_autostart(!system::autostart_enabled());
                self.refresh();
                return Outcome::Stay;
            }
            "language" => {
                s.language = LANGUAGES
                    .iter()
                    .find(|(_, code)| *code == value)
                    .map_or(Language::System, |(language, _)| *language);
                config::save(&self.settings);
                // An open menu cannot be re-measured for the new labels, so it is rebuilt
                // once closed and shown again from the same icon.
                self.rebuild_pending = true;
                return Outcome::Reopen;
            }
            "log" => {
                s.battery_log = !s.battery_log;
                let _ = self.commands.send(Command::Logging(s.battery_log));
            }
            "about" => {
                open_url(PROJECT_URL);
                return Outcome::Close;
            }
            "supported" => {
                open_url(SUPPORTED_URL);
                return Outcome::Close;
            }
            "quit" => {
                unsafe { PostQuitMessage(0) };
                return Outcome::Close;
            }
            _ => return Outcome::Stay,
        }
        config::save(&self.settings);
        self.refresh();
        Outcome::Stay
    }

    fn raise_alerts(&mut self, snapshot: &Snapshot) {
        let strings = self.strings();
        // A device charging under any connection is charging under all of them; otherwise
        // its receiver entry would raise the low toast the cable entry re-arms, every round.
        let charging_ids: HashSet<&str> = snapshot
            .devices
            .iter()
            .filter(|d| matches!(d.reading, Reading::Battery { charging: true, .. }))
            .map(|d| d.id.as_str())
            .collect();
        for device in &snapshot.devices {
            let Reading::Battery {
                percent, charging, ..
            } = device.reading
            else {
                continue;
            };
            let alerts = self.alerts.entry(device.id.clone()).or_insert(Alerts {
                low_armed: true,
                full_pending: false,
            });
            if charging_ids.contains(device.id.as_str()) {
                alerts.low_armed = true;
            } else if alerts.low_armed
                && self.settings.low_threshold().is_some_and(|t| percent <= t)
            {
                let (title, body) = strings.low_toast(&device.name, percent);
                system::toast(&title, &body);
                alerts.low_armed = false;
            }
            // A device may stop reporting "charging" once full, so reaching 100% after
            // charging below it is what counts.
            if charging && percent < 100 {
                alerts.full_pending = true;
            } else if percent == 100 && alerts.full_pending {
                if self.settings.full_alert {
                    let (title, body) = strings.full_toast(&device.name);
                    system::toast(&title, &body);
                }
                alerts.full_pending = false;
            }
        }
    }

    /// Brings the icons, cards and menu up to date with the snapshot and settings.
    fn refresh(&mut self) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        let strings = self.strings();
        let time = snapshot.at.format("%H:%M").to_string();

        // Each icon: what it shows and its card.
        let low = self.settings.low_threshold();
        let faces: Vec<(Face, Card)> = if snapshot.devices.is_empty() {
            let card = Card {
                title: strings.card_title.to_owned(),
                panels: Vec::new(),
                empty: strings.no_device.to_owned(),
            };
            vec![(Face::NoDevice, card)]
        } else if self.settings.merged_icon {
            let all: Vec<(&DeviceStatus, View)> =
                snapshot.devices.iter().map(|d| (d, self.view(d))).collect();
            let lowest = all
                .iter()
                .filter_map(|&(_, view)| match view {
                    View::Live { percent, .. } | View::Asleep { percent, .. } => {
                        Some((percent, view))
                    }
                    View::Unknown => None,
                })
                .min_by_key(|(percent, _)| *percent);
            let face = lowest.map_or(Face::Asleep, |(_, view)| self.face(view));
            vec![(face, card(strings, &all, &time, low))]
        } else {
            snapshot
                .devices
                .iter()
                .map(|d| {
                    let view = self.view(d);
                    (self.face(view), card(strings, &[(d, view)], &time, low))
                })
                .collect()
        };

        let needs_menu = self.rebuild_pending
            || self
                .menu
                .as_ref()
                .is_none_or(|m| m.devices != snapshot.devices.len());
        // Neither the menu nor the set of icons can change under an open menu. Icons are
        // never replaced either: the shell knows an icon by its window and number, and
        // remembers its "always show" setting under them, so a new icon is a new entry.
        let reshape = needs_menu || self.shown != faces.len();
        if self.menu_open && reshape {
            self.rebuild_pending = true;
        } else if needs_menu {
            let items = self.build_menu(snapshot.devices.len());
            // Existing icons keep their identity and only take the new menu; replacing an
            // icon makes the shell treat it as a new program and forget its visibility.
            for tray in &self.icons {
                tray.set_menu(Some(Box::new(items.menu.clone())));
            }
            self.menu = Some(items);
            self.rebuild_pending = false;
        }
        // Icons beyond the faces are hidden rather than dropped, so one that is needed again
        // comes back under its old number.
        if !self.menu_open {
            for (slot, tray) in self.icons.iter().enumerate() {
                if (slot < faces.len()) != (slot < self.shown) {
                    let _ = tray.set_visible(slot < faces.len());
                }
            }
            self.shown = faces.len().min(self.icons.len());
        }
        for (slot, (face, _)) in faces.iter().enumerate() {
            let image = icon::render(*face, self.look.dpi, self.look.light);
            if let Some(tray) = self.icons.get(slot) {
                let _ = tray.set_icon(Some(image));
                continue;
            }
            if self.menu_open {
                break;
            }
            let menu = &self.menu.as_ref().expect("built above").menu;
            let built = TrayIconBuilder::new()
                .with_icon(image)
                .with_menu(Box::new(menu.clone()))
                .with_menu_on_left_click(false)
                .build();
            match built {
                Ok(tray) => {
                    self.icons.push(tray);
                    self.shown += 1;
                }
                // Only creating the icon's window fails here; an icon the shell is not
                // ready for is added by tray-icon once the taskbar announces itself.
                Err(_) => break,
            }
        }
        self.cards = faces.into_iter().map(|(_, card)| card).collect();
        let icons = &self.icons;
        let at = |slot: usize| icons.get(slot)?.rect().map(anchor);
        popup::update(&self.cards, at, self.look.dpi, self.look.light);
        self.sync_menu();
    }

    /// Updates the menu's check marks and device rows in place, so an open menu shows the
    /// change immediately.
    fn sync_menu(&self) {
        let (Some(items), Some(snapshot)) = (&self.menu, &self.snapshot) else {
            return;
        };
        if items.devices != snapshot.devices.len() {
            return;
        }
        let t = self.strings();
        let s = &self.settings;
        for ((name, detail), device) in items.rows.iter().zip(&snapshot.devices) {
            name.set_text(&device.name);
            detail.set_text(row_detail(t, device, self.view(device)));
        }
        for (p, item) in &items.low {
            item.set_checked(s.low_threshold == *p);
        }
        for (secs, item) in &items.intervals {
            item.set_checked(s.interval_secs == *secs);
        }
        items.icons_each.set_checked(!s.merged_icon);
        items.icons_merged.set_checked(s.merged_icon);
        items.full.set_checked(s.full_alert);
        items.autostart.set_checked(system::autostart_enabled());
        for (language, item) in &items.languages {
            item.set_checked(s.language == *language);
        }
        items.log.set_checked(s.battery_log);
        if self.menu_open {
            repaint_menus();
        }
    }

    fn view(&self, device: &DeviceStatus) -> View {
        match device.reading {
            Reading::Battery {
                percent,
                charging,
                estimated,
            } => View::Live {
                percent,
                charging,
                estimated,
            },
            Reading::Asleep => match self.last_read.get(&device.key()) {
                Some(&(percent, read_at)) => View::Asleep { percent, read_at },
                None => View::Unknown,
            },
        }
    }

    fn face(&self, view: View) -> Face {
        let low = |percent| self.settings.low_threshold().is_some_and(|t| percent <= t);
        match view {
            View::Live {
                percent, charging, ..
            } => Face::Level {
                percent,
                charging,
                low: low(percent),
                stale: false,
            },
            View::Asleep { percent, .. } => Face::Level {
                percent,
                charging: false,
                low: low(percent),
                stale: true,
            },
            View::Unknown => Face::Asleep,
        }
    }

    /// A menu for `devices` devices; check marks and row text are filled in by `sync_menu`.
    fn build_menu(&self, devices: usize) -> MenuItems {
        let t = self.strings();
        let menu = Menu::new();
        let check = |id: String, text: &str| CheckMenuItem::with_id(id, text, true, false, None);
        let low: Vec<(u8, CheckMenuItem)> = THRESHOLDS
            .iter()
            .map(|&p| {
                let text = if p == 0 {
                    t.off.to_owned()
                } else {
                    format!("{p}%")
                };
                (p, check(format!("low:{p}"), &text))
            })
            .collect();
        let intervals: Vec<(u64, CheckMenuItem)> = INTERVALS
            .iter()
            .map(|&secs| {
                (
                    secs,
                    check(format!("interval:{secs}"), &t.interval_label(secs)),
                )
            })
            .collect();
        // Language names stay in their own language so either can be found.
        let languages: Vec<(Language, CheckMenuItem)> = LANGUAGES
            .iter()
            .map(|&(language, code)| {
                let text = match language {
                    Language::System => t.follow_system,
                    Language::English => "English",
                    Language::Chinese => "简体中文",
                };
                (language, check(format!("language:{code}"), text))
            })
            .collect();
        let items = MenuItems {
            menu: menu.clone(),
            devices,
            rows: (0..devices)
                .map(|i| {
                    (
                        MenuItem::with_id(format!("device:{i}"), "", true, None),
                        MenuItem::new("", false, None),
                    )
                })
                .collect(),
            low,
            intervals,
            icons_each: check("icons:each".to_owned(), t.icons_each),
            icons_merged: check("icons:merged".to_owned(), t.icons_merged),
            full: check("full".to_owned(), t.full_alert),
            autostart: check("autostart".to_owned(), t.autostart),
            languages,
            log: check("log".to_owned(), t.battery_log),
        };

        let quit = MenuItem::with_id("quit", t.quit, true, None);
        if devices == 0 {
            append(
                &menu,
                &[
                    &MenuItem::new(t.no_device, false, None),
                    &MenuItem::with_id("supported", t.supported, true, None),
                    &PredefinedMenuItem::separator(),
                    &quit,
                ],
            );
            return items;
        }
        for (name, detail) in &items.rows {
            append(&menu, &[name, detail]);
        }
        let version = env!("CARGO_PKG_VERSION");
        append(
            &menu,
            &[
                &PredefinedMenuItem::separator(),
                &MenuItem::with_id("refresh", t.refresh, true, None),
                &PredefinedMenuItem::separator(),
                &submenu(t.low_alert, items.low.iter().map(|(_, i)| i)),
                &submenu(t.interval, items.intervals.iter().map(|(_, i)| i)),
                &submenu(t.icons, [&items.icons_each, &items.icons_merged]),
                &items.full,
                &items.autostart,
                &submenu(t.language, items.languages.iter().map(|(_, i)| i)),
                &items.log,
                &PredefinedMenuItem::separator(),
                &MenuItem::with_id("about", format!("{}\tv{version}", t.about), true, None),
                &quit,
            ],
        );
        items
    }
}

fn append(menu: &Menu, items: &[&dyn IsMenuItem]) {
    menu.append_items(items).expect("menu items are new");
}

fn submenu<'a>(text: &str, items: impl IntoIterator<Item = &'a CheckMenuItem>) -> Submenu {
    let items: Vec<&dyn IsMenuItem> = items.into_iter().map(|i| i as &dyn IsMenuItem).collect();
    Submenu::with_items(text, true, &items).expect("menu items are new")
}

/// The greyed row under a device's name in the menu.
fn row_detail(strings: &Strings, device: &DeviceStatus, view: View) -> String {
    let battery = match view {
        View::Live {
            percent, charging, ..
        } => strings.row_status(percent, charging),
        View::Asleep { percent, .. } => strings.row_asleep(percent),
        View::Unknown => strings.no_response.to_owned(),
    };
    format!("{} · {battery}", strings.connection(device.connection))
}

fn card(
    strings: &Strings,
    devices: &[(&DeviceStatus, View)],
    time: &str,
    low_threshold: Option<u8>,
) -> Card {
    let low = |percent| low_threshold.is_some_and(|t| percent <= t);
    let panels = devices
        .iter()
        .map(|&(d, view)| {
            let mut estimated = false;
            let (percent, tone, status) = match view {
                View::Live {
                    percent,
                    charging,
                    estimated: approximate,
                } => {
                    estimated = approximate;
                    let tone = if charging {
                        Tone::Charging
                    } else if low(percent) {
                        Tone::Low
                    } else {
                        Tone::Normal
                    };
                    (Some(percent), tone, strings.live_status(charging, time))
                }
                View::Asleep { percent, read_at } => (
                    Some(percent),
                    Tone::Stale,
                    strings.asleep_status(&read_at.format("%H:%M").to_string()),
                ),
                View::Unknown => (None, Tone::Stale, strings.asleep.to_owned()),
            };
            Panel {
                name: d.name.clone(),
                connection: strings.connection_detail(d.connection),
                percent,
                estimated,
                tone,
                status,
            }
        })
        .collect();
    Card {
        title: strings.card_title.to_owned(),
        panels,
        empty: String::new(),
    }
}

fn anchor(rect: Rect) -> Anchor {
    Anchor {
        x: rect.position.x as i32,
        y: rect.position.y as i32,
        width: rect.size.width as i32,
        height: rect.size.height as i32,
    }
}

/// Whether `hwnd` is a popup menu window, whose class is always "#32768".
fn is_menu_window(hwnd: HWND) -> bool {
    let mut class = [0u16; 8];
    let len = unsafe { GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32) };
    class[..len.max(0) as usize] == *"#32768".encode_utf16().collect::<Vec<_>>()
}

/// Repaints this thread's open menu windows after their items changed underneath them.
fn repaint_menus() {
    unsafe extern "system" fn repaint(hwnd: HWND, _: LPARAM) -> i32 {
        if is_menu_window(hwnd) {
            unsafe { InvalidateRect(hwnd, std::ptr::null(), 1) };
        }
        1
    }
    unsafe { EnumThreadWindows(GetCurrentThreadId(), Some(repaint), 0) };
}

fn open_url(url: &str) {
    let verb: Vec<u16> = "open\0".encode_utf16().collect();
    let target: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
    unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        );
    }
}
