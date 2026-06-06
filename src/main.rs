use anyhow::Result;
use clap::{CommandFactory, Parser};
use git_stk::cli::{Cli, Command};
use git_stk::commands::Run;
use git_stk::completions;

fn main() -> Result<()> {
    // Dynamic shell completion: when invoked by a shell's completer with
    // COMPLETE=<shell>, print candidates and exit instead of running a command.
    clap_complete::env::CompleteEnv::with_factory(Cli::command)
        .var(completions::COMPLETE_VAR)
        .complete();

    match Cli::parse().command {
        Command::New(command) => command.run(),
        Command::Parent(command) => command.run(),
        Command::Children(command) => command.run(),
        Command::Up(command) => command.run(),
        Command::Down(command) => command.run(),
        Command::List(command) => command.run(),
        Command::Status(command) => command.run(),
        Command::Adopt(command) => command.run(),
        Command::Detach(command) => command.run(),
        Command::Restack(command) => command.run(),
        Command::Continue(command) => command.run(),
        Command::Abort(command) => command.run(),
        Command::Provider(command) => command.run(),
        Command::Review(command) => command.run(),
        Command::Sync(command) => command.run(),
        Command::Merge(command) => command.run(),
        Command::Repair(command) => command.run(),
        Command::Submit(command) => command.run(),
        Command::Config(command) => command.run(),
        Command::Completions(command) => command.run(),
        Command::Setup(command) => command.run(),
        Command::Upgrade(command) => command.run(),
        Command::Cleanup(command) => command.run(),
    }
}
