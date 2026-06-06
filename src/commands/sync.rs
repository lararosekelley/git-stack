use anyhow::Result;
use clap::ArgAction;
use clap_complete::engine::ArgValueCompleter;

use crate::commands::Run;
use crate::completions;

/// Sync local stack metadata from remote review requests.
#[derive(Debug, clap::Args)]
pub struct Sync {
    #[arg(add = ArgValueCompleter::new(completions::branch_candidates))]
    branch: Option<String>,
    /// Print what would change without updating local metadata.
    #[arg(long, action = ArgAction::SetTrue)]
    dry_run: bool,
}

impl Run for Sync {
    fn run(self) -> Result<()> {
        crate::providers::sync_stack(self.branch.as_deref(), self.dry_run)
    }
}
