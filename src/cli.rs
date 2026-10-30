use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::platform::{self, Opts};

#[derive(Parser)]
#[command(name = "hushup", version, about = "Stop OS updates and silence the \"update available\" nags")]
struct Cli {
    /// Print what would change without touching the system
    #[arg(long, global = true)]
    dry_run: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Show update checks, pending updates and blocker state
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Turn off update checks and clear the badges (needs sudo / admin)
    Apply {
        /// Don't block Apple's update servers in /etc/hosts
        #[arg(long)]
        no_hosts: bool,
        /// Also stop XProtect/security data updates (not recommended)
        #[arg(long)]
        block_security_data: bool,
        /// Also install the background guard (same as `install-daemon`)
        #[arg(long)]
        daemon: bool,
    },
    /// Re-apply what `apply` set; run by the background daemon
    Enforce,
    /// Undo everything `apply` changed (needs sudo / admin)
    Restore {
        /// Also remove the background guard (same as `uninstall-daemon`)
        #[arg(long)]
        daemon: bool,
    },
    /// Install a launchd daemon that keeps the badges cleared (needs sudo / admin)
    InstallDaemon,
    /// Remove the launchd daemon (needs sudo / admin)
    UninstallDaemon,
}

/// Parses `std::env::args` and runs the command.
pub fn run() -> Result<()> {
    let cli = Cli::parse();
    platform::set_dry_run(cli.dry_run);
    match cli.cmd {
        Cmd::Status { json } => {
            let s = platform::status()?;
            if json {
                println!("{}", serde_json::to_string(&s)?);
            } else {
                print!("{s}");
            }
            Ok(())
        }
        Cmd::Apply { no_hosts, block_security_data, daemon } => {
            platform::apply(&Opts {
                hosts: !no_hosts,
                security_data: block_security_data,
            })?;
            if daemon {
                platform::install_daemon()?;
            }
            Ok(())
        }
        Cmd::Enforce => platform::enforce(),
        Cmd::Restore { daemon } => {
            if daemon {
                platform::uninstall_daemon()?;
            }
            platform::restore()
        }
        Cmd::InstallDaemon => platform::install_daemon(),
        Cmd::UninstallDaemon => platform::uninstall_daemon(),
    }
}
