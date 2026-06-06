use std::process::Command;

use anyhow::{Context, Result, bail};
use axoupdater::AxoUpdater;

use crate::prompt::confirm;

/// Source repository used for `--head` installs.
const REPO_URL: &str = "https://github.com/lararosekelley/git-stk";

pub fn upgrade(head: bool, force: bool, yes: bool) -> Result<()> {
    if head {
        upgrade_to_head(yes)
    } else {
        upgrade_to_latest_release(force)
    }
}

fn upgrade_to_head(yes: bool) -> Result<()> {
    println!("--head builds and installs the latest unreleased commit from {REPO_URL}");
    println!("HEAD is a pre-release snapshot: it may be broken or untested");

    if !yes && !confirm("continue? [y/N] ")? {
        println!("upgrade cancelled");
        return Ok(());
    }

    let status = Command::new("cargo")
        .args(["install", "--git", REPO_URL, "--locked", "git-stk"])
        .status()
        .context("failed to run cargo; --head requires a Rust toolchain")?;

    if !status.success() {
        bail!("cargo install exited with status {status}");
    }

    println!("installed git-stk from HEAD");
    println!("to return to the latest release, run: git stk upgrade --force");
    refresh_assets_with_new_binary();
    Ok(())
}

/// Re-render generated assets (man page) after an upgrade, using the newly
/// installed binary so the assets match its version rather than the running
/// (pre-upgrade) one. Failure is a warning, not an error: the upgrade itself
/// already succeeded.
fn refresh_assets_with_new_binary() {
    let refreshed = Command::new("git-stk")
        .args(["setup", "--refresh"])
        .status()
        .map(|status| status.success())
        .unwrap_or(false);

    if !refreshed {
        eprintln!("warning: failed to refresh generated assets; run `git stk setup` manually");
    }
}

fn upgrade_to_latest_release(force: bool) -> Result<()> {
    let mut updater = AxoUpdater::new_for("git-stk");
    updater
        .load_receipt()
        .map_err(anyhow::Error::from)
        .context(
            "no usable install receipt found; if git-stk was installed with cargo, \
             upgrade with `cargo install git-stk --locked` instead",
        )?;
    updater.always_update(force);

    match updater
        .run_sync()
        .context("failed to upgrade to the latest release")?
    {
        Some(result) => {
            let old = result
                .old_version
                .map(|version| version.to_string())
                .unwrap_or_else(|| "unknown".to_owned());
            println!("upgraded git-stk {old} -> {}", result.new_version);
            refresh_assets_with_new_binary();
        }
        None => println!(
            "git-stk {} is already the latest release",
            env!("CARGO_PKG_VERSION")
        ),
    }

    Ok(())
}
