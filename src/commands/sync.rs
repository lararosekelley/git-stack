use anyhow::{Result, bail};
use clap::ArgAction;
use clap_complete::engine::ArgValueCompleter;

use crate::commands::Run;
use crate::completions;
use crate::providers::{detect_provider, review_provider};
use crate::{git, stack};

/// Sync local stack metadata from remote review requests.
#[derive(Debug, clap::Args)]
pub struct Sync {
    #[arg(add = ArgValueCompleter::new(completions::branch_candidates))]
    branch: Option<String>,
    /// Print what would change without updating local metadata.
    #[arg(long, action = ArgAction::SetTrue)]
    dry_run: bool,
}

impl Run for Sync {
    fn run(self) -> Result<()> {
        sync_stack(self.branch.as_deref(), self.dry_run)
    }
}

pub fn sync_stack(branch: Option<&str>, dry_run: bool) -> Result<()> {
    let branches = match branch {
        Some(branch) => vec![branch.to_owned()],
        None => git::local_branches()?,
    };

    let provider = detect_provider()?;
    let review_provider = review_provider(provider.kind);
    let mut synced = 0;
    let mut skipped = 0;

    for branch in branches {
        let Some(review) = review_provider.review_for_branch(&branch)? else {
            println!("skipped {branch}: no {} review found", provider.kind);
            skipped += 1;
            continue;
        };

        if review.branch != branch {
            println!(
                "skipped {branch}: {} review belongs to {}",
                provider.kind, review.branch
            );
            skipped += 1;
            continue;
        }

        if review.branch == review.base {
            bail!("refusing to set {branch} as its own stack parent");
        }

        if !dry_run {
            stack::set_parent_for_branch(&branch, &review.base)?;
            stack::record_base(&branch, &review.base);
        }
        println!(
            "{} {} -> {} ({})",
            if dry_run { "would sync" } else { "synced" },
            review.branch,
            review.base,
            review.id
        );
        synced += 1;
    }

    println!(
        "sync complete: {synced} {}synced, {skipped} skipped",
        if dry_run { "would be " } else { "" }
    );
    Ok(())
}
