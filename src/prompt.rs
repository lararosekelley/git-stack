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

/// Like [`confirm`], but a bare Enter - or EOF, i.e. a non-interactive run -
/// counts as yes. For prompts whose safe default is to proceed.
pub fn confirm_default_yes(prompt: &str) -> Result<bool> {
    print!("{prompt}");
    io::stdout().flush().context("failed to flush stdout")?;

    let mut answer = String::new();
    io::stdin()
        .lock()
        .read_line(&mut answer)
        .context("failed to read confirmation")?;

    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "" | "y" | "yes"
    ))
}

/// Number the options and read a 1-based choice from stdin. EOF or input
/// that is not a valid number picks nothing, so non-interactive callers
/// fall through to their error path.
pub fn pick(title: &str, options: &[String]) -> Result<Option<usize>> {
    anstream::eprintln!("{title}");
    for (index, option) in options.iter().enumerate() {
        let number = crate::style::paint(crate::style::DIM, &format!("{}.", index + 1));
        anstream::eprintln!("  {number} {option}");
    }
    eprint!("pick [1-{}]: ", options.len());
    io::stderr().flush().context("failed to flush stderr")?;

    let mut answer = String::new();
    io::stdin()
        .lock()
        .read_line(&mut answer)
        .context("failed to read choice")?;

    Ok(answer
        .trim()
        .parse::<usize>()
        .ok()
        .filter(|choice| (1..=options.len()).contains(choice))
        .map(|choice| choice - 1))
}
