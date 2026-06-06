use anyhow::Result;

use crate::commands::Run;

/// Print shell completions.
#[derive(Debug, clap::Args)]
pub struct Completions {
    /// Shell to print completions for.
    #[arg(value_enum)]
    shell: clap_complete::Shell,
}

impl Run for Completions {
    fn run(self) -> Result<()> {
        crate::completions::print(self.shell)
    }
}
