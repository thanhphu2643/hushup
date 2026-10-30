//! Windows and Linux are on the roadmap; until then every command says so.

use anyhow::{bail, Result};

use super::{Opts, Status};

fn todo<T>() -> Result<T> {
    bail!("hushup does not support {} yet (macOS only for now)", std::env::consts::OS)
}

pub fn set_dry_run(_: bool) {}
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
