use anyhow::Result;
use clap::ArgAction;

use crate::commands::Run;

/// Rebuild or verify local stack metadata from reviews and ancestry.
#[derive(Debug, clap::Args)]
pub struct Repair {
    /// Print what would change without updating local metadata.
    #[arg(long, action = ArgAction::SetTrue)]
    dry_run: bool,
}

impl Run for Repair {
    fn run(self) -> Result<()> {
        crate::providers::repair(self.dry_run)
    }
}
