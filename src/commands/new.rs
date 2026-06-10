use anyhow::Result;

use crate::commands::Run;

/// Create a new child branch from the current branch.
#[derive(Debug, clap::Args)]
pub struct New {
    branch: String,
    /// Insert above the current branch, moving its children onto the new one.
    #[arg(long, conflicts_with = "prepend")]
    insert: bool,
    /// Insert below the current branch, moving it onto the new one.
    #[arg(long)]
    prepend: bool,
}

impl Run for New {
    fn run(self) -> Result<()> {
        if self.insert {
            crate::stack::insert_branch(&self.branch)
        } else if self.prepend {
            crate::stack::prepend_branch(&self.branch)
        } else {
            crate::stack::create_branch(&self.branch)
        }
    }
}
