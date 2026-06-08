//! Undo support: capture the current stack's branch tips and metadata
//! before a mutating command rewrites them, and restore that capture on
//! `git stk undo`. Local only - pushes and platform merges are not
//! reverted.

use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use serde_json::{Value, json};

use super::{base_of, branch_and_descendants, parent_of, stack_root};
use crate::git;
use crate::style;

const SNAPSHOT_FILE: &str = "stk-undo";

// One snapshot per process: the outermost mutating command captures state;
// inner calls (sync's restack, merge's sync) must not overwrite it.
static TAKEN: AtomicBool = AtomicBool::new(false);

/// Record the current stack so `undo` can restore it. The `label` names the
/// operation being undone. No-ops after the first call in a process, and is
/// best effort: a snapshot failure never blocks the command itself.
pub fn take(label: &str) {
    if TAKEN.swap(true, Ordering::Relaxed) {
        return;
    }
    if let Err(error) = capture(label) {
        // The command should still run; we just lose undo for it.
        let _ = error;
    }
}

fn capture(label: &str) -> Result<()> {
    let head = git::current_branch()?;
    let root = stack_root(&head)?;

    let branches: Vec<Value> = branch_and_descendants(&root)?
        .into_iter()
        .map(|branch| {
            json!({
                "name": branch,
                "sha": git::branch_sha(&branch),
                "parent": parent_of(&branch).ok().flatten(),
                "base": base_of(&branch).ok().flatten(),
            })
        })
        .collect();

    let snapshot = json!({
        "label": label,
        "head": head,
        "branches": branches,
    });
    let path = git::git_path(SNAPSHOT_FILE)?;
    std::fs::write(&path, snapshot.to_string())
        .with_context(|| format!("failed to write {path}"))?;
    Ok(())
}

/// Restore the most recent snapshot: reset branch tips and metadata to their
/// pre-mutation state. Refuses on a dirty worktree (it resets the current
/// branch) and consumes the snapshot so it is one-shot.
pub fn undo() -> Result<()> {
    let path = git::git_path(SNAPSHOT_FILE)?;
    let Ok(contents) = std::fs::read_to_string(&path) else {
        anyhow::bail!("nothing to undo");
    };
    let snapshot: Value = serde_json::from_str(&contents).context("failed to parse undo state")?;

    if super::restack::in_progress() {
        anyhow::bail!(
            "a restack is in progress; finish with `git stk continue` or `git stk abort` first"
        );
    }
    if !git::worktree_is_clean()? {
        anyhow::bail!(
            "worktree has uncommitted changes; commit or stash them before `git stk undo`"
        );
    }

    let label = snapshot["label"].as_str().unwrap_or("the last operation");
    let head = snapshot["head"].as_str().unwrap_or_default().to_owned();
    let branches = snapshot["branches"].as_array().cloned().unwrap_or_default();

    let mut restored = 0;
    for entry in &branches {
        let name = entry["name"].as_str().unwrap_or_default();
        if name.is_empty() {
            continue;
        }

        // Refs first: recreate deleted branches, rewind moved ones.
        if let Some(sha) = entry["sha"].as_str() {
            git::update_ref(name, sha)?;
        }

        // Then metadata, set or cleared to match the snapshot.
        restore_config(name, "stkParent", entry["parent"].as_str())?;
        restore_config(name, "stkBase", entry["base"].as_str())?;
        restored += 1;
    }

    // Put HEAD back where it was and sync the worktree to the restored tip
    // (clean-tree precondition makes this lossless).
    if !head.is_empty() && git::branch_sha(&head).is_some() {
        if git::current_branch().ok().as_deref() != Some(&head) {
            git::checkout(&head)?;
        }
        git::reset_hard()?;
    }

    std::fs::remove_file(&path).ok();

    anstream::println!(
        "{}",
        style::success(&format!("undid {label}: restored {restored} branches"))
    );
    anstream::println!(
        "{}",
        style::dim("local refs and metadata only; pushes and merged reviews are not reverted")
    );
    Ok(())
}

fn restore_config(branch: &str, key: &str, value: Option<&str>) -> Result<()> {
    let full = format!("branch.{branch}.{key}");
    match value {
        Some(value) => git::config_set(&full, value),
        None => git::config_unset(&full),
    }
}
