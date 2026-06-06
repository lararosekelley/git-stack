use anyhow::Result;
use clap::ArgAction;

use crate::commands::Run;

/// Upgrade git-stk to the latest release.
#[derive(Debug, clap::Args)]
pub struct Upgrade {
    /// Build and install the latest unreleased commit instead.
    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "force")]
    head: bool,
    /// Reinstall the latest release even when already up to date.
    #[arg(long, action = ArgAction::SetTrue)]
    force: bool,
    /// Skip the --head confirmation prompt.
    #[arg(long, short = 'y', action = ArgAction::SetTrue, requires = "head")]
    yes: bool,
}

impl Run for Upgrade {
    fn run(self) -> Result<()> {
        crate::upgrade::upgrade(self.head, self.force, self.yes)
    }
}
