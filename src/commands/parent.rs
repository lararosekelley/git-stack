use anyhow::Result;
use clap_complete::engine::ArgValueCompleter;

use crate::commands::Run;
use crate::completions;

/// Print a branch's stack parent.
#[derive(Debug, clap::Args)]
pub struct Parent {
    #[arg(add = ArgValueCompleter::new(completions::branch_candidates))]
    branch: Option<String>,
}

impl Run for Parent {
    fn run(self) -> Result<()> {
        crate::stack::print_parent(self.branch.as_deref())
    }
}
