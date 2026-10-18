use std::process::ExitCode;

use logitech_battery::HidApi;

const USAGE: &str = "Usage: peripheral-battery-tray --once";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["--once"] => print_once(),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
    }
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
    let mut lines = Vec::new();
    for device in rog_battery::discover(&api) {
        let status = match device.read_battery(&api) {
            Ok(reading) => reading.to_string(),
            Err(error) => format!("query failed: {error}"),
        };
        lines.push(format!(
            "{} ({}): {status}",
            device.name(),
            device.connection()
        ));
    }
    for device in logitech_battery::discover(&api) {
        let status = match device.read_battery(&api) {
            Ok(reading) => reading.to_string(),
            Err(error) => format!("query failed: {error}"),
        };
        lines.push(format!(
            "{} ({}): {status}",
            device.name(),
            device.connection()
        ));
    }
    if lines.is_empty() {
        println!("No supported device found.");
        return ExitCode::FAILURE;
    }
    for line in lines {
        println!("{line}");
    }
    ExitCode::SUCCESS
}
