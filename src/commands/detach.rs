use anyhow::Result;
use clap_complete::engine::ArgValueCompleter;

use crate::commands::Run;
use crate::completions;

/// Remove stack parent metadata from a branch.
#[derive(Debug, clap::Args)]
pub struct Detach {
    #[arg(add = ArgValueCompleter::new(completions::branch_candidates))]
    branch: Option<String>,
}

impl Run for Detach {
    fn run(self) -> Result<()> {
        crate::stack::detach_branch(self.branch.as_deref())
    }
}
