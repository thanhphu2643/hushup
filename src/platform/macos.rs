//! macOS: three layers, each reversible.
//!
//! 1. Preferences: turn off softwareupdated's automatic check/download/install.
//! 2. Badges: drop `RecommendedUpdates` (what System Settings counts for the red
//!    "1" and the "Software Update Available" row) plus the per-user badge keys.
//! 3. Hosts: point Apple's update servers at 0.0.0.0 so a scan can't refill them.
//!
//! A launchd daemon re-runs `enforce` whenever softwareupdated rewrites its prefs.

use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{bail, Context, Result};
use plist::{Dictionary, Value};
use serde::{Deserialize, Serialize};

use super::{Opts, Status};

const SU_SYSTEM: &str = "/Library/Preferences/com.apple.SoftwareUpdate";
const SU_USER: &str = "com.apple.SoftwareUpdate";
const SYSPREFS_USER: &str = "com.apple.systempreferences";

/// Keys that let softwareupdated check, download or install on its own.
const AUTO_KEYS: &[&str] = &["AutomaticCheckEnabled", "AutomaticDownload", "AutomaticallyInstallMacOSUpdates"];
/// XProtect / security data. Left alone unless asked.
const SECURITY_KEYS: &[&str] = &["ConfigDataInstall", "CriticalUpdateInstall"];
/// Per-user keys behind the red badge, by OS version.
const BADGE_KEYS: &[(&str, &str)] = &[
    (SU_USER, "AvailableUpdatesNotificationCountKey"),
    (SYSPREFS_USER, "AttentionPrefBundleIDs"),
];

/// Software Update catalog, CDN and the gdmf/pallas OS-update feed.
/// mesu.apple.com is left out on purpose: it also serves non-update assets.
const UPDATE_HOSTS: &[&str] = &[
    "swscan.apple.com",
    "swdist.apple.com",
    "swcdn.apple.com",
    "updates.cdn-apple.com",
    "gdmf.apple.com",
];
const HOSTS: &str = "/etc/hosts";
const HOSTS_BEGIN: &str = "# >>> hushup";
const HOSTS_END: &str = "# <<< hushup";

const STATE_DIR: &str = "/Library/Application Support/hushup";
const STATE_FILE: &str = "/Library/Application Support/hushup/state.json";
const DAEMON_LABEL: &str = "io.github.hushup";
const DAEMON_PLIST: &str = "/Library/LaunchDaemons/io.github.hushup.plist";
const INSTALL_PATH: &str = "/usr/local/bin/hushup";

static DRY_RUN: AtomicBool = AtomicBool::new(false);

pub fn set_dry_run(on: bool) {
    DRY_RUN.store(on, Ordering::Relaxed);
}

fn dry_run() -> bool {
    DRY_RUN.load(Ordering::Relaxed)
}

#[derive(Default, Serialize, Deserialize)]
struct State {
    opts: Opts,
    /// System pref values before the first `apply`; `None` means the key was absent.
    backup: BTreeMap<String, Option<Saved>>,
}

#[derive(Clone, Serialize, Deserialize)]
enum Saved {
    Bool(bool),
    Int(i64),
    Str(String),
}

impl Saved {
    fn from_value(v: &Value) -> Option<Self> {
        match v {
            Value::Boolean(b) => Some(Self::Bool(*b)),
            Value::Integer(i) => i.as_signed().map(Self::Int),
            Value::String(s) => Some(Self::Str(s.clone())),
            _ => None,
        }
    }

    fn defaults_args(&self) -> [String; 2] {
        match self {
            Self::Bool(b) => ["-bool".into(), b.to_string()],
            Self::Int(i) => ["-int".into(), i.to_string()],
            Self::Str(s) => ["-string".into(), s.clone()],
        }
    }
}

