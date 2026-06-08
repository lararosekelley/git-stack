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

    let cli = Cli::parse();
    git_stk::git::set_verbose(cli.verbose);

    // Common, human-facing commands get the once-a-day update nudge after
    // their work is done. Plumbing-ish output (completions, parent) and
    // upgrade itself stay clean.
    let hint_update = matches!(
        &cli.command,
        Command::List(_)
            | Command::Status(_)
            | Command::Sync(_)
            | Command::Submit(_)
            | Command::Merge(_)
            | Command::Restack(_)
    );

    let result = match cli.command {
        Command::New(command) => command.run(),
        Command::Parent(command) => command.run(),
        Command::Children(command) => command.run(),
        Command::Up(command) => command.run(),
        Command::Down(command) => command.run(),
        Command::Top(command) => command.run(),
        Command::Bottom(command) => command.run(),
        Command::List(command) => command.run(),
        Command::Status(command) => command.run(),
        Command::Adopt(command) => command.run(),
        Command::Detach(command) => command.run(),
        Command::Rename(command) => command.run(),
        Command::Restack(command) => command.run(),
        Command::Continue(command) => command.run(),
        Command::Abort(command) => command.run(),
        Command::Undo(command) => command.run(),
        Command::Provider(command) => command.run(),
        Command::Review(command) => command.run(),
        Command::View(command) => command.run(),
        Command::Sync(command) => command.run(),
        Command::Merge(command) => command.run(),
        Command::Repair(command) => command.run(),
        Command::Submit(command) => command.run(),
        Command::Config(command) => command.run(),
        Command::Completions(command) => command.run(),
        Command::Guide(command) => command.run(),
        Command::Setup(command) => command.run(),
        Command::Upgrade(command) => command.run(),
        Command::Cleanup(command) => command.run(),
    };

    if result.is_ok() && hint_update {
        git_stk::upgrade::maybe_hint_update();
    }
    result
}
