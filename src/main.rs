use anyhow::Result;
use clap::{CommandFactory, Parser};
use git_stk::{cli, completions, config, providers, setup, stack, upgrade};

use git_stk::cli::{Cli, Command};

fn main() -> Result<()> {
    // Dynamic shell completion: when invoked by a shell's completer with
    // COMPLETE=<shell>, print candidates and exit instead of running a command.
    clap_complete::env::CompleteEnv::with_factory(Cli::command)
        .var(completions::COMPLETE_VAR)
        .complete();

    let cli = Cli::parse();

    match cli.command {
        Command::New { branch } => stack::create_branch(&branch),
        Command::Parent { branch } => stack::print_parent(branch.as_deref()),
        Command::Children { branch } => stack::print_children(branch.as_deref()),
        Command::Up { branch } => stack::checkout_child(branch.as_deref()),
        Command::Down => stack::checkout_parent(),
        Command::List { markdown } => {
            if markdown {
                providers::list_markdown()
            } else {
                stack::print_stack()
            }
        }
        Command::Status { branch } => providers::print_status(branch.as_deref()),
        Command::Adopt { branch, parent } => stack::adopt_branch(&branch, &parent),
        Command::Detach { branch } => stack::detach_branch(branch.as_deref()),
        Command::Restack {
            update_refs,
            no_update_refs,
            push,
            no_push,
        } => stack::restack(
            cli::UpdateRefsMode::from_flags(update_refs, no_update_refs),
            cli::PushMode::from_flags(push, no_push),
        ),
        Command::Continue => stack::continue_restack(),
        Command::Abort => stack::abort_restack(),
        Command::Provider => providers::print_provider(),
        Command::Review { branch } => providers::print_review(branch.as_deref()),
        Command::Sync { branch, dry_run } => providers::sync_stack(branch.as_deref(), dry_run),
        Command::Repair { dry_run } => providers::repair(dry_run),
        Command::Submit {
            branch,
            dry_run,
            stack,
            push,
            no_push,
        } => providers::submit(
            branch.as_deref(),
            stack,
            dry_run,
            cli::PushMode::from_flags(push, no_push),
        ),
        Command::Config => config::print_config(),
        Command::Completions { shell } => completions::print(shell),
        Command::Setup { yes, refresh } => setup::setup(yes, refresh),
        Command::Upgrade { head, force, yes } => upgrade::upgrade(head, force, yes),
        Command::Cleanup {
            branch,
            dry_run,
            delete_branch,
        } => providers::cleanup(branch.as_deref(), dry_run, delete_branch),
    }
}
