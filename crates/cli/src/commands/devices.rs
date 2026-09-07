use crate::app::App;
use crate::ui;
use lan_send_core::store::KnownDevice;

pub fn run(
    app: &App,
    favorite: Option<String>,
    unfavorite: Option<String>,
    forget: Option<String>,
) -> anyhow::Result<()> {
    let devices = app.db.list_devices()?;
    if let Some(query) = favorite {
        let device = find(&devices, &query)?;
        app.db.set_favorite(&device.fingerprint, true)?;
        println!("{} is now a favorite.", device.display_name());
        return Ok(());
    }
    if let Some(query) = unfavorite {
        let device = find(&devices, &query)?;
        app.db.set_favorite(&device.fingerprint, false)?;
        println!("{} is no longer a favorite.", device.display_name());
        return Ok(());
    }
    if let Some(query) = forget {
        let device = find(&devices, &query)?;
        app.db.remove_device(&device.fingerprint)?;
        println!("Forgot {}.", device.display_name());
        return Ok(());
    }
    ui::print_known_devices(&devices);
    Ok(())
}

/// Matches by display name, alias or fingerprint prefix; must be unique.
fn find<'a>(devices: &'a [KnownDevice], query: &str) -> anyhow::Result<&'a KnownDevice> {
    let query = query.trim();
    let matches: Vec<&KnownDevice> = devices
        .iter()
        .filter(|device| {
            device.display_name().eq_ignore_ascii_case(query)
                || device.alias.eq_ignore_ascii_case(query)
                || (query.len() >= 4
                    && device
                        .fingerprint
                        .get(..query.len())
                        .is_some_and(|head| head.eq_ignore_ascii_case(query)))
        })
        .collect();
    match matches.as_slice() {
        [device] => Ok(device),
        [] => anyhow::bail!("no known device matches '{query}'"),
        _ => anyhow::bail!("'{query}' matches several devices; use a longer fingerprint prefix"),
    }
}
