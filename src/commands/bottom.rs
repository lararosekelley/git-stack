use anyhow::Result;

use crate::commands::Run;

/// Move to the bottom of the stack: check out the branch just above the
/// trunk.
#[derive(Debug, clap::Args)]
pub struct Bottom {}

impl Run for Bottom {
    fn run(self) -> Result<()> {
        crate::stack::checkout_bottom()
    }
}
