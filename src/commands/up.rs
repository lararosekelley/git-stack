use anyhow::Result;
use clap_complete::engine::ArgValueCompleter;

use crate::commands::Run;
use crate::completions;

/// Move up the stack: check out a child of the current branch.
#[derive(Debug, clap::Args)]
pub struct Up {
    #[arg(add = ArgValueCompleter::new(completions::child_branch_candidates))]
    branch: Option<String>,
}

impl Run for Up {
    fn run(self) -> Result<()> {
        crate::stack::checkout_child(self.branch.as_deref())
    }
}
