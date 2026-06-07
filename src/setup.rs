use std::env;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::CommandFactory;

use crate::cli::Cli;
use crate::prompt::confirm;

/// Marker comment written above the completion line so re-runs can detect it.
const COMPLETION_MARKER: &str = "# added by git-stk setup";

pub fn setup(yes: bool, refresh: bool) -> Result<()> {
    if refresh {
        // Re-render assets that can go stale across versions. Non-interactive;
        // run by `upgrade` via the newly installed binary. Completion wiring is
        // left alone because the rc line re-sources from the binary on every
        // shell start; missing wiring gets a hint instead of a prompt.
        install_man_page()?;
        return print_completion_hint();
    }

    install_man_page()?;
    wire_completions(yes)?;
    Ok(())
}

/// Render the man page into the XDG data directory, which is on the default
/// manpath. This makes `git stk --help` work: git resolves it as `man git-stk`.
fn install_man_page() -> Result<()> {
    if cfg!(windows) {
        return Ok(());
    }

    let dir = man_dir()?;
    fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;

    let mut buffer = Vec::new();
    clap_mangen::Man::new(Cli::command())
        .render(&mut buffer)
        .context("failed to render man page")?;

    let path = dir.join("git-stk.1");
    fs::write(&path, buffer).with_context(|| format!("failed to write {}", path.display()))?;
    println!("installed man page to {}", path.display());
    Ok(())
}

fn man_dir() -> Result<PathBuf> {
    let data_home = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .context("cannot locate a data directory; set HOME or XDG_DATA_HOME")?;
    Ok(data_home.join("man/man1"))
}

/// Append a completion-sourcing line to the detected shell's rc file, once.
fn wire_completions(yes: bool) -> Result<()> {
    let Some((shell, rc_path, line)) = completion_target()? else {
        println!("could not detect a supported shell from $SHELL");
        println!("see the README for manual completion setup");
        return Ok(());
    };

    let existing = match fs::read_to_string(&rc_path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", rc_path.display()));
        }
    };

    if existing.contains(COMPLETION_MARKER) || existing.contains("git stk completions") {
        println!(
            "{shell} completions already configured in {}",
            rc_path.display()
        );
        return Ok(());
    }

    if !yes
        && !confirm(&format!(
            "append completion setup to {}? [y/N] ",
            rc_path.display()
        ))?
    {
        println!("skipped completion setup");
        println!("to configure manually, add this to {}:", rc_path.display());
        println!("  {line}");
        return Ok(());
    }

    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(&format!("\n{COMPLETION_MARKER}\n{line}\n"));
    fs::write(&rc_path, updated)
        .with_context(|| format!("failed to write {}", rc_path.display()))?;
    println!("added {shell} completion setup to {}", rc_path.display());
    Ok(())
}

/// Point at `git stk setup` when the detected shell has no completion
/// wiring yet. Used after upgrades, where prompting is not an option.
fn print_completion_hint() -> Result<()> {
    let Some((shell, rc_path, line)) = completion_target()? else {
        return Ok(());
    };

    let configured = fs::read_to_string(&rc_path)
        .map(|rc| rc.contains(COMPLETION_MARKER) || rc.contains("git stk completions"))
        .unwrap_or(false);
    if configured {
        return Ok(());
    }

    println!(
        "{shell} completions are not configured; run `git stk setup`, \
         or add this to {}:",
        rc_path.display()
    );
    println!("  {line}");
    Ok(())
}

/// Resolve (shell name, rc file, completion line) from $SHELL. The lines
/// guard on the binary existing so a removed git-stk never breaks shell
/// startup.
fn completion_target() -> Result<Option<(&'static str, PathBuf, &'static str)>> {
    let shell = env::var("SHELL").unwrap_or_default();
    let shell = shell.rsplit('/').next().unwrap_or_default();

    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .context("cannot locate home directory; set HOME")?;

    let target = match shell {
        "bash" => Some((
            "bash",
            home.join(".bashrc"),
            "command -v git-stk >/dev/null && source <(git stk completions bash)",
        )),
        "zsh" => Some((
            "zsh",
            home.join(".zshrc"),
            "command -v git-stk >/dev/null && source <(git stk completions zsh)",
        )),
        "fish" => Some((
            "fish",
            home.join(".config/fish/config.fish"),
            "command -q git-stk; and git stk completions fish | source",
        )),
        _ => None,
    };
    Ok(target)
}
