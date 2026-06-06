use anyhow::Result;
use clap::ArgAction;

use crate::commands::Run;

/// Print the current stack.
#[derive(Debug, clap::Args)]
pub struct List {
    /// Print a shareable markdown summary with PR links and states.
    #[arg(long, action = ArgAction::SetTrue)]
    markdown: bool,
}

impl Run for List {
    fn run(self) -> Result<()> {
        if self.markdown {
            crate::providers::list_markdown()
        } else {
            crate::stack::print_stack()
        }
    }
}
