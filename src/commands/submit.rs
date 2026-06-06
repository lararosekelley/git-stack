use anyhow::{Result, bail};
use clap::ArgAction;
use clap_complete::engine::ArgValueCompleter;

use crate::cli::PushMode;
use crate::commands::Run;
use crate::providers::{ReviewProvider, detect_provider, review_provider};
use crate::{git, stack};

// TODO(PR 4): centralize config keys.
const PUSH_ON_SUBMIT_KEY: &str = "stk.pushOnSubmit";
const REMOTE_KEY: &str = "stk.remote";
const DEFAULT_REMOTE: &str = "origin";
use crate::completions;

/// Create or update a remote review request for a branch.
#[derive(Debug, clap::Args)]
pub struct Submit {
    #[arg(add = ArgValueCompleter::new(completions::branch_candidates))]
    branch: Option<String>,
    /// Print what would change without creating or updating reviews.
    #[arg(long, action = ArgAction::SetTrue)]
    dry_run: bool,
    /// Submit the branch and its descendants parent-first.
    #[arg(long, conflicts_with = "branch")]
    stack: bool,
    /// Push branches (-u --force-with-lease) before submitting.
    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "no_push")]
    push: bool,
    /// Do not push branches, overriding stk.pushOnSubmit.
    #[arg(long, action = ArgAction::SetTrue)]
    no_push: bool,
}

impl Run for Submit {
    fn run(self) -> Result<()> {
        submit(
            self.branch.as_deref(),
            self.stack,
            self.dry_run,
            PushMode::from_flags(self.push, self.no_push),
        )
    }
}

pub fn submit(
    branch: Option<&str>,
    submit_stack: bool,
    dry_run: bool,
    push_mode: crate::cli::PushMode,
) -> Result<()> {
    let branch = branch
        .map(str::to_owned)
        .map_or_else(git::current_branch, Ok)?;

    let branches = if submit_stack {
        stack::branch_and_descendants(&branch)?
    } else {
        vec![branch]
    };

    let branch_parents = branch_parents(&branches)?;

    // Push after stack validation but before any provider calls: creating a
    // review requires the branch to exist remotely, and -u --force-with-lease
    // covers both first pushes and safely updating rebased branches.
    let push = match push_mode {
        crate::cli::PushMode::Config => git::config_get_bool(PUSH_ON_SUBMIT_KEY)?.unwrap_or(false),
        crate::cli::PushMode::Enabled => true,
        crate::cli::PushMode::Disabled => false,
    };
    if push {
        let remote = git::config_get(REMOTE_KEY)?.unwrap_or_else(|| DEFAULT_REMOTE.to_owned());
        if dry_run {
            println!("would push {} to {remote}", branches.join(" "));
        } else {
            git::push_set_upstream_force_with_lease(&remote, &branches)?;
            println!("pushed {} to {remote}", branches.join(" "));
        }
    }

    let provider = detect_provider()?;
    let review_provider = review_provider(provider.kind);
    let mut summary = SubmitSummary::default();

    for (branch, parent) in &branch_parents {
        summary.record(submit_branch(
            review_provider.as_ref(),
            branch,
            parent,
            dry_run,
        )?);
    }

    // After every review exists, write the stack overview into each body.
    if submit_stack {
        crate::notes::update_stack_notes(review_provider.as_ref(), &branch_parents, dry_run)?;
    }

    println!(
        "submit complete: {} created, {} updated, {} skipped",
        summary.created, summary.updated, summary.skipped
    );
    Ok(())
}

fn branch_parents(branches: &[String]) -> Result<Vec<(String, String)>> {
    let mut branch_parents = Vec::new();
    for branch in branches {
        let Some(parent) = stack::parent_for_branch(branch)? else {
            bail!("{branch} has no stack parent; run `git stk adopt` or `git stk sync` first");
        };
        branch_parents.push((branch.to_owned(), parent));
    }
    Ok(branch_parents)
}

fn submit_branch(
    review_provider: &dyn ReviewProvider,
    branch: &str,
    parent: &str,
    dry_run: bool,
) -> Result<SubmitAction> {
    if let Some(review) = review_provider.review_for_branch(branch)? {
        if review.base == parent {
            if dry_run {
                println!(
                    "would skip {} -> {} ({})",
                    review.branch, review.base, review.id
                );
            } else {
                println!(
                    "{} already targets {} ({})",
                    review.branch, review.base, review.id
                );
            }
            return Ok(SubmitAction::Skipped);
        }

        let output = if dry_run {
            String::new()
        } else {
            review_provider.update_review_base(&review, parent)?
        };
        println!(
            "{} {} -> {} ({})",
            if dry_run { "would update" } else { "updated" },
            review.branch,
            parent,
            review.id
        );
        if !output.is_empty() {
            println!("{output}");
        }
    } else {
        let output = if dry_run {
            String::new()
        } else {
            review_provider.create_review(branch, parent)?
        };
        println!(
            "{} {branch} -> {parent}",
            if dry_run { "would create" } else { "created" }
        );
        if !output.is_empty() {
            println!("{output}");
        }
        return Ok(SubmitAction::Created);
    }

    Ok(SubmitAction::Updated)
}

#[derive(Debug, Default)]
struct SubmitSummary {
    created: usize,
    updated: usize,
    skipped: usize,
}

impl SubmitSummary {
    fn record(&mut self, action: SubmitAction) {
        match action {
            SubmitAction::Created => self.created += 1,
            SubmitAction::Updated => self.updated += 1,
            SubmitAction::Skipped => self.skipped += 1,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum SubmitAction {
    Created,
    Updated,
    Skipped,
}
