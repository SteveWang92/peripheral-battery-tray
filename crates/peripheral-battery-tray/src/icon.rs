//! Tray icons rendered at runtime: the battery percentage as digits, drawn with the system
//! font at the taskbar's DPI so they stay sharp at every scale.

use std::ptr;

use tray_icon::Icon;
use windows_sys::Win32::Graphics::Gdi::{
    ANTIALIASED_QUALITY, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CLIP_DEFAULT_PRECIS,
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DEFAULT_CHARSET, DEFAULT_PITCH,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, FW_BOLD, GdiFlush, OUT_TT_PRECIS, SelectObject,
    SetBkMode, SetTextColor, TRANSPARENT, TextOutW,
};
use windows_sys::Win32::UI::HiDpi::GetSystemMetricsForDpi;
use windows_sys::Win32::UI::WindowsAndMessaging::SM_CXSMICON;

/// Digit colours for a low or charging battery, darker on a light taskbar.
const LOW_ON_DARK: [u8; 3] = [0xF9, 0x73, 0x16];
const LOW_ON_LIGHT: [u8; 3] = [0xC2, 0x41, 0x0C];
const CHARGING_ON_DARK: [u8; 3] = [0x22, 0xC5, 0x5E];
const CHARGING_ON_LIGHT: [u8; 3] = [0x15, 0x80, 0x3D];
const GREY: [u8; 3] = [0x8A, 0x8A, 0x8A];
const WHITE: [u8; 3] = [0xFF, 0xFF, 0xFF];
const DARK_TEXT: [u8; 3] = [0x1A, 0x1A, 0x1A];

/// What one tray icon shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    Level {
        percent: u8,
        charging: bool,
        low: bool,
        /// The last reading of a device now asleep, drawn in grey unless it is low.
        stale: bool,
    },
    Asleep,
    NoDevice,
}

/// Renders `face` at the small-icon size for `dpi`. `light_taskbar` picks the colour of
/// plain digits so they read against the taskbar.
pub fn render(face: Face, dpi: u32, light_taskbar: bool) -> Icon {
    let (rgba, size) = match face {
        Face::Asleep => battery_glyph(dpi, false),
        Face::NoDevice => battery_glyph(dpi, true),
        Face::Level {
            percent,
            charging,
            low,
            stale,
        } => {
            let colour = match (charging, low, light_taskbar) {
                (true, _, false) => CHARGING_ON_DARK,
                (true, _, true) => CHARGING_ON_LIGHT,
                (false, true, false) => LOW_ON_DARK,
                (false, true, true) => LOW_ON_LIGHT,
                _ if stale => GREY,
                (false, false, true) => DARK_TEXT,
                (false, false, false) => WHITE,
            };
            digits(percent, dpi, colour)
        }
    };
    Icon::from_rgba(rgba, size as u32, size as u32).expect("icon buffer matches its size")
}

/// The battery percentage as digits in `colour`. Returns straight-alpha RGBA pixels and the
/// edge length.
fn digits(percent: u8, dpi: u32, colour: [u8; 3]) -> (Vec<u8>, usize) {
    let size = icon_size(dpi);
    let glyphs = Glyphs::fit(&percent.to_string(), size, size, size * 4 / 5);
    let mut rgba = vec![0u8; size * size * 4];
    let ox = (size - glyphs.width) / 2;
    let oy = (size - glyphs.height) / 2;
    for y in 0..glyphs.height {
        for x in 0..glyphs.width {
            let pixel = &mut rgba[((y + oy) * size + x + ox) * 4..][..4];
            pixel[..3].copy_from_slice(&colour);
            pixel[3] = glyphs.coverage[y * glyphs.width + x];
        }
    }
    (rgba, size)
}

fn icon_size(dpi: u32) -> usize {
    (unsafe { GetSystemMetricsForDpi(SM_CXSMICON, dpi) }) as usize
}

