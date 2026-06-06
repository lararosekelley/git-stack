use anyhow::Result;

use crate::commands::Run;
use crate::git;
use crate::settings::SETTINGS;

/// Print all stk git config settings and branch metadata.
#[derive(Debug, clap::Args)]
pub struct Config {}

impl Run for Config {
    fn run(self) -> Result<()> {
        print_config()
    }
}

/// Print every stk-owned git config value: the `[stk]` settings (with
/// defaults for unset keys) and the per-branch stack metadata.
pub fn print_config() -> Result<()> {
    for (key, default) in SETTINGS {
        match git::config_get(key)? {
            Some(value) => println!("{key} = {value}"),
            None => println!("{key} (default: {default})"),
        }
    }

    let metadata = git::config_get_regexp(r"^branch\..*\.stk(parent|base)$")?;
    if metadata.is_empty() {
        println!();
        println!("no branch metadata (no stacked branches)");
        return Ok(());
    }

    println!();
    println!("branch metadata:");
    for (key, value) in metadata {
        println!("  {key} = {value}");
    }
    Ok(())
}
