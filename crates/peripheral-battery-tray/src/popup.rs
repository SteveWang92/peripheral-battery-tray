//! The card a left click on a tray icon shows: a title with a refresh button, then a panel
//! per device with its battery as a percentage and a bar, its connection, and when it was
//! read. It is drawn here rather than left to the shell's tooltip, which appears only after
//! a hover delay and holds at most 128 characters.
//!
//! The card closes when it loses activation (a click anywhere else), on Escape, or on a
//! second click on its icon.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{
    DWMWA_BORDER_COLOR, DWMWA_CLOAK, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    DwmSetWindowAttribute,
};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontIndirectW, CreateSolidBrush, DT_CALCRECT, DT_CENTER, DT_NOPREFIX,
    DT_SINGLELINE, DT_VCENTER, DeleteObject, DrawTextW, EndPaint, FW_SEMIBOLD, FillRect, GetDC,
    GetMonitorInfoW, GetStockObject, HDC, HFONT, InvalidateRect, MONITOR_DEFAULTTONEAREST,
    MONITORINFO, MonitorFromPoint, NULL_PEN, PAINTSTRUCT, ReleaseDC, RoundRect, SelectObject,
    SetBkMode, SetTextColor, TRANSPARENT, UpdateWindow,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::SystemParametersInfoForDpi;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CS_DROPSHADOW, CreateWindowExW, DefWindowProcW, HWND_TOPMOST, IsWindowVisible,
    NONCLIENTMETRICSW, RegisterClassW, SPI_GETNONCLIENTMETRICS, SW_HIDE, SWP_SHOWWINDOW,
    SetForegroundWindow, SetWindowPos, ShowWindow, WA_INACTIVE, WM_ACTIVATE, WM_KEYDOWN,
    WM_LBUTTONUP, WM_PAINT, WNDCLASSW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

/// What one icon's card shows.
#[derive(Clone, PartialEq, Eq)]
pub struct Card {
    pub title: String,
    pub panels: Vec<Panel>,
    /// Shown in place of the panels when there are none.
    pub empty: String,
}

/// One device on the card.
#[derive(Clone, PartialEq, Eq)]
pub struct Panel {
    pub name: String,
    pub connection: String,
    /// `None` when nothing has been read from the device yet.
    pub percent: Option<u8>,
    pub tone: Tone,
    pub status: String,
}

/// The colour of a device's battery bar.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Normal,
    Low,
    Charging,
    /// The last reading of a device now asleep.
    Stale,
}

