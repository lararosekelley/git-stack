use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{env, fs, sync::mpsc, thread};

use anyhow::{Context, Result, bail};
use axoupdater::AxoUpdater;

use crate::prompt::confirm;

/// Source repository used for `--head` installs.
const REPO_URL: &str = "https://github.com/lararosekelley/git-stk";

/// Stamp file next to the install receipt; one release check per day.
const UPDATE_CHECK_FILE: &str = "update-check";
const CHECK_INTERVAL_SECS: u64 = 24 * 60 * 60;

/// Once a day, after a common command: print one dim line when a newer
/// release exists. Best effort with a hard time cap; anything unusual (no
/// receipt, offline, piped stderr, opt-out) prints nothing.
pub fn maybe_hint_update() {
    if !std::io::stderr().is_terminal() {
        return;
    }
    let Some(path) = update_check_path() else {
        return;
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    if !should_check(fs::read_to_string(&path).ok().as_deref(), now) {
        return;
    }
    if crate::settings::bool_setting(crate::settings::NO_UPDATE_CHECK_KEY).unwrap_or(false) {
        return;
    }

    // Stamp before checking so a failure does not retry until tomorrow.
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&path, format!("checked={now}\n"));

    // The query runs on a thread the process is free to abandon: the
    // command's work is already done, so cap the wait.
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut updater = AxoUpdater::new_for("git-stk");
        let behind =
            updater.load_receipt().is_ok() && updater.is_update_needed_sync().unwrap_or(false);
        let _ = sender.send(behind);
    });
    if let Ok(true) = receiver.recv_timeout(Duration::from_secs(2)) {
        anstream::eprintln!(
            "{}",
            crate::style::paint(
                crate::style::DIM,
                "a newer git-stk release is available - run `git stk upgrade`"
            )
        );
    }
}

fn update_check_path() -> Option<PathBuf> {
    let base = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        // Windows has no HOME; %LOCALAPPDATA% is the home for app state.
        .or_else(|| env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("git-stk").join(UPDATE_CHECK_FILE))
}

/// Whether the daily window has passed (or the stamp is missing/garbled).
fn should_check(cache: Option<&str>, now: u64) -> bool {
    let Some(cache) = cache else {
        return true;
    };
    cache
        .lines()
        .find_map(|line| line.strip_prefix("checked="))
        .and_then(|value| value.trim().parse::<u64>().ok())
        .is_none_or(|checked| now.saturating_sub(checked) >= CHECK_INTERVAL_SECS)
}

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
            anstream::println!(
                "{}",
                crate::style::success(&format!("upgraded git-stk {old} -> {}", result.new_version))
            );
            refresh_assets_with_new_binary();
        }
        None => println!(
            "git-stk {} is already the latest release",
            env!("CARGO_PKG_VERSION")
        ),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_check_when_stamp_is_missing_or_garbled() {
        assert!(should_check(None, 1_000_000));
        assert!(should_check(Some(""), 1_000_000));
        assert!(should_check(Some("checked=not-a-number\n"), 1_000_000));
    }

    #[test]
    fn should_check_once_per_day() {
        let stamp = format!("checked={}\n", 1_000_000);
        assert!(!should_check(Some(&stamp), 1_000_000 + 60));
        assert!(!should_check(
            Some(&stamp),
            1_000_000 + CHECK_INTERVAL_SECS - 1
        ));
        assert!(should_check(Some(&stamp), 1_000_000 + CHECK_INTERVAL_SECS));
    }
}
