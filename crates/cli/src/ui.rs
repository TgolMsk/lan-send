//! Terminal output helpers.

use indicatif::{ProgressBar, ProgressStyle};
use lan_send_core::discovery::Device;
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

pub fn print_device_table(devices: &[Device]) {
    if devices.is_empty() {
        println!("No devices found.");
        return;
    }
    println!(
        "{:<24} {:<9} {:<10} {:<22} {:<7} FINGERPRINT",
        "ALIAS", "TYPE", "MODEL", "ADDRESS", "VERSION"
    );
    for device in devices {
        println!(
            "{:<24} {:<9} {:<10} {:<22} {:<7} {}",
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
