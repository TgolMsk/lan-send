//! `lan-send` command-line interface.
//!
//! The full command surface from the brief is declared here so it is stable
//! from day one. Commands whose milestone has not landed fail with a clear
//! message instead of silently doing nothing.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "lan-send",
    version,
    about = "LocalSend-compatible LAN file and clipboard transfer"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List devices on the local network
    Discover,
    /// Send files or folders to a device (alias, fingerprint prefix or IP)
    Send {
        /// Destination device
        device: String,
        /// Files or folders to send
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
    /// Receive files in the foreground
    Receive {
        /// Directory to save received files into (default: Downloads)
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Clipboard synchronisation with a paired device
    Clip {
        #[command(subcommand)]
        command: ClipCommand,
    },
    /// Show the transfer history
    History {
        /// Maximum number of entries to show
        #[arg(long)]
        limit: Option<usize>,
    },
}

#[derive(Subcommand)]
enum ClipCommand {
    /// Keep the clipboard in sync with a paired device
    Watch {
        /// Paired device
        device: String,
    },
    /// Push the current clipboard content once
    Push {
        /// Paired device
        device: String,
    },
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Discover => not_yet("discover", 1),
        Command::Send { device, paths } => {
            not_yet(&format!("send {device} ({} path(s))", paths.len()), 1)
        }
        Command::Receive { dir } => not_yet(
            &format!("receive --dir {}", dir.unwrap_or_default().display()),
            1,
        ),
        Command::Clip { command } => match command {
            ClipCommand::Watch { device } => not_yet(&format!("clip watch {device}"), 3),
            ClipCommand::Push { device } => not_yet(&format!("clip push {device}"), 3),
        },
        Command::History { limit } => {
            not_yet(&format!("history --limit {}", limit.unwrap_or(50)), 2)
        }
    }
}

fn not_yet(command: &str, milestone: u8) -> anyhow::Result<()> {
    anyhow::bail!("`lan-send {command}` is not implemented yet (planned for milestone {milestone})")
}
