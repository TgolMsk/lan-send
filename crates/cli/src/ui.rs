//! Terminal output helpers.

use indicatif::{ProgressBar, ProgressStyle};
use lan_send_core::discovery::Device;
use lan_send_core::store::{Direction, KnownDevice, TransferRecord, TransferStatus, unix_now};
use std::io::Write;

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// A Unix timestamp as local `YYYY-MM-DD HH:MM`.
pub fn format_time(timestamp: i64) -> String {
    let Ok(utc) = time::OffsetDateTime::from_unix_timestamp(timestamp) else {
        return "-".into();
    };
    let local = time::UtcOffset::current_local_offset()
        .map(|offset| utc.to_offset(offset))
        .unwrap_or(utc);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        local.year(),
        u8::from(local.month()),
        local.day(),
        local.hour(),
        local.minute()
    )
}

/// How long ago a Unix timestamp was, coarsely.
pub fn format_ago(timestamp: i64) -> String {
    let seconds = (unix_now() - timestamp).max(0);
    match seconds {
        s if s < 60 => "just now".into(),
        s if s < 3600 => format!("{} min ago", s / 60),
        s if s < 86_400 => format!("{} h ago", s / 3600),
        s => format!("{} d ago", s / 86_400),
    }
}

pub fn print_device_table(devices: &[Device], favorites: &[String]) {
    if devices.is_empty() {
        println!("No devices found.");
        return;
    }
    println!(
        "{:<26} {:<9} {:<10} {:<22} {:<7} FINGERPRINT",
        "ALIAS", "TYPE", "MODEL", "ADDRESS", "VERSION"
    );
    for device in devices {
        let star = if favorites.iter().any(|f| device.fingerprint.matches(f)) {
            "* "
        } else {
            "  "
        };
        println!(
            "{star}{:<24} {:<9} {:<10} {:<22} {:<7} {}",
            truncate(&device.alias, 24),
            device
                .device_type
                .map(|kind| kind.to_string())
                .unwrap_or_else(|| "-".into()),
            truncate(device.device_model.as_deref().unwrap_or("-"), 10),
            format!("{}:{}", device.host, device.port),
            device.version,
            device.fingerprint.short()
        );
    }
}

pub fn print_known_devices(devices: &[KnownDevice]) {
    if devices.is_empty() {
        println!("No known devices yet. Run `lan-send discover` first.");
        return;
    }
    println!(
        "{:<26} {:<9} {:<10} {:<22} {:<12} FINGERPRINT",
        "NAME", "TYPE", "MODEL", "LAST ADDRESS", "LAST SEEN"
    );
    for device in devices {
        let star = if device.favorite { "* " } else { "  " };
        let address = match (&device.host, device.port) {
            (Some(host), Some(port)) => format!("{host}:{port}"),
            _ => "-".into(),
        };
        println!(
            "{star}{:<24} {:<9} {:<10} {:<22} {:<12} {}",
            truncate(device.display_name(), 24),
            device.device_type.as_deref().unwrap_or("-"),
            truncate(device.device_model.as_deref().unwrap_or("-"), 10),
            truncate(&address, 22),
            format_ago(device.last_seen),
            device.fingerprint.get(..8).unwrap_or(&device.fingerprint)
        );
    }
}

pub fn print_history(records: &[TransferRecord]) {
    if records.is_empty() {
        println!("No transfers yet.");
        return;
    }
    println!(
        "{:<8} {:<16} {:<4} {:<18} {:<32} {:>9} {:<9}",
        "ID", "WHEN", "DIR", "PEER", "FILE", "SIZE", "STATUS"
    );
    for record in records {
        let direction = match record.direction {
            Direction::Send => "out",
            Direction::Receive => "in",
        };
        let status = match record.status {
            TransferStatus::Finished => "ok",
            TransferStatus::Failed => "failed",
            TransferStatus::Cancelled => "cancelled",
            TransferStatus::Skipped => "skipped",
        };
        println!(
            "{:<8} {:<16} {:<4} {:<18} {:<32} {:>9} {:<9}",
            record.id.get(..8).unwrap_or(&record.id),
            format_time(record.started_at),
            direction,
            truncate(&record.peer_alias, 18),
            truncate(&record.file_name, 32),
            format_bytes(record.size),
            status
        );
    }
}

fn truncate(text: &str, max: usize) -> String {
    let mut out: String = text.chars().take(max).collect();
    if text.chars().count() > max {
        out.pop();
        out.push('…');
    }
    out
}

/// Reads one line from the terminal after printing `prompt`.
pub async fn prompt_line(prompt: &str) -> anyhow::Result<String> {
    let prompt = prompt.to_string();
    tokio::task::spawn_blocking(move || {
        let mut stderr = std::io::stderr();
        write!(stderr, "{prompt}")?;
        stderr.flush()?;
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        Ok(line.trim().to_string())
    })
    .await?
}

pub fn transfer_bar(name: &str, size: u64) -> ProgressBar {
    let bar = ProgressBar::new(size);
    let style = ProgressStyle::with_template(
        "{msg:<24} [{bar:30}] {bytes:>9}/{total_bytes:<9} {bytes_per_sec:>10} {eta:>5}",
    )
    .unwrap_or_else(|_| ProgressStyle::default_bar())
    .progress_chars("=> ");
    bar.set_style(style);
    bar.set_message(truncate(name, 24));
    bar
}