/// The icon a card is shown for, in physical pixels.
#[derive(Clone, Copy)]
pub struct Anchor {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// A click on the icon whose card just closed by losing activation is that same click,
/// arriving after the taskbar took activation; it must not open the card again.
const RECLICK: Duration = Duration::from_millis(400);

struct Shown {
    slot: usize,
    card: Card,
    anchor: Anchor,
    dpi: u32,
    light: bool,
}

#[derive(Default)]
struct State {
    hwnd: Option<HWND>,
    shown: Option<Shown>,
    /// The slot whose card last closed by losing activation, and when.
    closed: Option<(usize, Instant)>,
    /// The refresh button, in client coordinates.
    button: Option<RECT>,
    on_refresh: Option<Box<dyn Fn()>>,
}

thread_local! {
    /// Kept apart from the tray's state: the window procedure runs inside Win32 calls the
    /// tray makes while it holds its own.
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// Sets what the refresh button does.
pub fn on_refresh(action: impl Fn() + 'static) {
    STATE.with_borrow_mut(|state| state.on_refresh = Some(Box::new(action)));
}

/// Opens the card for icon `slot`, or closes it if that card is already showing.
pub fn toggle(slot: usize, card: Card, anchor: Anchor, dpi: u32, light: bool) {
    let hwnd = window();
    let (visible, reclick) = STATE.with_borrow(|state| {
        let reclick = state
            .closed
            .is_some_and(|(closed, at)| closed == slot && at.elapsed() < RECLICK);
        let visible = unsafe { IsWindowVisible(hwnd) } != 0;
        (
            visible && state.shown.as_ref().is_some_and(|s| s.slot == slot),
            reclick,
        )
    });
    if visible {
        hide(hwnd);
        return;
    }
    if reclick {
        return;
    }
    STATE.with_borrow_mut(|state| {
        state.shown = Some(Shown {
            slot,
            card,
            anchor,
            dpi,
            light,
        });
    });
    place(hwnd);
    unsafe { SetForegroundWindow(hwnd) };
}

/// Brings an open card up to date with its icon's current card, look and position.
pub fn update(cards: &[Card], anchor: impl Fn(usize) -> Option<Anchor>, dpi: u32, light: bool) {
    let Some(hwnd) = STATE.with_borrow(|state| state.hwnd) else {
        return;
    };
    if unsafe { IsWindowVisible(hwnd) } == 0 {
        return;
    }
    let Some(slot) = STATE.with_borrow(|state| state.shown.as_ref().map(|s| s.slot)) else {
        return;
    };
    // The icon it belonged to is gone.
    let Some(card) = cards.get(slot) else {
        hide(hwnd);
        return;
    };
    let anchor = anchor(slot);
    STATE.with_borrow_mut(|state| {
        if let Some(shown) = state.shown.as_mut() {
            shown.card = card.clone();
            shown.anchor = anchor.unwrap_or(shown.anchor);
            shown.dpi = dpi;
            shown.light = light;
        }
    });
    place(hwnd);
    unsafe { InvalidateRect(hwnd, std::ptr::null(), 1) };
}

fn hide(hwnd: HWND) {
    unsafe { ShowWindow(hwnd, SW_HIDE) };
}

/// The card's window, created on first use.
fn window() -> HWND {
    if let Some(hwnd) = STATE.with_borrow(|state| state.hwnd) {
        return hwnd;
    }
    let class: Vec<u16> = "peripheral-battery-tray-card\0".encode_utf16().collect();
    let hwnd = unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let wc = WNDCLASSW {
            style: CS_DROPSHADOW,
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassW(&wc);
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class.as_ptr(),
            class.as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    let corner = DWMWCP_ROUND;
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            (&raw const corner).cast(),
            size_of_val(&corner) as u32,
        );
    }
    STATE.with_borrow_mut(|state| state.hwnd = Some(hwnd));
    hwnd
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint(hwnd);
            0
        }
        WM_LBUTTONUP => {
            let (x, y) = (lparam as i16 as i32, (lparam >> 16) as i16 as i32);
            STATE.with_borrow(|state| {
                let hit = state.button.is_some_and(|b| {
                    (b.left..b.right).contains(&x) && (b.top..b.bottom).contains(&y)
                });
                if let (true, Some(action)) = (hit, &state.on_refresh) {
                    action();
                }
            });
            0
        }
        WM_ACTIVATE if (wparam & 0xFFFF) as u32 == WA_INACTIVE => {
            STATE.with_borrow_mut(|state| {
                state.closed = state.shown.as_ref().map(|s| (s.slot, Instant::now()));
            });
            hide(hwnd);
            0
        }
        WM_KEYDOWN if wparam == VK_ESCAPE as usize => {
            hide(hwnd);
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

struct Palette {
    background: COLORREF,
    border: COLORREF,
    panel: COLORREF,
    track: COLORREF,
    text: COLORREF,
    dim: COLORREF,
}

/// Follows the taskbar, as the icons and menu do.
fn palette(light: bool) -> Palette {
    // COLORREF is 0x00BBGGRR; every colour here is a grey.
    if light {
        Palette {
            background: 0x00F9F9F9,
            border: 0x00E0E0E0,
            panel: 0x00EDEDED,
            track: 0x00D4D4D4,
            text: 0x001A1A1A,
            dim: 0x00606060,
        }
    } else {
        Palette {
            background: 0x002C2C2C,
            border: 0x00484848,
            panel: 0x00383838,
            track: 0x00505050,
            text: 0x00FFFFFF,
            dim: 0x00B4B4B4,
        }
    }
}

/// The bar's fill: the text colour normally, and the tray icon's colours otherwise.
fn bar_colour(tone: Tone, colors: &Palette) -> COLORREF {
    match tone {
        Tone::Normal => colors.text,
        Tone::Low => 0x000C58EA,
        Tone::Charging => 0x005EC522,
        Tone::Stale => 0x008A8A8A,
    }
}

/// The system message font at the card's DPI in the sizes the card uses.
struct Fonts {
    small: HFONT,
    name: HFONT,
    title: HFONT,
    percent: HFONT,
    glyph: HFONT,
}

impl Fonts {
    fn new(dpi: u32) -> Fonts {
        let mut metrics: NONCLIENTMETRICSW = unsafe { std::mem::zeroed() };
        metrics.cbSize = size_of::<NONCLIENTMETRICSW>() as u32;
        unsafe {
            SystemParametersInfoForDpi(
                SPI_GETNONCLIENTMETRICS,
                metrics.cbSize,
                (&raw mut metrics).cast(),
                0,
                dpi,
            );
        }
        let base = metrics.lfMessageFont;
        let make = |scale: f32, weight: i32, face: Option<&str>| {
            let mut font = base;
            font.lfHeight = (base.lfHeight as f32 * scale).round() as i32;
            font.lfWeight = weight;
            if let Some(face) = face {
                font.lfFaceName = [0; 32];
                for (slot, unit) in font.lfFaceName.iter_mut().zip(face.encode_utf16()) {
                    *slot = unit;
                }
            }
            unsafe { CreateFontIndirectW(&font) }
        };
        let semibold = FW_SEMIBOLD as i32;
        Fonts {
            small: make(1.0, base.lfWeight, None),
            name: make(1.15, semibold, None),
            title: make(1.2, semibold, None),
            percent: make(1.85, semibold, None),
            glyph: make(1.15, base.lfWeight, Some("Segoe MDL2 Assets")),
        }
    }
}

impl Drop for Fonts {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.small);
            DeleteObject(self.name);
            DeleteObject(self.title);
            DeleteObject(self.percent);
            DeleteObject(self.glyph);
        }
    }
}

