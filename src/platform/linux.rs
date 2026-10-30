//! Linux (Ubuntu, Linux Mint and other apt-based desktops). Only official
//! switches, each reversible:
//!
//! 1. apt: `APT::Periodic::*` off in `/etc/apt/apt.conf.d/99hushup`, which also
//!    stops unattended-upgrades.
//! 2. Snaps: `snap refresh --hold` (Ubuntu).
//! 3. Release upgrades: `Prompt=never` in `/etc/update-manager/release-upgrades`.
//! 4. Notifiers: per-user autostart overrides (`Hidden=true`) for Ubuntu's
//!    Software Updater and Mint's Update Manager; GNOME Software stops
//!    downloading; Ubuntu Pro apt news off.
//! 5. Mint's automatic-upgrade timers, if someone turned them on.
//!
//! A systemd timer re-runs `enforce` hourly for the system-wide parts.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use super::{dry_run, mutate, output, remove_file, write_file, Opts, Status};

const APT_CONF: &str = "/etc/apt/apt.conf.d/99hushup";
const APT_CONF_BODY: &str = "// Managed by hushup; remove with `sudo hushup restore`.
APT::Periodic::Update-Package-Lists \"0\";
APT::Periodic::Download-Upgradeable-Packages \"0\";
APT::Periodic::AutocleanInterval \"0\";
APT::Periodic::Unattended-Upgrade \"0\";
";
const RELEASE_UPGRADES: &str = "/etc/update-manager/release-upgrades";
/// Session autostart entries that pop up or remind about updates.
const NOTIFIERS: &[(&str, &str)] = &[("update-notifier.desktop", "update-notifier"), ("mintupdate.desktop", "mintUpdate")];
const OVERRIDE_MARKER: &str = "X-HushUp=true";
const MINT_TIMERS: &[&str] = &["mintupdate-automation-upgrade.timer", "mintupdate-automation-autoremove.timer"];
const GNOME_SOFTWARE: &str = "org.gnome.software";
const GNOME_SOFTWARE_KEYS: &[&str] = &["download-updates", "download-updates-notify"];

const STATE_FILE: &str = "/var/lib/hushup/state.json";
const INSTALL_PATH: &str = "/usr/local/bin/hushup";
const SERVICE: &str = "/etc/systemd/system/hushup.service";
const TIMER: &str = "/etc/systemd/system/hushup.timer";

#[derive(Default, Serialize, Deserialize)]
struct State {
    opts: Opts,
    /// `Prompt=` value before the first apply.
    release_prompt: Option<String>,
    /// Mint automation timers that were enabled and we turned off.
    disabled_units: Vec<String>,
    /// The desktop user the per-user changes were made for.
    user: Option<String>,
    /// GNOME Software settings before the first apply.
    gsettings: BTreeMap<String, String>,
}

struct User {
    name: String,
    uid: String,
    home: PathBuf,
}

pub fn status() -> Result<Status> {
    let apt_on = !Path::new(APT_CONF).exists();
    let user = target_user();
    let notifiers_hidden = user.as_ref().map_or(true, |u| {
        installed_notifiers().all(|(file, _)| fs::read_to_string(autostart_path(u, file)).is_ok_and(|s| s.contains("Hidden=true")))
    });
    let gnome_quiet = user.as_ref().map_or(true, |u| {
        !has_gnome_software(u) || gsetting(u, "download-updates-notify").as_deref() == Some("false")
    });
    Ok(Status {
        applied: Path::new(STATE_FILE).exists(),
        auto_check: apt_on,
        auto_download: apt_on || (has("snap") && !snaps_held()),
        auto_install_os: release_prompt().is_some_and(|p| p != "never"),
        pending_updates: Vec::new(),
        notification_count: 0,
        notifications_hidden: notifiers_hidden && gnome_quiet,
        hosts_blocked: false,
        daemon_installed: Path::new(TIMER).exists(),
    })
}

pub fn apply(opts: &Opts) -> Result<()> {
    require_root()?;
    let mut state = load_state().unwrap_or_default();
    // Keep what was there before the *first* apply so restore goes all the way back.
    if state.release_prompt.is_none() {
        state.release_prompt = release_prompt();
    }
    for unit in MINT_TIMERS {
        if unit_enabled(unit) && !state.disabled_units.iter().any(|u| u == unit) {
            state.disabled_units.push(unit.to_string());
        }
    }
    if let Some(user) = target_user() {
        if has_gnome_software(&user) {
            for key in GNOME_SOFTWARE_KEYS {
                if let Some(v) = gsetting(&user, key) {
                    state.gsettings.entry(key.to_string()).or_insert(v);
                }
            }
        }
        state.user = Some(user.name);
    }
    state.opts = opts.clone();
    write_file(STATE_FILE, &serde_json::to_string_pretty(&state)?)?;
    enforce_with(&state)?;
    if has("pro") {
        warn(mutate(Command::new("pro").args(["config", "set", "apt_news=false"])), "Ubuntu Pro apt news");
    }
    println!("updates blocked; run `sudo hushup install-daemon` to keep them blocked");
    Ok(())
}

