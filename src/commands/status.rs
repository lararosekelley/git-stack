use anyhow::Result;
use clap_complete::engine::ArgValueCompleter;

use crate::commands::Run;
use crate::completions;

/// Print local and remote stack status for a branch.
#[derive(Debug, clap::Args)]
pub struct Status {
    #[arg(add = ArgValueCompleter::new(completions::branch_candidates))]
    branch: Option<String>,
}

impl Run for Status {
    fn run(self) -> Result<()> {
        crate::providers::print_status(self.branch.as_deref())
    }
}
