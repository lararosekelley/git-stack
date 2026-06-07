use anyhow::Result;
use clap_complete::engine::ArgValueCompleter;

use crate::commands::Run;
use crate::completions;
use crate::providers::{ReviewState, detect_provider, review_provider};
use crate::{git, stack};

/// Print local and remote stack status for a branch.
#[derive(Debug, clap::Args)]
pub struct Status {
    #[arg(add = ArgValueCompleter::new(completions::branch_candidates))]
    branch: Option<String>,
}

impl Run for Status {
    fn run(self) -> Result<()> {
        print_status(self.branch.as_deref())
    }
}

pub fn print_status(branch: Option<&str>) -> Result<()> {
    let branch = branch
        .map(str::to_owned)
        .map_or_else(git::current_branch, Ok)?;
    let parent = stack::parent_for_branch(&branch)?;
    let children = stack::children_for_branch(&branch)?;

    println!("branch: {branch}");
    match parent.as_deref() {
        Some(parent) => println!("parent: {parent}"),
        None => println!("parent: none"),
    }
    if children.is_empty() {
        println!("children: none");
    } else {
        println!("children: {}", children.join(", "));
    }

    let provider = detect_provider()?;
    println!("provider: {} ({})", provider.kind, provider.source);
    let review_provider = review_provider(provider.kind);

    let review = review_provider.review_for_branch(&branch)?;
    match &review {
        Some(review) => {
            println!(
                "review: {} {} {} -> {}",
                review.id, review.state, review.branch, review.base
            );
            println!("url: {}", review.url);

            if let Some(parent) = parent.as_deref()
                && parent != review.base
            {
                println!(
                    "warning: review base is {}, local parent is {parent} - run `git stk submit`",
                    review.base
                );
            }
        }
        None => println!("review: none"),
    }

    // Teach the loop: the next command, derived from review states and
    // local drift. A sync covers the restack, so the nudges don't stack.
    let mut hints = Vec::new();
    if let Some(review) = &review
        && review.state == ReviewState::Merged
    {
        hints.push(format!(
            "review {} is merged - run `git stk sync`",
            review.id
        ));
    }
    if let Some(parent) = parent.as_deref() {
        if let Ok(Some(parent_review)) = review_provider.review_for_branch(parent)
            && parent_review.branch == parent
            && parent_review.state == ReviewState::Merged
        {
            hints.push(format!(
                "parent review {} is merged - run `git stk sync`",
                parent_review.id
            ));
        }

        if hints.is_empty()
            && let Some(hint) = stack::behind_parent_hint(&branch, parent)
        {
            hints.push(hint);
        }
    }
    for hint in hints {
        println!("hint: {hint}");
    }

    Ok(())
}
