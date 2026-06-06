use clap::{ArgAction, Parser, Subcommand};
use clap_complete::engine::ArgValueCompleter;

use crate::completions;

#[derive(Debug, Parser)]
#[command(name = "git-stk")]
#[command(version)]
#[command(about = "Git-native stacked branch workflow helper, with GitHub and GitLab integration")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create a new child branch from the current branch.
    New { branch: String },
    /// Print a branch's stack parent.
    Parent {
        #[arg(add = ArgValueCompleter::new(completions::branch_candidates))]
        branch: Option<String>,
    },
    /// Print a branch's stack children.
    Children {
        #[arg(add = ArgValueCompleter::new(completions::branch_candidates))]
        branch: Option<String>,
    },
    /// Check out the current branch's stack parent.
    Up,
    /// Check out a stack child of the current branch.
    Down {
        #[arg(add = ArgValueCompleter::new(completions::child_branch_candidates))]
        branch: Option<String>,
    },
    /// Print the current stack.
    List,
    /// Print local and remote stack status for a branch.
    Status {
        #[arg(add = ArgValueCompleter::new(completions::branch_candidates))]
        branch: Option<String>,
    },
    /// Attach an existing branch to a parent.
    Adopt {
        #[arg(add = ArgValueCompleter::new(completions::branch_candidates))]
        branch: String,
        #[arg(long, add = ArgValueCompleter::new(completions::branch_candidates))]
        parent: String,
    },
    /// Remove stack parent metadata from a branch.
    Detach {
        #[arg(add = ArgValueCompleter::new(completions::branch_candidates))]
        branch: Option<String>,
    },
    /// Rebase the current branch and descendants onto their stack parents.
    Restack {
        /// Pass --update-refs to git rebase.
        #[arg(long, action = ArgAction::SetTrue, conflicts_with = "no_update_refs")]
        update_refs: bool,
        /// Do not pass --update-refs to git rebase.
        #[arg(long, action = ArgAction::SetTrue)]
        no_update_refs: bool,
        /// Force-push (with lease) every rebased branch afterwards.
        #[arg(long, action = ArgAction::SetTrue, conflicts_with = "no_push")]
        push: bool,
        /// Do not push rebased branches, overriding stk.pushOnRestack.
        #[arg(long, action = ArgAction::SetTrue)]
        no_push: bool,
    },
    /// Continue an interrupted restack after resolving conflicts.
    Continue,
    /// Abort an interrupted restack.
    Abort,
    /// Print detected review provider.
    Provider,
    /// Print the review request for a branch.
    Review {
        #[arg(add = ArgValueCompleter::new(completions::branch_candidates))]
        branch: Option<String>,
    },
    /// Sync local stack metadata from remote review requests.
    Sync {
        #[arg(add = ArgValueCompleter::new(completions::branch_candidates))]
        branch: Option<String>,
        /// Print what would change without updating local metadata.
        #[arg(long, action = ArgAction::SetTrue)]
        dry_run: bool,
    },
    /// Create or update a remote review request for a branch.
    Submit {
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
    },
    /// Print all stk git config settings and branch metadata.
    Config,
    /// Print shell completions.
    Completions {
        /// Shell to print completions for.
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Install the man page and wire up shell completions.
    Setup {
        /// Skip confirmation prompts.
        #[arg(long, short = 'y', action = ArgAction::SetTrue)]
        yes: bool,
        /// Only re-render generated assets (man page); never touch shell rc files.
        #[arg(long, action = ArgAction::SetTrue, conflicts_with = "yes")]
        refresh: bool,
    },
    /// Upgrade git-stk to the latest release.
    Upgrade {
        /// Build and install the latest unreleased commit instead.
        #[arg(long, action = ArgAction::SetTrue, conflicts_with = "force")]
        head: bool,
        /// Reinstall the latest release even when already up to date.
        #[arg(long, action = ArgAction::SetTrue)]
        force: bool,
        /// Skip the --head confirmation prompt.
        #[arg(long, short = 'y', action = ArgAction::SetTrue, requires = "head")]
        yes: bool,
    },
    /// Clean up local metadata for merged review requests.
    Cleanup {
        #[arg(add = ArgValueCompleter::new(completions::branch_candidates))]
        branch: Option<String>,
        /// Print what would change without updating local metadata.
        #[arg(long, action = ArgAction::SetTrue)]
        dry_run: bool,
        /// Delete cleaned merged branches after updating stack metadata.
        #[arg(long, action = ArgAction::SetTrue)]
        delete_branch: bool,
    },
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum UpdateRefsMode {
    Config,
    Enabled,
    Disabled,
}

impl UpdateRefsMode {
    pub fn from_flags(update_refs: bool, no_update_refs: bool) -> Self {
        match (update_refs, no_update_refs) {
            (true, false) => Self::Enabled,
            (false, true) => Self::Disabled,
            _ => Self::Config,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PushMode {
    Config,
    Enabled,
    Disabled,
}

impl PushMode {
    pub fn from_flags(push: bool, no_push: bool) -> Self {
        match (push, no_push) {
            (true, false) => Self::Enabled,
            (false, true) => Self::Disabled,
            _ => Self::Config,
        }
    }
}
