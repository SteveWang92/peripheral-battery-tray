// A GUI-subsystem binary, so launching the tray opens no console window.
#![windows_subsystem = "windows"]

mod app_icon;
mod config;
mod icon;
mod poller;
mod popup;
mod strings;
mod system;
mod theme;
mod tray;

use std::process::ExitCode;

use logitech_battery::HidApi;
use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};

const USAGE: &str = "Usage: peripheral-battery-tray [--once]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        [] => run_tray(),
        ["--once"] => {
            attach_console();
            print_once()
        }
        _ => {
            attach_console();
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}

/// Lets command-line output reach the terminal that started this GUI-subsystem binary.
fn attach_console() {
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}

/// Records a panic in `crash.log` beside the battery log: the tray has no console, and a
/// panic inside a Windows callback ends the process without a trace.
fn log_panics() {
    std::panic::set_hook(Box::new(|info| {
        let Some(dir) = std::env::var_os("LOCALAPPDATA") else {
            return;
        };
        let dir = std::path::PathBuf::from(dir).join("peripheral-battery-tray");
        let _ = std::fs::create_dir_all(&dir);
        let line = format!(
            "{}  {info}\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        );
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("crash.log"))
            .and_then(|mut file| std::io::Write::write_all(&mut file, line.as_bytes()));
    }));
}

fn run_tray() -> ExitCode {
    let name: Vec<u16> = "Local\\peripheral-battery-tray\0".encode_utf16().collect();
    // The handle stays open for the life of the process; a second launch finds it and exits.
    let mutex = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    if !mutex.is_null() && unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        return ExitCode::SUCCESS;
    }
    log_panics();
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let Ok(api) = HidApi::new() else {
        return ExitCode::FAILURE;
    };
    tray::run(api);
    ExitCode::SUCCESS
}

/// Prints every detected device's battery and exits.
fn print_once() -> ExitCode {
    let api = match HidApi::new() {
        Ok(api) => api,
        Err(error) => {
            eprintln!("Cannot open the HID subsystem: {error}");
            return ExitCode::FAILURE;
        }
    };
    let rog = rog_battery::discover(&api);
    let logitech = logitech_battery::discover(&api);
    let lines: Vec<String> = poller::every_device(&rog, &logitech)
        .map(|device| device.query(&api).line)
        .collect();
    if lines.is_empty() {
        println!("No supported device found.");
        return ExitCode::FAILURE;
    }
    for line in lines {
        println!("{line}");
    }
    ExitCode::SUCCESS
}
