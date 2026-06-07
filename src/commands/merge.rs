use anyhow::{Result, bail};
use clap::ArgAction;

use crate::cli::PushMode;
use crate::commands::Run;
use crate::commands::sync::sync;
use crate::prompt::confirm;
use crate::providers::{ProviderKind, ReviewProvider, ReviewRequest, ReviewState};
use crate::providers::{detect_provider, review_provider};
use crate::settings;
use crate::stack;

/// Merge the review at the bottom of the stack, then sync.
#[derive(Debug, clap::Args)]
pub struct Merge {
    /// Print what would happen without merging anything.
    #[arg(long, action = ArgAction::SetTrue)]
    dry_run: bool,
    /// Skip the confirmation prompt.
    #[arg(long, short = 'y', action = ArgAction::SetTrue)]
    yes: bool,
    /// Schedule the merge for when required checks pass instead of merging
    /// now.
    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "all")]
    auto: bool,
    /// Repeat merge-and-sync bottom-up until the whole stack has landed.
    #[arg(long, action = ArgAction::SetTrue)]
    all: bool,
}

impl Run for Merge {
    fn run(self) -> Result<()> {
        if self.all {
            merge_all(self.dry_run, self.yes)
        } else {
            merge(self.dry_run, self.yes, self.auto)
        }
    }
}

fn merge(dry_run: bool, yes: bool, auto: bool) -> Result<()> {
    let Some(bottom) = bottom_branch()? else {
        bail!("no stacked branches to merge");
    };

    let provider = detect_provider()?;
    let review_provider = review_provider(provider.kind);
    let review = open_review_for(review_provider.as_ref(), provider.kind, &bottom)?;

    let strategy = settings::merge_strategy()?;
    let mode = if auto {
        format!("{strategy}, auto")
    } else {
        strategy.clone()
    };
    let label = review_label(&review);

    if dry_run {
        println!("would merge {label} into {} ({mode})", review.base);
        println!("would sync afterwards");
        return Ok(());
    }

    if !yes
        && !confirm(&format!(
            "merge {label} into {} ({mode})? [y/N] ",
            review.base
        ))?
    {
        println!("merge cancelled");
        return Ok(());
    }

    match merge_and_check(review_provider.as_ref(), &review, &strategy, auto)? {
        // Reconcile everything the merge changed: fetch, clean up, restack,
        // push.
        MergeOutcome::Merged => sync(false, PushMode::Config),
        MergeOutcome::Scheduled => Ok(()),
    }
}

/// Land the whole stack: merge the bottom review and sync, bottom-up, until
/// the stack is complete. One confirmation up front; a merge that only gets
/// scheduled stops the loop.
fn merge_all(dry_run: bool, yes: bool) -> Result<()> {
    let Some(bottom) = bottom_branch()? else {
        bail!("no stacked branches to merge");
    };

    let provider = detect_provider()?;
    let review_provider = review_provider(provider.kind);
    let strategy = settings::merge_strategy()?;

    // What is about to land, bottom-up, for the dry run and the prompt.
    let current = crate::git::current_branch()?;
    let root = stack::stack_root(&current)?;
    let trunk = stack::trunk_branch(&crate::git::local_branches()?);
    let branches: Vec<String> = stack::branch_and_descendants(&root)?
        .into_iter()
        .filter(|branch| Some(branch) != trunk.as_ref())
        .collect();
    let count = branches.len();

    if dry_run {
        for branch in &branches {
            let review = open_review_for(review_provider.as_ref(), provider.kind, branch)?;
            println!(
                "would merge {} into {} ({strategy})",
                review_label(&review),
                review.base
            );
        }
        println!("would sync after each merge");
        return Ok(());
    }

    let base = stack::parent_for_branch(&bottom)?.unwrap_or_else(|| "its base".to_owned());
    if !yes
        && !confirm(&format!(
            "merge {count} review{} into {base}, bottom-up ({strategy})? [y/N] ",
            if count == 1 { "" } else { "s" }
        ))?
    {
        println!("merge cancelled");
        return Ok(());
    }

    // Each sync removes the merged bottom, so the loop is bounded by the
    // number of branches it started with.
    let mut landed = 0;
    for _ in 0..count {
        let Some(bottom) = bottom_branch()? else {
            break;
        };
        let review = open_review_for(review_provider.as_ref(), provider.kind, &bottom)?;
        match merge_and_check(review_provider.as_ref(), &review, &strategy, false)? {
            MergeOutcome::Merged => {
                sync(false, PushMode::Config)?;
                landed += 1;
            }
            MergeOutcome::Scheduled => break,
        }
    }

    println!(
        "merge complete: {landed} of {count} review{} merged",
        if count == 1 { "" } else { "s" }
    );
    Ok(())
}

/// The bottom of the stack containing the current branch: the first branch
/// that is not the trunk. (A rootless fragment's own root is its bottom.)
fn bottom_branch() -> Result<Option<String>> {
    let current = crate::git::current_branch()?;
    let root = stack::stack_root(&current)?;
    let trunk = stack::trunk_branch(&crate::git::local_branches()?);

    Ok(stack::branch_and_descendants(&root)?
        .into_iter()
        .find(|branch| Some(branch) != trunk.as_ref()))
}

/// The branch's review, validated as mergeable: it exists, is open, and
/// still targets the branch's stack parent.
fn open_review_for(
    review_provider: &dyn ReviewProvider,
    kind: ProviderKind,
    branch: &str,
) -> Result<ReviewRequest> {
    let Some(review) = review_provider.review_for_branch(branch)? else {
        bail!("no {kind} review found for {branch}; submit the stack first");
    };
    if review.state != ReviewState::Open {
        bail!(
            "review {} for {branch} is {}, not open",
            review.id,
            review.state
        );
    }

    let expected_base = stack::parent_for_branch(branch)?;
    if let Some(expected) = &expected_base
        && *expected != review.base
    {
        bail!(
            "review {} targets {}, but {branch}'s stack parent is {expected}; \
             run `git stk submit` first",
            review.id,
            review.base
        );
    }

    Ok(review)
}

fn review_label(review: &ReviewRequest) -> String {
    if review.title.is_empty() {
        review.id.clone()
    } else {
        format!("{} ({})", review.title, review.id)
    }
}

enum MergeOutcome {
    Merged,
    Scheduled,
}

/// Merge the review and report what actually happened: gh --auto and glab's
/// default auto-merge schedule the merge instead of performing it, and only
/// a review that reads merged afterwards should start a sync.
fn merge_and_check(
    review_provider: &dyn ReviewProvider,
    review: &ReviewRequest,
    strategy: &str,
    auto: bool,
) -> Result<MergeOutcome> {
    let label = review_label(review);

    let output = match review_provider.merge_review(review, strategy, auto) {
        Ok(output) => output,
        Err(error) => {
            // gh refuses outright when required checks are not green.
            let text = error.to_string().to_lowercase();
            if text.contains("status check") || text.contains("not mergeable") {
                eprintln!(
                    "hint: required checks may not be green yet - rerun `git stk merge` \
                     when they pass, or schedule with `git stk merge --auto`"
                );
            }
            return Err(error);
        }
    };
    if !output.is_empty() {
        println!("{output}");
    }

    match review_provider.review_for_branch(&review.branch)? {
        Some(after) if after.state == ReviewState::Merged => {
            println!("merged {label}");
            Ok(MergeOutcome::Merged)
        }
        _ => {
            println!("merge scheduled for {label}; rerun `git stk sync` once checks pass");
            Ok(MergeOutcome::Scheduled)
        }
    }
}
