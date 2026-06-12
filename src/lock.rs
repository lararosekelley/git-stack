//! A coarse advisory lock so two git-stk processes never run state-mutating
//! commands at once. Git locks its own index and refs, but not git-stk's
//! multi-step orchestration (snapshot, rebases, metadata, provider calls), so
//! a concurrent run could clobber the undo snapshot or half-rewrite the stack.

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use crate::git;

const LOCK_FILE: &str = "stk-lock";

/// Held for the duration of a mutating command; removes the lock file on drop.
/// Outside a git repo it is a no-op, so the command surfaces its own error.
pub struct Lock {
    path: Option<PathBuf>,
}

impl Lock {
    /// Take the lock for `command`, or fail if another git-stk process holds
    /// it. Naming the command makes the contention message actionable.
    pub fn acquire(command: &str) -> Result<Self> {
        let Ok(path) = git::git_common_path(LOCK_FILE) else {
            // Not a git repo: nothing to guard, and the command will report
            // the real problem itself.
            return Ok(Self { path: None });
        };
        let path = PathBuf::from(path);

        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                // Best effort: the holder line only feeds the error message.
                let _ = writeln!(file, "{} {command}", std::process::id());
                Ok(Self { path: Some(path) })
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                let holder = fs::read_to_string(&path).unwrap_or_default();
                let holder = holder.trim();
                let by = if holder.is_empty() {
                    String::new()
                } else {
                    format!(" ({holder})")
                };
                bail!(
                    "another git stk operation is in progress{by}; wait for it to \
                     finish, or remove {} if it is stale",
                    path.display()
                );
            }
            Err(error) => {
                Err(error).with_context(|| format!("failed to take the lock at {}", path.display()))
            }
        }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        if let Some(path) = &self.path {
            let _ = fs::remove_file(path);
        }
    }
}
