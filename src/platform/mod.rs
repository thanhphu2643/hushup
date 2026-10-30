//! One module per OS, each exposing the same free functions:
//! `set_dry_run`, `status`, `apply`, `enforce`, `restore`, `install_daemon`, `uninstall_daemon`.

use std::fmt;

use serde::{Deserialize, Serialize};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(not(target_os = "macos"))]
mod unsupported;
#[cfg(not(target_os = "macos"))]
pub use unsupported::*;

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Opts {
    /// Point the OS update servers at 0.0.0.0.
    pub hosts: bool,
    /// Also turn off security data (XProtect etc.) updates.
    pub security_data: bool,
}

#[derive(Serialize)]
pub struct Status {
    pub applied: bool,
    pub auto_check: bool,
    pub auto_download: bool,
    pub auto_install_os: bool,
    pub pending_updates: Vec<String>,
    pub notification_count: i64,
    pub hosts_blocked: bool,
    pub daemon_installed: bool,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let on = |b: bool| if b { "on" } else { "off" };
        writeln!(f, "blocker applied:     {}", if self.applied { "yes" } else { "no" })?;
        writeln!(f, "auto check:          {}", on(self.auto_check))?;
        writeln!(f, "auto download:       {}", on(self.auto_download))?;
        writeln!(f, "auto install OS:     {}", on(self.auto_install_os))?;
        writeln!(f, "update hosts blocked: {}", if self.hosts_blocked { "yes" } else { "no" })?;
        writeln!(f, "daemon installed:    {}", if self.daemon_installed { "yes" } else { "no" })?;
        writeln!(f, "notification count:  {}", self.notification_count)?;
        if self.pending_updates.is_empty() {
            writeln!(f, "pending updates:     none")
        } else {
            writeln!(f, "pending updates:")?;
            self.pending_updates.iter().try_for_each(|u| writeln!(f, "  - {u}"))
        }
    }
}
