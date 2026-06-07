use anyhow::Result;
use clap_complete::engine::ArgValueCompleter;

use crate::commands::Run;
use crate::completions;
use crate::providers::{detect_provider, review_provider};
use crate::{git, stack};

/// Rename a branch and retarget its stack children.
#[derive(Debug, clap::Args)]
pub struct Rename {
    /// New name for the current branch, or a branch and its new name.
    #[arg(
        required = true,
        num_args = 1..=2,
        value_name = "[BRANCH] NEW_NAME",
        add = ArgValueCompleter::new(completions::branch_candidates),
    )]
    names: Vec<String>,
}

impl Run for Rename {
    fn run(self) -> Result<()> {
        let (old, new) = match self.names.as_slice() {
            [new] => (git::current_branch()?, new.clone()),
            [old, new] => (old.clone(), new.clone()),
            _ => unreachable!("clap enforces one or two names"),
        };
        rename(&old, &new)
    }
}

fn rename(old: &str, new: &str) -> Result<()> {
    stack::rename_branch(old, new)?;

    // Best effort: an existing review still heads the old branch name, and
    // the platform does not follow local renames.
    if let Ok(provider) = detect_provider() {
        let review_provider = review_provider(provider.kind);
        if let Ok(Some(review)) = review_provider.review_for_branch(old)
            && review.branch == *old
        {
            println!(
                "warning: review {} still heads {old}; submitting {new} will open a new review",
                review.id
            );
        }
    }

    Ok(())
}
