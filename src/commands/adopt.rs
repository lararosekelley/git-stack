use anyhow::Result;
use clap_complete::engine::ArgValueCompleter;

use crate::commands::Run;
use crate::completions;

/// Attach an existing branch to a parent.
#[derive(Debug, clap::Args)]
pub struct Adopt {
    #[arg(add = ArgValueCompleter::new(completions::branch_candidates))]
    branch: String,
    #[arg(long, add = ArgValueCompleter::new(completions::branch_candidates))]
    parent: String,
}

impl Run for Adopt {
    fn run(self) -> Result<()> {
        crate::stack::adopt_branch(&self.branch, &self.parent)
    }
}
