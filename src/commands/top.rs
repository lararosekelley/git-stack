use anyhow::Result;

use crate::commands::Run;

/// Move to the top of the stack: check out its leaf branch.
#[derive(Debug, clap::Args)]
pub struct Top {}

impl Run for Top {
    fn run(self) -> Result<()> {
        crate::stack::checkout_top()
    }
}
