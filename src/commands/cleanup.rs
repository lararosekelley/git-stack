use anyhow::Result;
use clap::ArgAction;
use clap_complete::engine::ArgValueCompleter;

use crate::commands::Run;
use crate::completions;
use crate::providers::{ReviewProvider, ReviewState, detect_provider, review_provider};
use crate::{git, stack};

/// Clean up local metadata for merged review requests and delete their
/// branches.
#[derive(Debug, clap::Args)]
pub struct Cleanup {
    #[arg(add = ArgValueCompleter::new(completions::branch_candidates))]
    branch: Option<String>,
    /// Print what would change without updating local metadata.
    #[arg(long, action = ArgAction::SetTrue)]
    dry_run: bool,
    /// Keep cleaned merged branches instead of deleting them.
    #[arg(long, action = ArgAction::SetTrue)]
    keep_branch: bool,
}

impl Run for Cleanup {
    fn run(self) -> Result<()> {
        cleanup(self.branch.as_deref(), self.dry_run, self.keep_branch)
    }
}

pub fn cleanup(branch: Option<&str>, dry_run: bool, keep_branch: bool) -> Result<()> {
    let branch = branch
        .map(str::to_owned)
        .map_or_else(git::current_branch, Ok)?;
    let branches = stack::branch_and_descendants(&branch)?;
    let current_branch = git::current_branch()?;
    let provider = detect_provider()?;
    let review_provider = review_provider(provider.kind);
    let mut cleaned = 0;
    let mut skipped = 0;

    for branch in branches {
        // Closed-inclusive so a review closed without merging gets a
        // truthful skip instead of "no review found". Only merged reviews
        // are ever cleaned: a closed review's work is not in the trunk.
        let Some(review) = review_provider.review_for_branch_including_closed(&branch)? else {
            println!("skipped {branch}: no {} review found", provider.kind);
            skipped += 1;
            continue;
        };

        if review.state != ReviewState::Merged {
            println!("skipped {branch}: review {} is {}", review.id, review.state);
            skipped += 1;
            continue;
        }

        cleanup_merged_branch(review_provider.as_ref(), &branch, dry_run)?;
        cleanup_branch_deletion(&branch, &current_branch, dry_run, !keep_branch)?;
        cleaned += 1;
    }

    println!("cleanup complete: {cleaned} cleaned, {skipped} skipped");
    Ok(())
}

pub(crate) fn cleanup_merged_branch(
    review_provider: &dyn ReviewProvider,
    branch: &str,
    dry_run: bool,
) -> Result<()> {
    let parent = stack::parent_for_branch(branch)?;
    let descendants = stack::branch_and_descendants(branch)?;
    let direct_children: Vec<_> = descendants
        .into_iter()
        .skip(1)
        .filter_map(|child| match stack::parent_for_branch(&child) {
            Ok(Some(child_parent)) if child_parent == branch => Some(Ok(child)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<_>>()?;

    for child in direct_children {
        match parent.as_deref() {
            Some(parent) => {
                println!(
                    "{} retarget {child} -> {parent}",
                    if dry_run { "would" } else { "will" }
                );
                update_child_review_base(review_provider, &child, parent, dry_run)?;
                if !dry_run {
                    // Record the fork point off the merged branch before
                    // retargeting, so the next restack replays only the
                    // child's own commits even after a squash merge.
                    if let Ok(base) = git::merge_base(branch, &child) {
                        stack::set_base_for_branch(&child, &base)?;
                    }
                    stack::set_parent_for_branch(&child, parent)?;
                }
            }
            None => {
                println!("{} detach {child}", if dry_run { "would" } else { "will" });
                if !dry_run {
                    stack::unset_parent_for_branch(&child)?;
                    stack::unset_base_for_branch(&child)?;
                }
            }
        }
    }

    println!("{} detach {branch}", if dry_run { "would" } else { "will" });
    if !dry_run {
        stack::unset_parent_for_branch(branch)?;
        stack::unset_base_for_branch(branch)?;
    }

    Ok(())
}

pub(crate) fn cleanup_branch_deletion(
    branch: &str,
    current_branch: &str,
    dry_run: bool,
    delete_branch: bool,
) -> Result<()> {
    if !delete_branch {
        return Ok(());
    }

    // The checked out branch cannot be deleted; keep it and let the user
    // switch away instead of failing the rest of the cleanup.
    if branch == current_branch {
        println!("kept {branch}: cannot delete the checked out branch");
        return Ok(());
    }

    println!(
        "{} delete branch {branch}",
        if dry_run { "would" } else { "will" }
    );
    if !dry_run {
        git::delete_branch(branch)?;
    }

    Ok(())
}

fn update_child_review_base(
    review_provider: &dyn ReviewProvider,
    child: &str,
    parent: &str,
    dry_run: bool,
) -> Result<()> {
    let Some(review) = review_provider.review_for_branch(child)? else {
        return Ok(());
    };

    if review.state == ReviewState::Merged || review.base == parent {
        return Ok(());
    }

    println!(
        "{} update review {} -> {} ({})",
        if dry_run { "would" } else { "will" },
        review.branch,
        parent,
        review.id
    );
    if !dry_run {
        let output = review_provider.update_review_base(&review, parent)?;
        if !output.is_empty() {
            println!("{output}");
        }
    }

    Ok(())
}
