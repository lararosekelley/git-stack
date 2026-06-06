use anyhow::Result;
use clap::ArgAction;
use clap_complete::engine::ArgValueCompleter;

use crate::cli::PushMode;
use crate::commands::Run;
use crate::completions;

/// Create or update a remote review request for a branch.
#[derive(Debug, clap::Args)]
pub struct Submit {
    #[arg(add = ArgValueCompleter::new(completions::branch_candidates))]
    branch: Option<String>,
    /// Print what would change without creating or updating reviews.
    #[arg(long, action = ArgAction::SetTrue)]
    dry_run: bool,
    /// Submit the branch and its descendants parent-first.
    #[arg(long, conflicts_with = "branch")]
    stack: bool,
    /// Push branches (-u --force-with-lease) before submitting.
    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "no_push")]
    push: bool,
    /// Do not push branches, overriding stk.pushOnSubmit.
    #[arg(long, action = ArgAction::SetTrue)]
    no_push: bool,
}

impl Run for Submit {
    fn run(self) -> Result<()> {
        crate::providers::submit(
            self.branch.as_deref(),
            self.stack,
            self.dry_run,
            PushMode::from_flags(self.push, self.no_push),
        )
    }
}
