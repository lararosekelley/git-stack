use anyhow::{Result, bail};
use clap::ArgAction;
use clap_complete::engine::ArgValueCompleter;

use crate::cli::PushMode;
use crate::commands::Run;
use crate::completions;
use crate::providers::{ReviewProvider, detect_provider, review_provider};
use crate::settings;
use crate::style;
use crate::{git, stack};

/// Create or update a remote review request for a branch.
#[derive(Debug, clap::Args)]
pub struct Submit {
    #[arg(add = ArgValueCompleter::new(completions::branch_candidates))]
    branch: Option<String>,
    /// Print what would change without creating or updating reviews.
    #[arg(long, action = ArgAction::SetTrue)]
    dry_run: bool,
    /// Submit the whole stack parent-first, from anywhere in it.
    #[arg(long, conflicts_with = "branch")]
    stack: bool,
    /// Submit only the current branch, overriding stk.submitStack.
    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "stack")]
    no_stack: bool,
    /// Push branches (-u --force-with-lease) before submitting.
    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "no_push")]
    push: bool,
    /// Do not push branches, overriding stk.pushOnSubmit.
    #[arg(long, action = ArgAction::SetTrue)]
    no_push: bool,
    /// Set a description block at the top of the review body; an empty
    /// string clears it. Applies to the current or named branch only.
    #[arg(long, short = 'd')]
    desc: Option<String>,
}

impl Run for Submit {
    fn run(self) -> Result<()> {
        // Stack mode: --stack forces it on; --no-stack or an explicit branch
        // forces it off; otherwise stk.submitStack decides.
        let submit_stack = if self.stack {
            true
        } else if self.no_stack || self.branch.is_some() {
            false
        } else {
            settings::bool_setting(settings::SUBMIT_STACK_KEY)?
        };

        submit(
            self.branch.as_deref(),
            submit_stack,
            self.dry_run,
            PushMode::from_flags(self.push, self.no_push),
            self.desc.as_deref(),
        )
    }
}

pub fn submit(
    branch: Option<&str>,
    submit_stack: bool,
    dry_run: bool,
    push_mode: crate::cli::PushMode,
    desc: Option<&str>,
) -> Result<()> {
    let branch = branch
        .map(str::to_owned)
        .map_or_else(git::current_branch, Ok)?;
    // The description targets this branch's review even in stack mode.
    let desc_branch = branch.clone();

    let branches = if submit_stack {
        // The whole stack containing the current branch, from anywhere in it:
        // walk to the root, then take its descendants. The root is excluded
        // only when it is the trunk (the base everything sits on); an
        // unanchored root stays in so validation can point at the missing
        // parent instead of silently skipping it.
        let root = stack::stack_root(&branch)?;
        let trunk = stack::trunk_branch(&git::local_branches()?);
        let full = stack::branch_and_descendants(&root)?;
        if Some(root) == trunk {
            full.into_iter().skip(1).collect()
        } else {
            full
        }
    } else {
        vec![branch]
    };

    let branch_parents = branch_parents(&branches)?;

    // Push after stack validation but before any provider calls: creating a
    // review requires the branch to exist remotely, and -u --force-with-lease
    // covers both first pushes and safely updating rebased branches.
    let push = settings::push_enabled(push_mode, settings::PUSH_ON_SUBMIT_KEY)?;
    if push {
        let remote = settings::remote()?;
        if dry_run {
            anstream::println!(
                "would push {} to {remote}",
                style::branch(&branches.join(" "))
            );
        } else {
            git::push_set_upstream_force_with_lease(&remote, &branches)?;
            anstream::println!("pushed {} to {remote}", style::branch(&branches.join(" ")));
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

    // After every review exists, write the description, link any issue the
    // branch name references, then (in stack mode) write the stack overview
    // into each body.
    if let Some(desc) = desc {
        crate::notes::update_description_note(
            review_provider.as_ref(),
            &desc_branch,
            desc,
            dry_run,
        )?;
    }
    crate::notes::update_closes_notes(review_provider.as_ref(), &branches, dry_run)?;
    if submit_stack {
        crate::notes::update_stack_notes(review_provider.as_ref(), &branch_parents, dry_run)?;
    }

    anstream::println!(
        "{}",
        style::success(&format!(
            "submit complete: {} created, {} updated, {} skipped",
            summary.created, summary.updated, summary.skipped
        ))
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
                anstream::println!(
                    "{}",
                    style::dim(&format!(
                        "{} already targets {} ({})",
                        review.branch, review.base, review.id
                    ))
                );
            }
            return Ok(SubmitAction::Skipped);
        }

        let output = if dry_run {
            String::new()
        } else {
            review_provider.update_review_base(&review, parent)?
        };
        anstream::println!(
            "{} {} -> {} {}",
            if dry_run { "would update" } else { "updated" },
            style::branch(&review.branch),
            style::branch(parent),
            style::dim(&format!("({})", review.id))
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
        anstream::println!(
            "{} {} -> {}",
            if dry_run { "would create" } else { "created" },
            style::branch(branch),
            style::branch(parent)
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
