use crate::app::App;
use crate::ui;

pub fn run(app: &App, limit: usize, delete: Option<String>, clear: bool) -> anyhow::Result<()> {
    if clear {
        let removed = app.db.clear_transfers()?;
        println!("Removed {removed} history entries.");
        return Ok(());
    }
    if let Some(id) = delete {
        let records = app.db.list_transfers(usize::MAX)?;
        let matching: Vec<&str> = records
            .iter()
            .filter(|record| record.id.starts_with(&id))
            .map(|record| record.id.as_str())
            .collect();
        match matching.as_slice() {
            [] => anyhow::bail!("no history entry starts with {id}"),
            [full] => {
                app.db.delete_transfer(full)?;
                println!("Deleted {full}.");
            }
            _ => anyhow::bail!(
                "{id} matches {} entries; give more characters",
                matching.len()
            ),
        }
        return Ok(());
    }
    ui::print_history(&app.db.list_transfers(limit)?);
    Ok(())
}