pub fn enforce() -> Result<()> {
    require_root()?;
    match load_state() {
        Some(state) => enforce_with(&state),
        None => Ok(()), // never applied, nothing to keep
    }
}

/// Idempotent: only touches what drifted. Per-user parts run when there is a
/// desktop user to act for (sudo/pkexec), not from the systemd timer.
fn enforce_with(state: &State) -> Result<()> {
    if fs::read_to_string(APT_CONF).ok().as_deref() != Some(APT_CONF_BODY) {
        write_file(APT_CONF, APT_CONF_BODY)?;
    }
    if release_prompt().is_some_and(|p| p != "never") {
        set_release_prompt("never")?;
    }
    if has("snap") && !snaps_held() {
        warn(mutate(Command::new("snap").args(["refresh", "--hold"])), "holding snap refreshes");
    }
    for unit in &state.disabled_units {
        if unit_enabled(unit) {
            mutate(Command::new("systemctl").args(["disable", "--now", unit]))?;
        }
    }
    if let Some(user) = target_user() {
        hide_notifiers(&user)?;
        if has_gnome_software(&user) {
            for key in GNOME_SOFTWARE_KEYS {
                if gsetting(&user, key).as_deref() != Some("false") {
                    warn(mutate(user_cmd(&user, "gsettings").args(["set", GNOME_SOFTWARE, key, "false"])), "GNOME Software");
                }
            }
        }
    }
    Ok(())
}

pub fn restore() -> Result<()> {
    require_root()?;
    let Some(state) = load_state() else {
        println!("nothing to restore");
        return Ok(());
    };
    remove_file(APT_CONF)?;
    if let Some(prompt) = &state.release_prompt {
        set_release_prompt(prompt)?;
    }
    if has("snap") {
        mutate(Command::new("snap").args(["refresh", "--unhold"])).ok(); // fails if nothing is held
    }
    if has("pro") {
        warn(mutate(Command::new("pro").args(["config", "set", "apt_news=true"])), "Ubuntu Pro apt news");
    }
    for unit in &state.disabled_units {
        mutate(Command::new("systemctl").args(["enable", "--now", unit]))?;
    }
    if let Some(user) = state.user.as_deref().and_then(lookup_user) {
        for (file, _) in NOTIFIERS {
            let path = autostart_path(&user, file);
            if fs::read_to_string(&path).is_ok_and(|s| s.contains(OVERRIDE_MARKER)) {
                remove_file(&path)?;
            }
        }
        for (key, value) in &state.gsettings {
            warn(mutate(user_cmd(&user, "gsettings").args(["set", GNOME_SOFTWARE, key, value])), "GNOME Software");
        }
    }
    remove_file(STATE_FILE)?;
    println!("restored; update notifiers come back at your next login");
    Ok(())
}

pub fn install_daemon() -> Result<()> {
    require_root()?;
    let exe = env::current_exe()?;
    if exe != Path::new(INSTALL_PATH) {
        if dry_run() {
            println!("would copy {} -> {INSTALL_PATH}", exe.display());
        } else {
            fs::create_dir_all("/usr/local/bin")?;
            fs::copy(&exe, INSTALL_PATH).context("installing binary")?;
        }
    }
    write_file(
        SERVICE,
        &format!("[Unit]\nDescription=HushUp: keep OS updates blocked\n\n[Service]\nType=oneshot\nExecStart={INSTALL_PATH} enforce\n"),
    )?;
    write_file(
        TIMER,
        "[Unit]\nDescription=Re-apply HushUp hourly\n\n[Timer]\nOnBootSec=2min\nOnUnitActiveSec=1h\n\n[Install]\nWantedBy=timers.target\n",
    )?;
    mutate(Command::new("systemctl").arg("daemon-reload"))?;
    mutate(Command::new("systemctl").args(["enable", "--now", "hushup.timer"]))
}

pub fn uninstall_daemon() -> Result<()> {
    require_root()?;
    mutate(Command::new("systemctl").args(["disable", "--now", "hushup.timer"])).ok(); // not installed is fine
    remove_file(TIMER)?;
    remove_file(SERVICE)?;
    mutate(Command::new("systemctl").arg("daemon-reload")).ok();
    remove_file(INSTALL_PATH)
}

fn installed_notifiers() -> impl Iterator<Item = &'static (&'static str, &'static str)> {
    NOTIFIERS.iter().filter(|(file, _)| Path::new("/etc/xdg/autostart").join(file).exists())
}

