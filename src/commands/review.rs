use anyhow::Result;
use clap_complete::engine::ArgValueCompleter;

use crate::commands::Run;
use crate::completions;

/// Print the review request for a branch.
#[derive(Debug, clap::Args)]
pub struct Review {
    #[arg(add = ArgValueCompleter::new(completions::branch_candidates))]
    branch: Option<String>,
}

impl Run for Review {
    fn run(self) -> Result<()> {
        crate::providers::print_review(self.branch.as_deref())
    }
}
