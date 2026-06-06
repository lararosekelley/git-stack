use clap::{ArgAction, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "git-stack")]
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
    Parent { branch: Option<String> },
    /// Print a branch's stack children.
    Children { branch: Option<String> },
    /// Check out the current branch's stack parent.
    Up,
    /// Check out a stack child of the current branch.
    Down { branch: Option<String> },
    /// Print the current stack.
    List,
    /// Attach an existing branch to a parent.
    Adopt {
        branch: String,
        #[arg(long)]
        parent: String,
    },
    /// Remove stack parent metadata from a branch.
    Detach { branch: Option<String> },
    /// Rebase the current branch and descendants onto their stack parents.
    Restack {
        /// Pass --update-refs to git rebase.
        #[arg(long, action = ArgAction::SetTrue, conflicts_with = "no_update_refs")]
        update_refs: bool,
        /// Do not pass --update-refs to git rebase.
        #[arg(long, action = ArgAction::SetTrue)]
        no_update_refs: bool,
    },
    /// Continue an interrupted restack after resolving conflicts.
    Continue,
    /// Abort an interrupted restack.
    Abort,
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
