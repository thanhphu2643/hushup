//! Anything that isn't macOS, Windows or Linux.

use anyhow::{bail, Result};

use super::{Opts, Status};

fn todo<T>() -> Result<T> {
    bail!("hushup does not support {} yet (macOS, Windows and Linux only)", std::env::consts::OS)
}

pub fn status() -> Result<Status> {
    todo()
}
pub fn apply(_: &Opts) -> Result<()> {
    todo()
}
pub fn enforce() -> Result<()> {
    todo()
}
pub fn restore() -> Result<()> {
    todo()
}
pub fn install_daemon() -> Result<()> {
    todo()
}
pub fn uninstall_daemon() -> Result<()> {
    todo()
}
