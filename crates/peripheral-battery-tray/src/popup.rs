//! The card a left click on a tray icon shows: the icon's devices, their battery and when
//! they were read. It is drawn here rather than left to the shell's tooltip, which appears
//! only after a hover delay and holds at most 128 characters.
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
    BeginPaint, CreateFontIndirectW, CreateSolidBrush, DT_CALCRECT, DT_NOPREFIX, DT_SINGLELINE,
    DeleteObject, DrawTextW, EndPaint, FW_SEMIBOLD, FillRect, GetDC, GetMonitorInfoW, HDC, HFONT,
    InvalidateRect, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint, PAINTSTRUCT,
    ReleaseDC, SelectObject, SetBkMode, SetTextColor, TRANSPARENT, UpdateWindow,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::SystemParametersInfoForDpi;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CS_DROPSHADOW, CreateWindowExW, DefWindowProcW, HWND_TOPMOST, IsWindowVisible,
    NONCLIENTMETRICSW, RegisterClassW, SPI_GETNONCLIENTMETRICS, SW_HIDE, SWP_SHOWWINDOW,
    SetForegroundWindow, SetWindowPos, ShowWindow, WA_INACTIVE, WM_ACTIVATE, WM_KEYDOWN, WM_PAINT,
    WNDCLASSW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

/// What one icon's card shows.
#[derive(Clone, PartialEq, Eq)]
pub struct Card {
    /// Each device's name line and status line; an empty status draws no line.
    pub sections: Vec<(String, String)>,
    /// A dimmed last line; empty draws none.
    pub footer: String,
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
}

thread_local! {
    /// Kept apart from the tray's state: the window procedure runs inside Win32 calls the
    /// tray makes while it holds its own.
    static STATE: RefCell<State> = RefCell::new(State::default());
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
            text: 0x001A1A1A,
            dim: 0x00606060,
        }
    } else {
        Palette {
            background: 0x002C2C2C,
            border: 0x00484848,
            text: 0x00FFFFFF,
            dim: 0x00C0C0C0,
        }
    }
}

/// The system message font at the card's DPI, regular and semibold.
struct Fonts {
    regular: HFONT,
    semibold: HFONT,
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
        let mut font = metrics.lfMessageFont;
        let regular = unsafe { CreateFontIndirectW(&font) };
        font.lfWeight = FW_SEMIBOLD as i32;
        let semibold = unsafe { CreateFontIndirectW(&font) };
        Fonts { regular, semibold }
    }
}

impl Drop for Fonts {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.regular);
            DeleteObject(self.semibold);
        }
    }
}

#[derive(Clone, Copy)]
enum Style {
    Title,
    Body,
    Dim,
}

/// One line of the card and the space above it, in physical pixels.
struct Line<'a> {
    text: &'a str,
    style: Style,
    gap: i32,
}

fn lines(card: &Card, scale: impl Fn(i32) -> i32) -> Vec<Line<'_>> {
    let mut lines = Vec::new();
    for (i, (name, status)) in card.sections.iter().enumerate() {
        let gap = if i == 0 { 0 } else { scale(8) };
        lines.push(Line {
            text: name,
            style: Style::Title,
            gap,
        });
        if !status.is_empty() {
            lines.push(Line {
                text: status,
                style: Style::Body,
                gap: scale(2),
            });
        }
    }
    if !card.footer.is_empty() {
        lines.push(Line {
            text: &card.footer,
            style: Style::Dim,
            gap: if lines.is_empty() { 0 } else { scale(8) },
        });
    }
    lines
}

fn font_for(fonts: &Fonts, style: Style) -> HFONT {
    match style {
        Style::Title => fonts.semibold,
        Style::Body | Style::Dim => fonts.regular,
    }
}

/// Measures `text` in the font selected into `dc`.
fn measure(dc: HDC, text: &[u16]) -> (i32, i32) {
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

/// Sizes the card to its text and puts it beside its icon, inside the icon's monitor's
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
    let (pad_x, pad_y) = (scale(12), scale(10));
    let fonts = Fonts::new(dpi);
    let (mut width, mut height) = (0, 0);
    unsafe {
        let dc = GetDC(hwnd);
        let original = SelectObject(dc, fonts.regular);
        for line in lines(&card, scale) {
            SelectObject(dc, font_for(&fonts, line.style));
            let (w, h) = measure(dc, &wide(line.text));
            width = width.max(w);
            height += line.gap + h;
        }
        SelectObject(dc, original);
        ReleaseDC(hwnd, dc);
    }
    let width = width.max(scale(140)) + 2 * pad_x;
    let height = height + 2 * pad_y;

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
        let original = unsafe { SelectObject(dc, fonts.regular) };
        let (x, mut y) = (scale(12), scale(10));
        for line in lines(&shown.card, scale) {
            let text = wide(line.text);
            unsafe {
                SelectObject(dc, font_for(&fonts, line.style));
                SetTextColor(
                    dc,
                    match line.style {
                        Style::Dim => colors.dim,
                        Style::Title | Style::Body => colors.text,
                    },
                );
            }
            let (w, h) = measure(dc, &text);
            y += line.gap;
            let mut rect = RECT {
                left: x,
                top: y,
                right: x + w,
                bottom: y + h,
            };
            unsafe {
                DrawTextW(
                    dc,
                    text.as_ptr(),
                    text.len() as i32,
                    &mut rect,
                    DT_SINGLELINE | DT_NOPREFIX,
                );
            }
            y += h;
        }
        unsafe { SelectObject(dc, original) };
    });
    unsafe { EndPaint(hwnd, &ps) };
}
