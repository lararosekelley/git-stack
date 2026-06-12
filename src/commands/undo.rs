use anyhow::Result;

use crate::commands::Run;

/// Undo the last stack-rewriting command (restack, sync, merge, cleanup,
/// rename, absorb, new --insert, new --prepend), restoring local branch
/// tips and metadata.
#[derive(Debug, clap::Args)]
pub struct Undo {}

impl Run for Undo {
    fn run(self) -> Result<()> {
        crate::stack::undo()
    }
}
