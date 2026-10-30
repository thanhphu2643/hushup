//! Windows: Windows Update group-policy values in the registry, each reversible.
//!
//! 1. Automatic Updates off (`NoAutoUpdate`); notify-only if they come back on.
//! 2. Feature updates pinned to the installed release (`TargetReleaseVersion`),
//!    so no jump to the next Windows version.
//! 3. Update notifications and restart warnings hidden (`SetUpdateNotificationLevel`).
//!
//! No hosts-file block here: Defender flags hosts entries for Microsoft domains as
//! `SettingsModifier:Win32/HostsFileHijack`. A SYSTEM scheduled task re-applies the
//! values hourly in case Windows or another tool resets them.
//! Windows Home may ignore some of these policies.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use super::{dry_run, mutate, output, remove_file, write_file, Opts, Status};

const WU: &str = r"HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate";
const AU: &str = r"HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU";
const CURRENT_VERSION: &str = r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion";
const TASK_NAME: &str = "HushUp";
/// Keeps the GUI from flashing a console window when it shells out.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum RegValue {
    Dword(u32),
    Sz(String),
}

impl RegValue {
    fn reg_args(&self) -> [String; 4] {
        match self {
            Self::Dword(d) => ["/t".into(), "REG_DWORD".into(), "/d".into(), d.to_string()],
            Self::Sz(s) => ["/t".into(), "REG_SZ".into(), "/d".into(), s.clone()],
        }
    }
}

struct Policy {
    key: &'static str,
    name: &'static str,
    value: RegValue,
}

#[derive(Default, Serialize, Deserialize)]
struct State {
    opts: Opts,
    /// Policy values before the first `apply`, keyed `key\name`; `None` means absent.
    backup: BTreeMap<String, Option<RegValue>>,
}

fn policies() -> Vec<Policy> {
    let dword = |key: &'static str, name: &'static str, d: u32| Policy { key, name, value: RegValue::Dword(d) };
    let mut list = vec![
        dword(AU, "NoAutoUpdate", 1),
        dword(AU, "AUOptions", 2), // notify before download, should auto updates come back on
        dword(AU, "NoAutoRebootWithLoggedOnUsers", 1),
        dword(WU, "SetUpdateNotificationLevel", 1),
        dword(WU, "UpdateNotificationLevel", 2), // no notifications, restart warnings included
    ];
    if let Some((product, release)) = installed_release() {
        list.push(dword(WU, "TargetReleaseVersion", 1));
        list.push(Policy { key: WU, name: "ProductVersion", value: RegValue::Sz(product) });
        list.push(Policy { key: WU, name: "TargetReleaseVersionInfo", value: RegValue::Sz(release) });
    }
    list
}

/// The release to pin feature updates to, e.g. ("Windows 11", "24H2").
fn installed_release() -> Option<(String, String)> {
    let Some(RegValue::Sz(release)) = read(CURRENT_VERSION, "DisplayVersion") else { return None };
    let Some(RegValue::Sz(build)) = read(CURRENT_VERSION, "CurrentBuild") else { return None };
    // ProductName still says "Windows 10" on Windows 11; the build number doesn't lie.
    let product = if build.parse::<u32>().ok()? >= 22000 { "Windows 11" } else { "Windows 10" };
    Some((product.into(), release))
}

pub fn status() -> Result<Status> {
    let is = |key, name, d| read(key, name) == Some(RegValue::Dword(d));
    let auto_check = !is(AU, "NoAutoUpdate", 1);
    Ok(Status {
        applied: state_file().exists(),
        auto_check,
        auto_download: auto_check && !is(AU, "AUOptions", 2),
        auto_install_os: !is(WU, "TargetReleaseVersion", 1),
        pending_updates: Vec::new(),
        notification_count: 0,
        notifications_hidden: is(WU, "SetUpdateNotificationLevel", 1) && is(WU, "UpdateNotificationLevel", 2),
        hosts_blocked: false,
        daemon_installed: output(&mut cmd("schtasks", &["/Query", "/TN", TASK_NAME])).is_ok(),
    })
}

pub fn apply(opts: &Opts) -> Result<()> {
    require_admin()?;
    let mut state = load_state().unwrap_or_default();
    for p in policies() {
        // Keep the value from before the *first* apply so restore goes all the way back.
        state.backup.entry(format!(r"{}\{}", p.key, p.name)).or_insert_with(|| read(p.key, p.name));
    }
    state.opts = opts.clone();
    write_file(state_file(), &serde_json::to_string_pretty(&state)?)?;
    enforce_with()?;
    println!("updates blocked; run `hushup install-daemon` as admin to keep them blocked");
    Ok(())
}

pub fn enforce() -> Result<()> {
    require_admin()?;
    match load_state() {
        Some(_) => enforce_with(),
        None => Ok(()), // never applied, nothing to keep
    }
}

/// Idempotent: only writes values that drifted.
fn enforce_with() -> Result<()> {
    for p in policies() {
        if read(p.key, p.name).as_ref() != Some(&p.value) {
            set(p.key, p.name, &p.value)?;
        }
    }
    Ok(())
}

