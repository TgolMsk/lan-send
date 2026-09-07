use crate::app::{App, os_name};

pub fn run(app: &App) -> anyhow::Result<()> {
    println!("Alias:        {}", app.alias);
    println!("Fingerprint:  {}", app.identity.fingerprint());
    println!("Device type:  headless");
    println!("Device model: {}", os_name());
    println!("Port:         {}", app.port);
    println!("Config dir:   {}", app.paths.config_dir.display());
    println!("Identity:     {}", app.paths.identity_file().display());
    Ok(())
}