pub fn status() -> Result<Status> {
    let su = export(Command::new("defaults"), SU_SYSTEM);
    let on = |key: &str| su.get(key).and_then(Value::as_boolean).unwrap_or(true);
    let pending_updates = su
        .get("RecommendedUpdates")
        .and_then(Value::as_array)
        .map(|updates| {
            updates
                .iter()
                .filter_map(|u| u.as_dictionary()?.get("Display Name")?.as_string().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let notification_count = user_defaults()
        .map(|cmd| export(cmd, SU_USER))
        .and_then(|d| d.get("AvailableUpdatesNotificationCountKey")?.as_signed_integer())
        .unwrap_or(0);
    Ok(Status {
        applied: Path::new(STATE_FILE).exists(),
        auto_check: on("AutomaticCheckEnabled"),
        auto_download: on("AutomaticDownload"),
        auto_install_os: on("AutomaticallyInstallMacOSUpdates"),
        pending_updates,
        notification_count,
        hosts_blocked: fs::read_to_string(HOSTS).is_ok_and(|s| s.contains(HOSTS_BEGIN)),
        daemon_installed: Path::new(DAEMON_PLIST).exists(),
    })
}

pub fn apply(opts: &Opts) -> Result<()> {
    require_root()?;
    let mut state = load_state().unwrap_or_default();
    let su = export(Command::new("defaults"), SU_SYSTEM);
    for key in pref_keys(opts) {
        // Keep the value from before the *first* apply so restore goes all the way back.
        state.backup.entry(key.to_string()).or_insert_with(|| su.get(key).and_then(Saved::from_value));
    }
    state.opts = opts.clone();
    write_file(STATE_FILE, &serde_json::to_string_pretty(&state)?)?;
    enforce_with(&state)?;
    println!("updates blocked; run `sudo hushup install-daemon` to keep the badge away");
    Ok(())
}

pub fn enforce() -> Result<()> {
    require_root()?;
    match load_state() {
        Some(state) => enforce_with(&state),
        None => Ok(()), // never applied, nothing to keep
    }
}

/// Idempotent: only writes what drifted, so the WatchPaths daemon doesn't loop on its own writes.
fn enforce_with(state: &State) -> Result<()> {
    let su = export(Command::new("defaults"), SU_SYSTEM);
    for key in pref_keys(&state.opts) {
        if su.get(key).and_then(Value::as_boolean) != Some(false) {
            mutate(Command::new("defaults").args(["write", SU_SYSTEM, key, "-bool", "false"]))?;
        }
    }
    if su.get("RecommendedUpdates").and_then(Value::as_array).is_some_and(|a| !a.is_empty()) {
        mutate(Command::new("defaults").args(["delete", SU_SYSTEM, "RecommendedUpdates"]))?;
    }
    set_hosts_block(state.opts.hosts)?;
    clear_user_badge()
}

pub fn restore() -> Result<()> {
    require_root()?;
    let Some(state) = load_state() else {
        println!("nothing to restore");
        return Ok(());
    };
    for (key, saved) in &state.backup {
        let mut cmd = Command::new("defaults");
        match saved {
            Some(v) => cmd.args(["write", SU_SYSTEM, key]).args(v.defaults_args()),
            None => cmd.args(["delete", SU_SYSTEM, key]),
        };
        mutate(&mut cmd)?;
    }
    set_hosts_block(false)?;
    remove_file(STATE_FILE)?;
    println!("restored; macOS will show available updates again after its next check");
    Ok(())
}

pub fn install_daemon() -> Result<()> {
    require_root()?;
    let exe = std::env::current_exe()?;
    if exe != Path::new(INSTALL_PATH) {
        if dry_run() {
            println!("would copy {} -> {INSTALL_PATH}", exe.display());
        } else {
            fs::create_dir_all("/usr/local/bin")?;
            fs::copy(&exe, INSTALL_PATH).context("installing binary")?;
        }
    }
    write_file(DAEMON_PLIST, &daemon_plist())?;
    mutate(Command::new("launchctl").args(["bootout", &format!("system/{DAEMON_LABEL}")])).ok(); // not loaded yet is fine
    mutate(Command::new("launchctl").args(["bootstrap", "system", DAEMON_PLIST]))
}

pub fn uninstall_daemon() -> Result<()> {
    require_root()?;
    mutate(Command::new("launchctl").args(["bootout", &format!("system/{DAEMON_LABEL}")])).ok();
    remove_file(DAEMON_PLIST)?;
    remove_file(INSTALL_PATH)
}

fn daemon_plist() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>{DAEMON_LABEL}</string>
    <key>ProgramArguments</key>
    <array><string>{INSTALL_PATH}</string><string>enforce</string></array>
    <key>RunAtLoad</key><true/>
    <key>StartInterval</key><integer>900</integer>
    <key>WatchPaths</key>
    <array><string>{SU_SYSTEM}.plist</string></array>
</dict>
</plist>
"#
    )
}

fn pref_keys(opts: &Opts) -> impl Iterator<Item = &'static str> {
    let security: &[&str] = if opts.security_data { SECURITY_KEYS } else { &[] };
    AUTO_KEYS.iter().chain(security).copied()
}

fn clear_user_badge() -> Result<()> {
    let mut changed = false;
    for (domain, key) in BADGE_KEYS {
        let Some(cmd) = user_defaults() else { return Ok(()) };
        if export(cmd, domain).contains_key(key) {
            if let Some(mut cmd) = user_defaults() {
                mutate(cmd.args(["delete", domain, key]))?;
                changed = true;
            }
        }
    }
    if changed {
        mutate(Command::new("killall").arg("Dock")).ok(); // Dock redraws the badge on relaunch
    }
    Ok(())
}

