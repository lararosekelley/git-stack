use anyhow::Result;
use clap::ArgAction;
use clap_complete::engine::ArgValueCompleter;

use crate::commands::Run;
use crate::completions;

/// Clean up local metadata for merged review requests.
#[derive(Debug, clap::Args)]
pub struct Cleanup {
    #[arg(add = ArgValueCompleter::new(completions::branch_candidates))]
    branch: Option<String>,
    /// Print what would change without updating local metadata.
    #[arg(long, action = ArgAction::SetTrue)]
    dry_run: bool,
    /// Delete cleaned merged branches after updating stack metadata.
    #[arg(long, action = ArgAction::SetTrue)]
    delete_branch: bool,
}

impl Run for Cleanup {
    fn run(self) -> Result<()> {
        crate::providers::cleanup(self.branch.as_deref(), self.dry_run, self.delete_branch)
    }
}
