use std::io;

use anyhow::Result;
use clap::CommandFactory;
use clap_complete::Shell;

use crate::cli::Cli;

/// Lets git's bash completion handle `git stk <TAB>` by delegating to the
/// clap-generated completer, which only knows the `git-stk` binary form.
const BASH_GIT_SHIM: &str = r#"
_git_stk() {
    COMP_WORDS=("git-stk" "${COMP_WORDS[@]:2}")
    COMP_CWORD=$((COMP_CWORD - 1))
    _git-stk
}
"#;

pub fn print(shell: Shell) -> Result<()> {
    let mut command = Cli::command();
    clap_complete::generate(shell, &mut command, "git-stk", &mut io::stdout());

    if shell == Shell::Bash {
        print!("{BASH_GIT_SHIM}");
    }

    Ok(())
}
