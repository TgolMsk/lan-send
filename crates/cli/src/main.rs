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

/// Options shared by every command. Flags override `settings.json`.
#[derive(Args, Clone, Debug)]
pub struct Globals {
    /// Name shown to other devices [default: settings, else the host name]
    #[arg(long, global = true, env = "LAN_SEND_ALIAS")]
    pub alias: Option<String>,

    /// Port of the HTTPS server [default: settings, else 53317]
    #[arg(long, global = true, env = "LAN_SEND_PORT")]
    pub port: Option<u16>,

    /// Directory holding the identity, settings and database [default: platform config dir]
    #[arg(long, global = true, env = "LAN_SEND_CONFIG_DIR", value_name = "DIR")]
    pub config_dir: Option<PathBuf>,

    /// Whether peers must present a client certificate [default: settings, else required]
    #[arg(long, global = true, value_enum)]
    pub client_certs: Option<ClientCerts>,

    /// Log verbosity: -v info, -vv debug, -vvv trace (or RUST_LOG)
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum ClientCerts {
    Required,
    Optional,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum ConflictArg {
    /// Save as "name (1).ext"
    Rename,
    /// Replace the existing file
    Overwrite,
    /// Ask on the terminal (rename when --auto-accept)
    Ask,
}

#[derive(Subcommand)]
enum Command {
    /// List devices on the local network
    Discover {
        /// How long to listen for answers
        #[arg(long, default_value_t = 4.0, value_name = "SECONDS")]
        timeout: f64,
    },
    /// Send files or folders to a device (alias, fingerprint prefix or IP[:port])
    Send {
        /// Destination device
        device: String,
        /// Files or folders to send (folders are sent recursively)
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// PIN of the receiver (asked interactively when required)
        #[arg(long)]
        pin: Option<String>,
        /// How long to wait for the device to appear
        #[arg(long, default_value_t = 10.0, value_name = "SECONDS")]
        timeout: f64,
        /// Files uploaded at the same time [default: settings, else 3]
        #[arg(long)]
        parallel: Option<usize>,
        /// Skip computing SHA-256 checksums
        #[arg(long)]
        no_checksum: bool,
        /// Also send hidden files inside folders
        #[arg(long)]
        include_hidden: bool,
    },
    /// Receive files in the foreground
    Receive {
        /// Directory to save received files into [default: settings, else Downloads]
        #[arg(long)]
        dir: Option<PathBuf>,
        /// Require senders to know this PIN [default: settings]
        #[arg(long)]
        pin: Option<String>,
        /// Accept every request without asking
        #[arg(long)]
        auto_accept: bool,
        /// Skip verifying sender-provided checksums
        #[arg(long)]
        no_verify: bool,
        /// Sub-directories: comma list of device, date, type; or "none" [default: settings]
        #[arg(long, value_name = "RULES")]
        organize: Option<String>,
        /// What to do when a file already exists [default: settings, else rename]
        #[arg(long, value_enum)]
        on_conflict: Option<ConflictArg>,
    },
    /// Show this device's identity (alias, fingerprint, config dir)
    Identity,
    /// Show the transfer history
    History {
        /// Maximum number of entries to show
        #[arg(long, default_value_t = 50)]
        limit: usize,
        /// Delete the entry with this id
        #[arg(long, value_name = "ID")]
        delete: Option<String>,
        /// Delete the whole history
        #[arg(long)]
        clear: bool,
    },
    /// Known devices: list them, mark favorites, forget entries
    Devices {
        /// Mark a device (alias, custom name or fingerprint prefix) as favorite
        #[arg(long, value_name = "DEVICE")]
        favorite: Option<String>,
        /// Remove the favorite mark
        #[arg(long, value_name = "DEVICE")]
        unfavorite: Option<String>,
        /// Forget a device
        #[arg(long, value_name = "DEVICE")]
        forget: Option<String>,
    },
    /// Clipboard synchronisation with a paired device
    Clip {
        #[command(subcommand)]
        command: ClipCommand,
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
            include_hidden,
        } => {
            let options = commands::send::SendOptions {
                device,
                paths,
                pin,
                timeout: Duration::from_secs_f64(timeout),
                parallel: parallel.unwrap_or(app.settings.parallel_uploads).max(1),
                checksum: !no_checksum && app.settings.create_checksums,
                skip_hidden: !include_hidden && app.settings.skip_hidden_files,
            };
            commands::send::run(app, options).await
        }
        Command::Receive {
            dir,
            pin,
            auto_accept,
            no_verify,
            organize,
            on_conflict,
        } => {
            let options = commands::receive::ReceiveOptions {
                dir: dir.or_else(|| app.settings.receive_dir.clone()),
                pin: pin.or_else(|| app.settings.pin.clone()),
                auto_accept,
                verify_checksums: !no_verify && app.settings.verify_checksums,
                organize: match organize {
                    Some(rules) => commands::receive::parse_organize(&rules)?,
                    None => app.settings.organize,
                },
                on_conflict: match on_conflict {
                    Some(ConflictArg::Rename) => lan_send_core::store::ConflictPolicy::Rename,
                    Some(ConflictArg::Overwrite) => lan_send_core::store::ConflictPolicy::Overwrite,
                    Some(ConflictArg::Ask) => lan_send_core::store::ConflictPolicy::Ask,
                    None => app.settings.on_conflict,
                },
            };
            commands::receive::run(app, options).await
        }
        Command::Identity => commands::identity::run(&app),
        Command::History {
            limit,
            delete,
            clear,
        } => commands::history::run(&app, limit, delete, clear),
        Command::Devices {
            favorite,
            unfavorite,
            forget,
        } => commands::devices::run(&app, favorite, unfavorite, forget),
        Command::Clip { command } => match command {
            ClipCommand::Watch { device } => not_yet(&format!("clip watch {device}"), 3),
            ClipCommand::Push { device } => not_yet(&format!("clip push {device}"), 3),
        },
    }
}

fn not_yet(command: &str, milestone: u8) -> anyhow::Result<()> {
    anyhow::bail!("`lan-send {command}` is not implemented yet (planned for milestone {milestone})")
}
