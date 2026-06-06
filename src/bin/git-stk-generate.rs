use std::{fs, path::PathBuf};

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;
use git_stk::cli::Cli;
use git_stk::completions;

#[derive(Debug, Parser)]
#[command(name = "git-stk-generate")]
#[command(about = "Generate git-stk shell completions and man pages")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Generate shell completions.
    Completions {
        /// Directory to write completion files into.
        out_dir: PathBuf,
        /// Shell to generate. Omit to generate all supported shells.
        #[arg(long)]
        shell: Option<CompletionShell>,
    },
    /// Generate man pages.
    Man {
        /// Directory to write man pages into.
        out_dir: PathBuf,
    },
    /// Generate shell completions and man pages.
    All {
        /// Directory to write generated assets into.
        out_dir: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CompletionShell {
    Bash,
    Elvish,
    Fish,
    PowerShell,
    Zsh,
}

impl From<CompletionShell> for Shell {
    fn from(shell: CompletionShell) -> Self {
        match shell {
            CompletionShell::Bash => Self::Bash,
            CompletionShell::Elvish => Self::Elvish,
            CompletionShell::Fish => Self::Fish,
            CompletionShell::PowerShell => Self::PowerShell,
            CompletionShell::Zsh => Self::Zsh,
        }
    }
}

fn main() -> Result<()> {
    match Args::parse().command {
        Command::Completions { out_dir, shell } => generate_completions(&out_dir, shell)?,
        Command::Man { out_dir } => generate_man_page(&out_dir)?,
        Command::All { out_dir } => {
            generate_completions(&out_dir.join("completions"), None)?;
            generate_man_page(&out_dir.join("man"))?;
        }
    }

    Ok(())
}

fn generate_completions(out_dir: &PathBuf, shell: Option<CompletionShell>) -> Result<()> {
    fs::create_dir_all(out_dir)?;
    let shells: Vec<_> = shell.map_or_else(
        || {
            vec![
                CompletionShell::Bash,
                CompletionShell::Elvish,
                CompletionShell::Fish,
                CompletionShell::PowerShell,
                CompletionShell::Zsh,
            ]
        },
        |shell| vec![shell],
    );

    for shell in shells {
        let shell = Shell::from(shell);
        let path = out_dir.join(format!("git-stk.{shell}"));
        let mut file = fs::File::create(&path)?;
        // Plain binary name: distributable files must rely on PATH rather
        // than a build-machine location.
        completions::write(shell, "git-stk", &mut file)?;
        println!("generated {}", path.display());
    }

    Ok(())
}

fn generate_man_page(out_dir: &PathBuf) -> Result<()> {
    fs::create_dir_all(out_dir)?;
    let path = out_dir.join("git-stk.1");
    let mut buffer = Vec::new();
    clap_mangen::Man::new(Cli::command()).render(&mut buffer)?;
    fs::write(&path, buffer)?;
    println!("generated {}", path.display());

    Ok(())
}