fn set_hosts_block(on: bool) -> Result<()> {
    let current = fs::read_to_string(HOSTS).context("reading /etc/hosts")?;
    let wanted = with_hosts_block(&current, on);
    if wanted == current {
        return Ok(());
    }
    write_file(HOSTS, &wanted)?;
    mutate(Command::new("dscacheutil").arg("-flushcache"))?;
    mutate(Command::new("killall").args(["-HUP", "mDNSResponder"])).ok();
    Ok(())
}

fn with_hosts_block(hosts: &str, on: bool) -> String {
    let mut out = match (hosts.find(HOSTS_BEGIN), hosts.find(HOSTS_END)) {
        (Some(a), Some(b)) if b > a => {
            let end = b + HOSTS_END.len();
            let end = hosts[end..].find('\n').map_or(hosts.len(), |i| end + i + 1);
            format!("{}{}", &hosts[..a], &hosts[end..])
        }
        _ => hosts.to_string(),
    };
    if on {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out += &format!("{HOSTS_BEGIN} managed by hushup, remove with `sudo hushup restore`\n");
        for h in UPDATE_HOSTS {
            out += &format!("0.0.0.0 {h}\n:: {h}\n");
        }
        out += HOSTS_END;
        out.push('\n');
    }
    out
}

/// `defaults` aimed at the logged-in user's domain. As root that means running it
/// as the console user; `None` when nobody is logged in.
fn user_defaults() -> Option<Command> {
    if !is_root() {
        return Some(Command::new("defaults"));
    }
    let user = std::env::var("SUDO_USER")
        .ok()
        .or_else(|| output(Command::new("stat").args(["-f%Su", "/dev/console"])).ok())
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty() && u != "root")?;
    let uid = output(Command::new("id").args(["-u", &user])).ok()?;
    let mut cmd = Command::new("launchctl");
    cmd.args(["asuser", uid.trim(), "sudo", "-u", &user, "defaults"]);
    Some(cmd)
}

/// Reads a domain through cfprefsd (not the file), so we see what softwareupdated sees.
fn export(mut cmd: Command, domain: &str) -> Dictionary {
    cmd.args(["export", domain, "-"]);
    output(&mut cmd)
        .ok()
        .and_then(|xml| Value::from_reader(Cursor::new(xml)).ok())
        .and_then(Value::into_dictionary)
        .unwrap_or_default()
}

fn load_state() -> Option<State> {
    serde_json::from_str(&fs::read_to_string(STATE_FILE).ok()?).ok()
}

fn is_root() -> bool {
    output(Command::new("id").arg("-u")).is_ok_and(|uid| uid.trim() == "0")
}

fn require_root() -> Result<()> {
    if !dry_run() && !is_root() {
        bail!("this needs root, re-run with sudo");
    }
    Ok(())
}

fn output(cmd: &mut Command) -> Result<String> {
    let out = cmd.output().with_context(|| format!("running {cmd:?}"))?;
    if !out.status.success() {
        bail!("{cmd:?} failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Every system change goes through these three so `--dry-run` covers them all.
fn mutate(cmd: &mut Command) -> Result<()> {
    if dry_run() {
        println!("would run: {cmd:?}");
        return Ok(());
    }
    output(cmd).map(drop)
}

fn write_file(path: &str, contents: &str) -> Result<()> {
    if dry_run() {
        println!("would write {path}:\n{contents}");
        return Ok(());
    }
    if path == STATE_FILE {
        fs::create_dir_all(STATE_DIR)?;
    }
    fs::write(path, contents).with_context(|| format!("writing {path}"))
}

fn remove_file(path: &str) -> Result<()> {
    if dry_run() {
        println!("would remove {path}");
        return Ok(());
    }
    match fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e).with_context(|| format!("removing {path}")),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "127.0.0.1 localhost\n::1 localhost";

    #[test]
    fn hosts_block_round_trips() {
        let on = with_hosts_block(BASE, true);
        assert!(on.contains("0.0.0.0 swscan.apple.com\n"));
        assert_eq!(with_hosts_block(&on, true), on, "applying twice is a no-op");
        assert_eq!(with_hosts_block(&on, false), format!("{BASE}\n"));
    }

    #[test]
    fn hosts_block_keeps_lines_after_it() {
        let on = with_hosts_block(BASE, true) + "10.0.0.1 nas\n";
        assert_eq!(with_hosts_block(&on, false), format!("{BASE}\n10.0.0.1 nas\n"));
    }

    #[test]
    fn hosts_untouched_without_block() {
        assert_eq!(with_hosts_block(BASE, false), BASE);
    }
}
