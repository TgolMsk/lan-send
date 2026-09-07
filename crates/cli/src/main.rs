//! `lan-send` command-line interface (ADR-0005).

mod app;
mod commands;
mod ui;

use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Parser)]
#[command(
    name = "lan-send",
    version,
    about = "LocalSend-compatible LAN file and clipboard transfer"
)]
struct Cli {
    #[command(flatten)]
    globals: Globals,

    #[command(subcommand)]
    command: Command,
}

/// Options shared by every command.
#[derive(Args, Clone, Debug)]
pub struct Globals {
    /// Name shown to other devices [default: the host name]
    #[arg(long, global = true, env = "LAN_SEND_ALIAS")]
    pub alias: Option<String>,

    /// Port of the HTTPS server [default: 53317]
    #[arg(long, global = true, env = "LAN_SEND_PORT")]
    pub port: Option<u16>,

    /// Directory holding the identity and settings [default: platform config dir]
    #[arg(long, global = true, env = "LAN_SEND_CONFIG_DIR", value_name = "DIR")]
    pub config_dir: Option<PathBuf>,

    /// Whether peers must present a client certificate (official 1.18+ does)
    #[arg(long, global = true, value_enum, default_value_t = ClientCerts::Required)]
    pub client_certs: ClientCerts,

    /// Log verbosity: -v info, -vv debug, -vvv trace (or RUST_LOG)
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum ClientCerts {
    Required,
    Optional,
}

#[derive(Subcommand)]
enum Command {
    /// List devices on the local network
    Discover {
        /// How long to listen for answers
        #[arg(long, default_value_t = 4.0, value_name = "SECONDS")]
        timeout: f64,
    },
    /// Send files to a device (alias, fingerprint prefix or IP[:port])
    Send {
        /// Destination device
        device: String,
        /// Files to send
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// PIN of the receiver (asked interactively when required)
        #[arg(long)]
        pin: Option<String>,
        /// How long to wait for the device to appear
        #[arg(long, default_value_t = 10.0, value_name = "SECONDS")]
        timeout: f64,
        /// Files uploaded at the same time
        #[arg(long, default_value_t = 3)]
        parallel: usize,
        /// Skip computing SHA-256 checksums
        #[arg(long)]
        no_checksum: bool,
    },
    /// Receive files in the foreground
    Receive {
        /// Directory to save received files into [default: Downloads]
        #[arg(long)]
        dir: Option<PathBuf>,
        /// Require senders to know this PIN
        #[arg(long)]
        pin: Option<String>,
        /// Accept every request without asking
        #[arg(long)]
        auto_accept: bool,
        /// Skip verifying sender-provided checksums
        #[arg(long)]
        no_verify: bool,
    },
    /// Show this device's identity (alias, fingerprint, config dir)
    Identity,
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
    let cli = Cli::parse();
    app::init_logging(cli.globals.verbose);
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(run(cli))
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    let app = app::App::load(&cli.globals)?;
    match cli.command {
        Command::Discover { timeout } => {
            commands::discover::run(app, Duration::from_secs_f64(timeout)).await
        }
        Command::Send {
            device,
            paths,
            pin,
            timeout,
            parallel,
            no_checksum,
        } => {
            commands::send::run(
                app,
                commands::send::SendOptions {
                    device,
                    paths,
                    pin,
                    timeout: Duration::from_secs_f64(timeout),
                    parallel: parallel.max(1),
                    checksum: !no_checksum,
                },
            )
            .await
        }
        Command::Receive {
            dir,
            pin,
            auto_accept,
            no_verify,
        } => {
            commands::receive::run(
                app,
                commands::receive::ReceiveOptions {
                    dir,
                    pin,
                    auto_accept,
                    verify_checksums: !no_verify,
                },
            )
            .await
        }
        Command::Identity => commands::identity::run(&app),
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
