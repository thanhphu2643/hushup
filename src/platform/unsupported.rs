//! Linux is on the roadmap; until then every command says so.

use anyhow::{bail, Result};

use super::{Opts, Status};

fn todo<T>() -> Result<T> {
    bail!("hushup does not support {} yet (macOS and Windows only for now)", std::env::consts::OS)
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
