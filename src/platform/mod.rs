//! One module per OS, each exposing the same free functions:
//! `status`, `apply`, `enforce`, `restore`, `install_daemon`, `uninstall_daemon`.
//! Shared plumbing (dry-run, running commands, writing files) lives here.

#![cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]

use std::fmt;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;

#[cfg(not(any(target_os = "macos", windows)))]
mod unsupported;
#[cfg(not(any(target_os = "macos", windows)))]
pub use unsupported::*;

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Opts {
    /// Point the OS update servers at 0.0.0.0 (macOS only).
    pub hosts: bool,
    /// Also turn off security data (XProtect etc.) updates (macOS only).
    pub security_data: bool,
}

#[derive(Serialize)]
pub struct Status {
    pub applied: bool,
    pub auto_check: bool,
    pub auto_download: bool,
    /// macOS: installs macOS updates on its own. Windows: may move to a newer feature release.
    pub auto_install_os: bool,
    pub pending_updates: Vec<String>,
    pub notification_count: i64,
    /// No update badge/toast is being shown (or allowed, on Windows).
    pub notifications_hidden: bool,
    pub hosts_blocked: bool,
    pub daemon_installed: bool,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let on = |b: bool| if b { "on" } else { "off" };
        let yes = |b: bool| if b { "yes" } else { "no" };
        writeln!(f, "blocker applied:      {}", yes(self.applied))?;
        writeln!(f, "auto check:           {}", on(self.auto_check))?;
        writeln!(f, "auto download:        {}", on(self.auto_download))?;
        writeln!(f, "auto install OS:      {}", on(self.auto_install_os))?;
        writeln!(f, "notifications hidden: {}", yes(self.notifications_hidden))?;
        if cfg!(target_os = "macos") {
            writeln!(f, "update hosts blocked: {}", yes(self.hosts_blocked))?;
            writeln!(f, "notification count:   {}", self.notification_count)?;
        }
        writeln!(f, "daemon installed:     {}", yes(self.daemon_installed))?;
        if self.pending_updates.is_empty() {
            writeln!(f, "pending updates:      none")
        } else {
            writeln!(f, "pending updates:")?;
            self.pending_updates.iter().try_for_each(|u| writeln!(f, "  - {u}"))
        }
    }
}

static DRY_RUN: AtomicBool = AtomicBool::new(false);

pub fn set_dry_run(on: bool) {
    DRY_RUN.store(on, Ordering::Relaxed);
}

fn dry_run() -> bool {
    DRY_RUN.load(Ordering::Relaxed)
}

fn output(cmd: &mut Command) -> Result<String> {
    let out = cmd.output().with_context(|| format!("running {cmd:?}"))?;
    if !out.status.success() {
        bail!("{cmd:?} failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Every system change goes through these helpers so `--dry-run` covers them all.
fn mutate(cmd: &mut Command) -> Result<()> {
    if dry_run() {
        println!("would run: {cmd:?}");
        return Ok(());
    }
    output(cmd).map(drop)
}

fn write_file(path: impl AsRef<Path>, contents: &str) -> Result<()> {
    let path = path.as_ref();
    if dry_run() {
        println!("would write {}:\n{contents}", path.display());
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, contents).with_context(|| format!("writing {}", path.display()))
}

fn remove_file(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    if dry_run() {
        println!("would remove {}", path.display());
        return Ok(());
    }
    match fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e).with_context(|| format!("removing {}", path.display())),
        _ => Ok(()),
    }
}