/// The refresh glyph in Segoe MDL2 Assets.
const REFRESH_GLYPH: &str = "\u{E72C}";

/// Where everything on the card goes, in client coordinates.
struct Layout {
    width: i32,
    height: i32,
    title: RECT,
    button: RECT,
    panels: Vec<PanelLayout>,
    empty: RECT,
}

struct PanelLayout {
    frame: RECT,
    name: RECT,
    percent: RECT,
    connection: RECT,
    bar: RECT,
    status: RECT,
}

fn rect(left: i32, top: i32, (width, height): (i32, i32)) -> RECT {
    RECT {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

fn percent_text(percent: Option<u8>) -> String {
    percent.map_or_else(|| "—".to_owned(), |p| format!("{p}%"))
}

fn layout(dc: HDC, fonts: &Fonts, card: &Card, dpi: u32) -> Layout {
    let scale = |px: i32| px * dpi as i32 / 96;
    let (pad, inner_x, inner_y) = (scale(16), scale(14), scale(12));
    let button_size = scale(30);
    let size = |font: HFONT, text: &str| {
        unsafe { SelectObject(dc, font) };
        measure(dc, &wide(text))
    };

    let title = size(fonts.title, &card.title);
    let measured: Vec<_> = card
        .panels
        .iter()
        .map(|panel| {
            (
                size(fonts.name, &panel.name),
                size(fonts.percent, &percent_text(panel.percent)),
                size(fonts.small, &panel.connection),
                size(fonts.small, &panel.status),
            )
        })
        .collect();
    let empty = size(fonts.small, &card.empty);
    let content = measured
        .iter()
        .map(|&(name, percent, connection, status)| {
            (name.0 + scale(24) + percent.0)
                .max(connection.0)
                .max(status.0)
        })
        .max()
        .unwrap_or(empty.0)
        .max(scale(230));
    let panel_width = content + 2 * inner_x;
    let width = panel_width.max(title.0 + scale(16) + button_size) + 2 * pad;

    let mut y = pad;
    let title_rect = rect(pad, y + (button_size - title.1) / 2, title);
    let button = rect(width - pad - button_size, y, (button_size, button_size));
    y += button_size + scale(12);

    let mut panels = Vec::new();
    for &(name, percent, connection, status) in &measured {
        let left = pad + inner_x;
        let top = y + inner_y;
        let row = name.1.max(percent.1);
        // Bottoms aligned, so the name sits on the percentage's baseline.
        let name_rect = rect(left, top + row - name.1, name);
        let percent_rect = rect(left + content - percent.0, top + row - percent.1, percent);
        let connection_rect = rect(left, top + row + scale(2), connection);
        let bar = rect(
            left,
            connection_rect.bottom + scale(10),
            (content, scale(6)),
        );
        let status_rect = rect(left, bar.bottom + scale(10), status);
        let frame = RECT {
            left: pad,
            top: y,
            right: pad + panel_width,
            bottom: status_rect.bottom + inner_y,
        };
        y = frame.bottom + scale(10);
        panels.push(PanelLayout {
            frame,
            name: name_rect,
            percent: percent_rect,
            connection: connection_rect,
            bar,
            status: status_rect,
        });
    }
    let empty_rect = rect(pad, y, empty);
    let height = if panels.is_empty() {
        empty_rect.bottom + pad
    } else {
        y - scale(10) + pad
    };
    Layout {
        width,
        height,
        title: title_rect,
        button,
        panels,
        empty: empty_rect,
    }
}

/// Measures `text` in the font selected into `dc`.
fn measure(dc: HDC, text: &[u16]) -> (i32, i32) {
    // DrawTextW reads the pointer even for no characters, and an empty Vec's is dangling.
    if text.is_empty() {
        return (0, 0);
    }
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    unsafe {
        DrawTextW(
            dc,
            text.as_ptr(),
            text.len() as i32,
            &mut rect,
            DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX,
        );
    }
    (rect.right, rect.bottom)
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// Sizes the card to its content and puts it beside its icon, inside the icon's monitor's
/// work area: above a bottom taskbar, below a top one, beside a side one, and above an icon
/// in the overflow area.
fn place(hwnd: HWND) {
    let Some((card, anchor, dpi, light)) = STATE.with_borrow(|state| {
        let shown = state.shown.as_ref()?;
        Some((shown.card.clone(), shown.anchor, shown.dpi, shown.light))
    }) else {
        return;
    };
    let border = palette(light).border;
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR as u32,
            (&raw const border).cast(),
            size_of_val(&border) as u32,
        );
    }
    let scale = |px: i32| px * dpi as i32 / 96;
    let fonts = Fonts::new(dpi);
    let layout = unsafe {
        let dc = GetDC(hwnd);
        let original = SelectObject(dc, fonts.small);
        let layout = layout(dc, &fonts, &card, dpi);
        SelectObject(dc, original);
        ReleaseDC(hwnd, dc);
        layout
    };
    STATE.with_borrow_mut(|state| state.button = Some(layout.button));
    let (width, height) = (layout.width, layout.height);

    let center = POINT {
        x: anchor.x + anchor.width / 2,
        y: anchor.y + anchor.height / 2,
    };
    let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
    info.cbSize = size_of::<MONITORINFO>() as u32;
    unsafe {
        GetMonitorInfoW(
            MonitorFromPoint(center, MONITOR_DEFAULTTONEAREST),
            &mut info,
        )
    };
    let work = info.rcWork;
    let gap = scale(8);
    let x = if center.x >= work.right {
        work.right - gap - width
    } else if center.x < work.left {
        work.left + gap
    } else {
        center.x - width / 2
    };
    let y = if center.y >= work.bottom {
        work.bottom - gap - height
    } else if center.y < work.top {
        work.top + gap
    } else if center.x >= work.right || center.x < work.left {
        center.y - height / 2
    } else {
        anchor.y - gap - height
    };
    let x = x.clamp(
        work.left + gap,
        (work.right - gap - width).max(work.left + gap),
    );
    let y = y.clamp(
        work.top + gap,
        (work.bottom - gap - height).max(work.top + gap),
    );
    if unsafe { IsWindowVisible(hwnd) } != 0 {
        unsafe { SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_SHOWWINDOW) };
        return;
    }
    // A window paints only once shown, so DWM would put its frame on screen before the
    // first WM_PAINT fills it. Shown cloaked and painted at once, it appears complete.
    cloak(hwnd, true);
    unsafe {
        SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_SHOWWINDOW);
        UpdateWindow(hwnd);
    }
    cloak(hwnd, false);
}

