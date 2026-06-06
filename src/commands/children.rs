use anyhow::Result;
use clap_complete::engine::ArgValueCompleter;

use crate::commands::Run;
use crate::completions;

/// Print a branch's stack children.
#[derive(Debug, clap::Args)]
pub struct Children {
    #[arg(add = ArgValueCompleter::new(completions::branch_candidates))]
    branch: Option<String>,
}

impl Run for Children {
    fn run(self) -> Result<()> {
        crate::stack::print_children(self.branch.as_deref())
    }
}
