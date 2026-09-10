use crate::app::{App, os_name};

pub fn run(app: &App) -> anyhow::Result<()> {
    println!("Alias:        {}", app.alias);
    println!("Fingerprint:  {}", app.identity.fingerprint());
    println!("Device type:  headless");
    println!("Device model: {}", os_name());
    println!("Port:         {}", app.port);
    println!("Config dir:   {}", app.paths.config_dir.display());
    println!("Identity:     {}", app.paths.identity_file().display());
    // Where received files land: the setting when one is chosen, otherwise
    // the platform default (on macOS the real ~/Downloads even when sandboxed).
    let receive = app
        .settings
        .receive_dir
        .as_ref()
        .or(app.paths.download_dir.as_ref());
    match receive {
        Some(dir) => println!("Receive dir:  {}", dir.display()),
        None => println!("Receive dir:  (none configured)"),
    }
    Ok(())
}
