use anyhow::Result;
use clap::Parser;
use git_stack::{cli, providers, stack};

use git_stack::cli::{Cli, Command};

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::New { branch } => stack::create_branch(&branch),
        Command::Parent { branch } => stack::print_parent(branch.as_deref()),
        Command::Children { branch } => stack::print_children(branch.as_deref()),
        Command::Up => stack::checkout_parent(),
        Command::Down { branch } => stack::checkout_child(branch.as_deref()),
        Command::List => stack::print_stack(),
        Command::Status { branch } => providers::print_status(branch.as_deref()),
        Command::Adopt { branch, parent } => stack::adopt_branch(&branch, &parent),
        Command::Detach { branch } => stack::detach_branch(branch.as_deref()),
        Command::Restack {
            update_refs,
            no_update_refs,
        } => stack::restack(cli::UpdateRefsMode::from_flags(update_refs, no_update_refs)),
        Command::Continue => stack::continue_restack(),
        Command::Abort => stack::abort_restack(),
        Command::Provider => providers::print_provider(),
        Command::Review { branch } => providers::print_review(branch.as_deref()),
        Command::Sync { branch, dry_run } => providers::sync_stack(branch.as_deref(), dry_run),
        Command::Submit {
            branch,
            dry_run,
            stack,
        } => providers::submit(branch.as_deref(), stack, dry_run),
        Command::Cleanup {
            branch,
            dry_run,
            delete_branch,
        } => providers::cleanup(branch.as_deref(), dry_run, delete_branch),
    }
}
