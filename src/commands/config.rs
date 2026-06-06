use anyhow::Result;

use crate::commands::Run;

/// Print all stk git config settings and branch metadata.
#[derive(Debug, clap::Args)]
pub struct Config {}

impl Run for Config {
    fn run(self) -> Result<()> {
        crate::config::print_config()
    }
}