fn cloak(hwnd: HWND, on: bool) {
    let value = i32::from(on);
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_CLOAK as u32,
            (&raw const value).cast(),
            size_of_val(&value) as u32,
        );
    }
}

/// Fills `area` with `colour`, its corners rounded by `radius`.
fn rounded(dc: HDC, area: RECT, radius: i32, colour: COLORREF) {
    unsafe {
        let brush = CreateSolidBrush(colour);
        let old_brush = SelectObject(dc, brush);
        let old_pen = SelectObject(dc, GetStockObject(NULL_PEN));
        // Without a pen the shape stops one pixel short of its right and bottom edges.
        RoundRect(
            dc,
            area.left,
            area.top,
            area.right + 1,
            area.bottom + 1,
            2 * radius,
            2 * radius,
        );
        SelectObject(dc, old_pen);
        SelectObject(dc, old_brush);
        DeleteObject(brush);
    }
}

fn text(dc: HDC, text: &str, mut area: RECT, font: HFONT, colour: COLORREF, centred: bool) {
    if text.is_empty() {
        return;
    }
    let text = wide(text);
    let format = if centred {
        DT_SINGLELINE | DT_NOPREFIX | DT_CENTER | DT_VCENTER
    } else {
        DT_SINGLELINE | DT_NOPREFIX
    };
    unsafe {
        SelectObject(dc, font);
        SetTextColor(dc, colour);
        DrawTextW(dc, text.as_ptr(), text.len() as i32, &mut area, format);
    }
}

