use std::io;

use anyhow::Result;
use clap::CommandFactory;
use clap_complete::Shell;

use crate::cli::Cli;

/// Lets git's bash completion handle `git stk <TAB>` by delegating to the
/// clap-generated completer, which only knows the `git-stk` binary form.
///
/// The clap completer reads the command name, current word, and previous word
/// from its positional arguments (the way `complete -F` invokes it), so the
/// shim must pass them explicitly after rewriting the word list.
const BASH_GIT_SHIM: &str = r#"
_git_stk() {
    local cur prev
    COMP_WORDS=("git-stk" "${COMP_WORDS[@]:2}")
    COMP_CWORD=$((COMP_CWORD - 1))
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD - 1]}"
    _git-stk git-stk "$cur" "$prev"
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
