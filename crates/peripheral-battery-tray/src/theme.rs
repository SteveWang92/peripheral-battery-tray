//! The taskbar's theme and DPI, which decide how icons are drawn and menus are themed.

use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow};
use windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW;
use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

/// What the icons and menu were last drawn for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look {
    pub dpi: u32,
    pub light: bool,
}

impl Look {
    pub fn current() -> Look {
        Look {
            dpi: taskbar_dpi(),
            light: taskbar_is_light(),
        }
    }
}

/// The taskbar follows the "Windows mode" setting; a missing value means the dark default.
fn taskbar_is_light() -> bool {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
        .and_then(|key| key.get_value::<u32, _>("SystemUsesLightTheme"))
        .is_ok_and(|value| value != 0)
}

fn taskbar_dpi() -> u32 {
    let class: Vec<u16> = "Shell_TrayWnd\0".encode_utf16().collect();
    unsafe {
        let taskbar = FindWindowW(class.as_ptr(), std::ptr::null());
        if taskbar.is_null() {
            GetDpiForSystem()
        } else {
            GetDpiForWindow(taskbar)
        }
    }
}

/// Themes this process's popup menus to match the taskbar. Windows exposes this only through
/// the undocumented uxtheme exports `SetPreferredAppMode` (ordinal 135) and
/// `FlushMenuThemes` (ordinal 136), which every Windows 10 1903+ and 11 build carries.
pub fn apply_menu_theme(light: bool) {
    const FORCE_DARK: i32 = 2;
    const FORCE_LIGHT: i32 = 3;
    let name: Vec<u16> = "uxtheme.dll\0".encode_utf16().collect();
    unsafe {
        let module = LoadLibraryW(name.as_ptr());
        if module.is_null() {
            return;
        }
        let set_mode = GetProcAddress(module, 135 as *const u8);
        let flush = GetProcAddress(module, 136 as *const u8);
        if let (Some(set_mode), Some(flush)) = (set_mode, flush) {
            let set_mode: extern "system" fn(i32) -> i32 = std::mem::transmute(set_mode);
            let flush: extern "system" fn() = std::mem::transmute(flush);
            set_mode(if light { FORCE_LIGHT } else { FORCE_DARK });
            flush();
        }
    }
}