pub fn restore() -> Result<()> {
    require_admin()?;
    let Some(state) = load_state() else {
        println!("nothing to restore");
        return Ok(());
    };
    for (path, saved) in &state.backup {
        let (key, name) = path.rsplit_once('\\').context("bad backup entry")?;
        match saved {
            Some(v) => set(key, name, v)?,
            None => mutate(&mut reg(&["delete", key, "/v", name, "/f"]))?,
        }
    }
    remove_file(state_file())?;
    println!("restored; Windows Update is back to its normal behaviour");
    Ok(())
}

pub fn install_daemon() -> Result<()> {
    require_admin()?;
    // Program Files is admin-only, so the SYSTEM task can't be pointed at a user-writable binary.
    let target = install_path();
    let exe = env::current_exe()?;
    if exe != target {
        if dry_run() {
            println!("would copy {} -> {}", exe.display(), target.display());
        } else {
            fs::create_dir_all(target.parent().context("install path has no parent")?)?;
            fs::copy(&exe, &target).context("installing binary")?;
        }
    }
    let run = format!("\"{}\" enforce", target.display());
    mutate(&mut cmd(
        "schtasks",
        &["/Create", "/TN", TASK_NAME, "/SC", "HOURLY", "/RU", "SYSTEM", "/RL", "HIGHEST", "/F", "/TR", &run],
    ))?;
    mutate(&mut cmd("schtasks", &["/Run", "/TN", TASK_NAME])).ok();
    Ok(())
}

pub fn uninstall_daemon() -> Result<()> {
    require_admin()?;
    mutate(&mut cmd("schtasks", &["/Delete", "/TN", TASK_NAME, "/F"])).ok(); // not installed is fine
    remove_file(install_path())
}

fn read(key: &str, name: &str) -> Option<RegValue> {
    parse_reg_query(&output(&mut reg(&["query", key, "/v", name])).ok()?, name)
}

fn set(key: &str, name: &str, value: &RegValue) -> Result<()> {
    let mut add = reg(&["add", key, "/v", name, "/f"]);
    add.args(value.reg_args());
    mutate(&mut add)
}

/// Parses the value line of `reg query`, e.g. `    NoAutoUpdate    REG_DWORD    0x1`.
fn parse_reg_query(out: &str, name: &str) -> Option<RegValue> {
    out.lines().find_map(|line| {
        let rest = line.trim().strip_prefix(name)?.trim_start();
        let (kind, data) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        let data = data.trim();
        match kind {
            "REG_DWORD" => u32::from_str_radix(data.trim_start_matches("0x"), 16).ok().map(RegValue::Dword),
            "REG_SZ" => Some(RegValue::Sz(data.to_string())),
            _ => None,
        }
    })
}

/// `reg.exe` on the 64-bit view, so a 32-bit build still writes where Windows Update reads.
fn reg(args: &[&str]) -> Command {
    let mut c = cmd("reg", args);
    c.arg("/reg:64");
    c
}

fn cmd(program: &str, args: &[&str]) -> Command {
    let mut c = Command::new(program);
    c.args(args).creation_flags(CREATE_NO_WINDOW);
    c
}

fn data_dir() -> PathBuf {
    PathBuf::from(env::var_os("ProgramData").unwrap_or_else(|| r"C:\ProgramData".into())).join("HushUp")
}

fn state_file() -> PathBuf {
    data_dir().join("state.json")
}

fn install_path() -> PathBuf {
    PathBuf::from(env::var_os("ProgramFiles").unwrap_or_else(|| r"C:\Program Files".into()))
        .join("HushUp")
        .join("hushup.exe")
}

fn load_state() -> Option<State> {
    serde_json::from_str(&fs::read_to_string(state_file()).ok()?).ok()
}

/// `net session` only succeeds in an elevated process.
fn is_admin() -> bool {
    output(&mut cmd("net", &["session"])).is_ok()
}

fn require_admin() -> Result<()> {
    if !dry_run() && !is_admin() {
        bail!("this needs administrator rights, re-run from an elevated terminal");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUERY: &str = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\Policies\\Microsoft\\Windows\\WindowsUpdate\\AU\r\n    NoAutoUpdate    REG_DWORD    0x1\r\n\r\n";

    #[test]
    fn parses_dword() {
        assert_eq!(parse_reg_query(QUERY, "NoAutoUpdate"), Some(RegValue::Dword(1)));
    }

    #[test]
    fn parses_string_with_spaces() {
        let out = "\r\nHKEY_LOCAL_MACHINE\\...\r\n    ProductVersion    REG_SZ    Windows 11\r\n";
        assert_eq!(parse_reg_query(out, "ProductVersion"), Some(RegValue::Sz("Windows 11".into())));
    }

    #[test]
    fn missing_value_is_none() {
        assert_eq!(parse_reg_query(QUERY, "AUOptions"), None);
    }
}
