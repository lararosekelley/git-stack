use anyhow::{Result, bail};
use clap::ArgAction;

use crate::cli::PushMode;
use crate::commands::Run;
use crate::commands::sync::sync;
use crate::prompt::confirm;
use crate::providers::{ReviewState, detect_provider, review_provider};
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
}

impl Run for Merge {
    fn run(self) -> Result<()> {
        merge(self.dry_run, self.yes)
    }
}

fn merge(dry_run: bool, yes: bool) -> Result<()> {
    let current = crate::git::current_branch()?;
    let root = stack::stack_root(&current)?;
    let trunk = stack::trunk_branch(&crate::git::local_branches()?);

    // The bottom of the stack: the first branch that is not the trunk.
    // (A rootless fragment's own root is its bottom.)
    let Some(bottom) = stack::branch_and_descendants(&root)?
        .into_iter()
        .find(|branch| Some(branch) != trunk.as_ref())
    else {
        bail!("no stacked branches to merge");
    };

    let provider = detect_provider()?;
    let review_provider = review_provider(provider.kind);

    let Some(review) = review_provider.review_for_branch(&bottom)? else {
        bail!(
            "no {} review found for {bottom}; submit the stack first",
            provider.kind
        );
    };
    if review.state != ReviewState::Open {
        bail!(
            "review {} for {bottom} is {}, not open",
            review.id,
            review.state
        );
    }

    let expected_base = stack::parent_for_branch(&bottom)?;
    if let Some(expected) = &expected_base
        && *expected != review.base
    {
        bail!(
            "review {} targets {}, but {bottom}'s stack parent is {expected}; \
             run `git stk submit` first",
            review.id,
            review.base
        );
    }

    let strategy = settings::merge_strategy()?;
    let label = if review.title.is_empty() {
        review.id.clone()
    } else {
        format!("{} ({})", review.title, review.id)
    };

    if dry_run {
        println!("would merge {label} into {} ({strategy})", review.base);
        println!("would sync afterwards");
        return Ok(());
    }

    if !yes
        && !confirm(&format!(
            "merge {label} into {} ({strategy})? [y/N] ",
            review.base
        ))?
    {
        println!("merge cancelled");
        return Ok(());
    }

    let output = review_provider.merge_review(&review, &strategy)?;
    if !output.is_empty() {
        println!("{output}");
    }
    println!("merged {label}");

    // Reconcile everything the merge changed: fetch, clean up, restack, push.
    sync(false, PushMode::Config)
}
