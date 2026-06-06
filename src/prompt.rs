use std::io::{self, BufRead, Write};

use anyhow::{Context, Result};

pub fn confirm(prompt: &str) -> Result<bool> {
    print!("{prompt}");
    io::stdout().flush().context("failed to flush stdout")?;

    let mut answer = String::new();
    io::stdin()
        .lock()
        .read_line(&mut answer)
        .context("failed to read confirmation")?;

    Ok(matches!(answer.trim(), "y" | "Y" | "yes" | "Yes" | "YES"))
}
