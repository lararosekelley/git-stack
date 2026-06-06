use anyhow::Result;

use crate::commands::Run;
use crate::providers::detect_provider;

/// Print detected review provider.
#[derive(Debug, clap::Args)]
pub struct Provider {}

impl Run for Provider {
    fn run(self) -> Result<()> {
        print_provider()
    }
}

pub fn print_provider() -> Result<()> {
    let provider = detect_provider()?;
    println!("{} ({})", provider.kind, provider.source);
    Ok(())
}
