use anyhow::Result;

use crate::commands::Run;

/// Move down the stack: check out the current branch's parent.
#[derive(Debug, clap::Args)]
pub struct Down {}

impl Run for Down {
    fn run(self) -> Result<()> {
        crate::stack::checkout_parent()
    }
}