/// Hides each notifier from the user's session; a file the user made themselves is left alone.
fn hide_notifiers(user: &User) -> Result<()> {
    for (file, process) in installed_notifiers() {
        let path = autostart_path(user, file);
        if path.exists() {
            continue; // ours from an earlier run, or the user's own override
        }
        // Create the directory as the user so ~/.config doesn't end up owned by root.
        mutate(user_cmd(user, "mkdir").arg("-p").arg(path.parent().context("autostart path")?))?;
        write_file(&path, &format!("[Desktop Entry]\nType=Application\nName={file}\nHidden=true\n{OVERRIDE_MARKER}\n"))?;
        mutate(Command::new("chown").arg(format!("{}:", user.name)).arg(&path))?;
        // Stop the running one too; the override keeps it from coming back at login.
        mutate(Command::new("pkill").args(["-u", &user.name, "-f", process])).ok();
    }
    Ok(())
}

fn autostart_path(user: &User, file: &str) -> PathBuf {
    user.home.join(".config/autostart").join(file)
}

fn release_prompt() -> Option<String> {
    parse_release_prompt(&fs::read_to_string(RELEASE_UPGRADES).ok()?)
}

fn set_release_prompt(value: &str) -> Result<()> {
    let current = fs::read_to_string(RELEASE_UPGRADES).context("reading release-upgrades")?;
    write_file(RELEASE_UPGRADES, &with_release_prompt(&current, value))
}

fn parse_release_prompt(conf: &str) -> Option<String> {
    conf.lines().find_map(|l| l.trim().strip_prefix("Prompt=")).map(|v| v.trim().to_string())
}

fn with_release_prompt(conf: &str, value: &str) -> String {
    conf.lines()
        .map(|l| if l.trim().starts_with("Prompt=") { format!("Prompt={value}") } else { l.to_string() })
        .collect::<Vec<_>>()
        .join("\n")
        + if conf.ends_with('\n') { "\n" } else { "" }
}

/// `snap refresh --time` prints a `hold:` line once refreshes are held.
fn snaps_held() -> bool {
    output(Command::new("snap").args(["refresh", "--time"])).is_ok_and(|s| s.lines().any(|l| l.starts_with("hold:")))
}

fn unit_enabled(unit: &str) -> bool {
    output(Command::new("systemctl").args(["is-enabled", unit])).is_ok_and(|s| s.trim() == "enabled")
}

fn has_gnome_software(user: &User) -> bool {
    output(user_cmd(user, "gsettings").args(["list-keys", GNOME_SOFTWARE])).is_ok()
}

fn gsetting(user: &User, key: &str) -> Option<String> {
    output(user_cmd(user, "gsettings").args(["get", GNOME_SOFTWARE, key])).ok().map(|v| v.trim().to_string())
}

fn has(program: &str) -> bool {
    Command::new(program).arg("--version").output().is_ok_and(|o| o.status.success())
}

/// The desktop user behind `sudo` or `pkexec` (or ourselves when not root).
fn target_user() -> Option<User> {
    let name = if !is_root() {
        output(Command::new("id").arg("-un")).ok()?
    } else if let Ok(name) = env::var("SUDO_USER") {
        name
    } else {
        output(Command::new("id").args(["-un", &env::var("PKEXEC_UID").ok()?])).ok()?
    };
    lookup_user(name.trim()).filter(|u| u.name != "root")
}

fn lookup_user(name: &str) -> Option<User> {
    let entry = output(Command::new("getent").args(["passwd", name])).ok()?;
    let fields: Vec<&str> = entry.trim().split(':').collect();
    Some(User { name: name.to_string(), uid: fields.get(2)?.to_string(), home: PathBuf::from(fields.get(5)?) })
}

/// Runs `program` as the desktop user, on their session bus so gsettings reaches dconf.
fn user_cmd(user: &User, program: &str) -> Command {
    if !is_root() {
        return Command::new(program);
    }
    let mut c = Command::new("runuser");
    c.args(["-u", &user.name, "--", "env"])
        .arg(format!("DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/{}/bus", user.uid))
        .arg(program);
    c
}

/// For extras that may legitimately be unavailable (no snapd running, no desktop session).
fn warn(result: Result<()>, what: &str) {
    if let Err(e) = result {
        eprintln!("warning: skipped {what}: {e}");
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    const CONF: &str = "# Default behavior for the release upgrader.\n[DEFAULT]\nPrompt=lts\n";

    #[test]
    fn reads_release_prompt() {
        assert_eq!(parse_release_prompt(CONF).as_deref(), Some("lts"));
        assert_eq!(parse_release_prompt("[DEFAULT]\n"), None);
    }

    #[test]
    fn release_prompt_round_trips() {
        let never = with_release_prompt(CONF, "never");
        assert_eq!(parse_release_prompt(&never).as_deref(), Some("never"));
        assert_eq!(with_release_prompt(&never, "lts"), CONF);
    }
}
