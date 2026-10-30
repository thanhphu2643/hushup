// No console window for the GUI in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! HushUp desktop app (Windows, Linux). The same exe doubles as the CLI when given
//! arguments, so elevated actions and the scheduled task need nothing else.

use hushup::platform::{self, Status};

fn main() {
    if std::env::args_os().len() > 1 {
        if let Err(e) = hushup::cli::run() {
            eprintln!("Error: {e:?}");
            std::process::exit(1);
        }
        return;
    }
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![os, status, block, restore])
        .run(tauri::generate_context!())
        .expect("error while running HushUp");
}

#[tauri::command]
fn os() -> &'static str {
    std::env::consts::OS
}

#[tauri::command]
fn status() -> Result<Status, String> {
    platform::status().map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn block() -> Result<(), String> {
    elevated("apply --daemon")
}

#[tauri::command(async)]
fn restore() -> Result<(), String> {
    elevated("restore --daemon")
}

/// Re-runs this exe as the CLI with admin rights (one UAC prompt) and waits for it.
#[cfg(windows)]
fn elevated(args: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let script = format!(
        "$p = Start-Process -FilePath '{}' -ArgumentList '{args}' -Verb RunAs -Wait -PassThru -WindowStyle Hidden; exit $p.ExitCode",
        exe.display().to_string().replace('\'', "''"),
    );
    let status = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "Not applied (permission declined, or `HushUp.exe {args}` failed with {status}). \
             Run it from an administrator terminal to see the error."
        ))
    }
}

/// Re-runs this exe as the CLI through polkit (the desktop's password prompt).
#[cfg(target_os = "linux")]
fn elevated(args: &str) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let status = std::process::Command::new("pkexec")
        .arg(exe)
        .args(args.split(' '))
        .status()
        .map_err(|e| format!("pkexec: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "Not applied (password declined, or `HushUp {args}` failed with {status}). \
             Run `sudo hushup {args}` in a terminal to see the error."
        ))
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
fn elevated(_: &str) -> Result<(), String> {
    Err("On macOS use the HushUp menu bar app.".into())
}