/// A grey empty battery outline, for a device asleep before any reading; crossed by a slash
/// when no supported device is connected at all.
fn battery_glyph(dpi: u32, crossed: bool) -> (Vec<u8>, usize) {
    let size = icon_size(dpi);
    let s = size as f32;
    let stroke = (s / 16.0).round().max(1.0);
    let nub_w = (stroke * 1.5).round().max(1.0);
    let margin = (s / 16.0).round();
    let body_w = s - 2.0 * margin - nub_w;
    let body_h = (s * 0.56).round();
    let x0 = margin;
    let y0 = ((s - body_h) / 2.0).round();
    let (x1, y1) = (x0 + body_w, y0 + body_h);
    let nub_h = (body_h * 0.4).round().max(2.0);
    let ny0 = (y0 + (body_h - nub_h) / 2.0).round();
    let inside =
        |x: f32, y: f32, ax: f32, ay: f32, bx: f32, by: f32| x >= ax && x < bx && y >= ay && y < by;
    // The slash runs corner to corner across the whole icon, one stroke wide.
    let (dx, dy) = (s - 2.0 * margin, -(s - 2.0 * margin));
    let length = (dx * dx + dy * dy).sqrt();

    const SAMPLES: usize = 4;
    let mut rgba = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let mut hits = 0;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let fx = x as f32 + (sx as f32 + 0.5) / SAMPLES as f32;
                    let fy = y as f32 + (sy as f32 + 0.5) / SAMPLES as f32;
                    let outline = inside(fx, fy, x0, y0, x1, y1)
                        && !inside(fx, fy, x0 + stroke, y0 + stroke, x1 - stroke, y1 - stroke);
                    let nub = inside(fx, fy, x1, ny0, x1 + nub_w, ny0 + nub_h);
                    let slash = crossed && {
                        let (px, py) = (fx - margin, fy - (s - margin));
                        let along = (px * dx + py * dy) / length;
                        let across = (px * dy - py * dx).abs() / length;
                        across <= stroke * 0.75 && (0.0..=length).contains(&along)
                    };
                    if outline || nub || slash {
                        hits += 1;
                    }
                }
            }
            let pixel = &mut rgba[(y * size + x) * 4..][..4];
            pixel[..3].copy_from_slice(&GREY);
            pixel[3] = (hits as f32 / (SAMPLES * SAMPLES) as f32 * 255.0).round() as u8;
        }
    }
    (rgba, size)
}

/// Text rasterised as an 8-bit coverage mask cropped to its ink.
struct Glyphs {
    width: usize,
    height: usize,
    coverage: Vec<u8>,
}

impl Glyphs {
    /// The largest bold system-font rendering of `text` whose ink fits `max_w` × `max_h`.
    fn fit(text: &str, size: usize, max_w: usize, max_h: usize) -> Glyphs {
        let mut em = size * 3 / 2;
        loop {
            let glyphs = Glyphs::draw(text, em as i32, size * 3);
            if em <= 6 || (glyphs.width <= max_w && glyphs.height <= max_h) {
                return glyphs;
            }
            em -= 1;
        }
    }

    fn draw(text: &str, em: i32, canvas: usize) -> Glyphs {
        let wide: Vec<u16> = text.encode_utf16().collect();
        let face: Vec<u16> = "Segoe UI\0".encode_utf16().collect();
        let mut full = vec![0u8; canvas * canvas];
        unsafe {
            let dc = CreateCompatibleDC(ptr::null_mut());
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: canvas as i32,
                biHeight: -(canvas as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..std::mem::zeroed()
            };
            let mut bits: *mut core::ffi::c_void = ptr::null_mut();
            let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, ptr::null_mut(), 0);
            let pixels = std::slice::from_raw_parts_mut(bits.cast::<u8>(), canvas * canvas * 4);
            pixels.fill(0);
            let old_bitmap = SelectObject(dc, bitmap);
            let font = CreateFontW(
                -em,
                0,
                0,
                0,
                FW_BOLD as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET.into(),
                OUT_TT_PRECIS.into(),
                CLIP_DEFAULT_PRECIS.into(),
                ANTIALIASED_QUALITY.into(),
                DEFAULT_PITCH as u32,
                face.as_ptr(),
            );
            let old_font = SelectObject(dc, font);
            SetTextColor(dc, 0x00FF_FFFF);
            SetBkMode(dc, TRANSPARENT as i32);
            let margin = (canvas / 6) as i32;
            TextOutW(dc, margin, margin, wide.as_ptr(), wide.len() as i32);
            GdiFlush();
            // White text on black: any channel is the coverage.
            for (cell, pixel) in full.iter_mut().zip(pixels.as_chunks::<4>().0) {
                *cell = pixel[0];
            }
            SelectObject(dc, old_font);
            DeleteObject(font);
            SelectObject(dc, old_bitmap);
            DeleteObject(bitmap);
            DeleteDC(dc);
        }
        Glyphs::crop(&full, canvas)
    }

    fn crop(full: &[u8], canvas: usize) -> Glyphs {
        let ink = |x: usize, y: usize| full[y * canvas + x] > 0;
        let rows: Vec<usize> = (0..canvas)
            .filter(|&y| (0..canvas).any(|x| ink(x, y)))
            .collect();
        let cols: Vec<usize> = (0..canvas)
            .filter(|&x| (0..canvas).any(|y| ink(x, y)))
            .collect();
        let (Some(&top), Some(&bottom), Some(&left), Some(&right)) =
            (rows.first(), rows.last(), cols.first(), cols.last())
        else {
            return Glyphs {
                width: 0,
                height: 0,
                coverage: Vec::new(),
            };
        };
        let width = right - left + 1;
        let height = bottom - top + 1;
        let coverage = (top..=bottom)
            .flat_map(|y| full[y * canvas + left..][..width].iter().copied())
            .collect();
        Glyphs {
            width,
            height,
            coverage,
        }
    }
}
