use anyhow::Result;

use crate::commands::Run;

/// Print detected review provider.
#[derive(Debug, clap::Args)]
pub struct Provider {}

impl Run for Provider {
    fn run(self) -> Result<()> {
        crate::providers::print_provider()
    }
}
