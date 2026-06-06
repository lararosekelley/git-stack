use anyhow::Result;

use crate::commands::Run;

/// Create a new child branch from the current branch.
#[derive(Debug, clap::Args)]
pub struct New {
    branch: String,
}

impl Run for New {
    fn run(self) -> Result<()> {
        crate::stack::create_branch(&self.branch)
    }
}