fn paint(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = unsafe { std::mem::zeroed() };
    let dc = unsafe { BeginPaint(hwnd, &mut ps) };
    STATE.with_borrow(|state| {
        let Some(shown) = &state.shown else {
            return;
        };
        let colors = palette(shown.light);
        unsafe {
            let brush = CreateSolidBrush(colors.background);
            FillRect(dc, &ps.rcPaint, brush);
            DeleteObject(brush);
            SetBkMode(dc, TRANSPARENT as i32);
        }
        let dpi = shown.dpi;
        let scale = |px: i32| px * dpi as i32 / 96;
        let fonts = Fonts::new(dpi);
        let original = unsafe { SelectObject(dc, fonts.small) };
        let card = &shown.card;
        let layout = layout(dc, &fonts, card, dpi);

        text(
            dc,
            &card.title,
            layout.title,
            fonts.title,
            colors.text,
            false,
        );
        rounded(dc, layout.button, scale(6), colors.panel);
        text(
            dc,
            REFRESH_GLYPH,
            layout.button,
            fonts.glyph,
            colors.text,
            true,
        );
        if card.panels.is_empty() {
            text(
                dc,
                &card.empty,
                layout.empty,
                fonts.small,
                colors.dim,
                false,
            );
        }
        for (panel, at) in card.panels.iter().zip(&layout.panels) {
            rounded(dc, at.frame, scale(8), colors.panel);
            text(dc, &panel.name, at.name, fonts.name, colors.text, false);
            let percent_colour = if panel.tone == Tone::Stale {
                colors.dim
            } else {
                colors.text
            };
            text(
                dc,
                &percent_text(panel.percent),
                at.percent,
                fonts.percent,
                percent_colour,
                false,
            );
            text(
                dc,
                &panel.connection,
                at.connection,
                fonts.small,
                colors.dim,
                false,
            );
            let radius = (at.bar.bottom - at.bar.top) / 2;
            rounded(dc, at.bar, radius, colors.track);
            if let Some(percent) = panel.percent.filter(|&p| p > 0) {
                let full = at.bar.right - at.bar.left;
                let filled = (full * i32::from(percent.min(100)) / 100).max(2 * radius);
                let fill = RECT {
                    right: at.bar.left + filled,
                    ..at.bar
                };
                rounded(dc, fill, radius, bar_colour(panel.tone, &colors));
            }
            text(dc, &panel.status, at.status, fonts.small, colors.dim, false);
        }
        unsafe { SelectObject(dc, original) };
    });
    unsafe { EndPaint(hwnd, &ps) };
}
