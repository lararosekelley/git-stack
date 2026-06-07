use anyhow::Result;

use crate::commands::Run;
use crate::providers::detect_provider;
use crate::style;

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
    anstream::println!(
        "{} {}",
        provider.kind,
        style::dim(&format!("({})", provider.source))
    );
    Ok(())
}
