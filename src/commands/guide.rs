use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

use anyhow::{Context, Result, bail};
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Confirm, Select};

use crate::commands::Run;
use crate::style;

type Tour = fn(&Path) -> Result<()>;

/// The available tours: (topic, menu description, runner).
const TOPICS: &[(&str, &str, Tour)] = &[
    ("intro", "create, submit, restack, and land a stack", intro),
    (
        "conflicts",
        "when a restack stops: resolve, continue, abort",
        conflicts,
    ),
    ("repair", "rebuild lost stack metadata", repair),
    (
        "absorb",
        "fold review fixes back into the commits they belong to",
        absorb,
    ),
];

/// Walk the stacked workflow in a disposable sandbox repository.
#[derive(Debug, clap::Args)]
pub struct Guide {
    /// Which tour to run; omit for a menu.
    #[arg(value_parser = clap::builder::PossibleValuesParser::new(["intro", "conflicts", "repair", "absorb"]))]
    topic: Option<String>,
}

impl Run for Guide {
    fn run(self) -> Result<()> {
        guide(self.topic.as_deref())
    }
}

fn guide(topic: Option<&str>) -> Result<()> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!("the guide is interactive; run it from a terminal");
    }

    banner("git stk guide");
    say("Short interactive tours. Everything happens in a disposable sandbox");
    say("repository - your real work is never touched, and a built-in demo");
    say("provider stands in for GitHub: same commands, no network.");
    println!();

    let chosen = match topic {
        Some(topic) => TOPICS
            .iter()
            .find(|(name, _, _)| *name == topic)
            .context("unknown guide topic")?,
        None => {
            let items: Vec<String> = TOPICS
                .iter()
                .map(|(name, blurb, _)| format!("{name} - {blurb}"))
                .collect();
            let index = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("which tour?")
                .items(&items)
                .default(0)
                .interact()
                .context("nothing chosen")?;
            &TOPICS[index]
        }
    };
    println!();

    let sandbox = env::temp_dir().join(format!("git-stk-guide-{}", std::process::id()));
    if sandbox.exists() {
        fs::remove_dir_all(&sandbox).context("failed to clear an old sandbox")?;
    }
    say(&format!("sandbox: {}", sandbox.display()));
    println!();
    setup_sandbox(&sandbox)?;

    let finished = (chosen.2)(&sandbox);
    println!();

    // Hand the sandbox over or clean it up, whether or not the tour ran dry.
    let delete = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("delete the sandbox?")
        .default(true)
        .interact()
        .unwrap_or(true);
    if delete {
        fs::remove_dir_all(&sandbox).context("failed to remove the sandbox")?;
        say("sandbox removed");
    } else {
        say(&format!("kept: cd {}", sandbox.display()));
        say("it uses `git config stk.provider demo`, so every command works offline");
    }

    finished
}

fn intro(sandbox: &Path) -> Result<()> {
    banner("1/5 - a stack is just branches");
    say("Each branch carries one reviewable change and knows its parent.");
    say("`new` creates a child of wherever you stand:");
    run_stk(sandbox, &["new", "feature/login"])?;
    commit(
        sandbox,
        "login.txt",
        "username + password form\n",
        "add login form",
    )?;
    run_stk(sandbox, &["new", "feature/avatar"])?;
    commit(sandbox, "avatar.txt", "round avatars\n", "add avatars")?;
    say("Two branches, stacked. `list` draws the pile, trunk at the bottom:");
    run_stk(sandbox, &["list"])?;
    if !proceed()? {
        return Ok(());
    }

    banner("2/5 - submit the whole stack");
    say("One command opens (or updates) a review per branch, parent-first,");
    say("and writes a live stack overview into every description:");
    run_stk(sandbox, &["submit", "--stack"])?;
    run_stk(sandbox, &["status"])?;
    if !proceed()? {
        return Ok(());
    }

    banner("3/5 - parents move; restack follows");
    say("Review feedback lands on the bottom branch:");
    run_stk(sandbox, &["down"])?;
    commit(
        sandbox,
        "login.txt",
        "username + password form\nremember me\n",
        "add remember me",
    )?;
    say("The child is now behind its parent - `list` notices:");
    run_stk(sandbox, &["list"])?;
    say("`restack` rebases every descendant back onto its parent:");
    run_stk(sandbox, &["restack"])?;
    run_stk(sandbox, &["top"])?;
    if !proceed()? {
        return Ok(());
    }

    banner("4/5 - land the stack");
    say("`merge --all` repeats merge-bottom-then-sync until the stack is");
    say("complete: children retarget, merged branches vanish, the overview");
    say("in every review restyles as history accumulates:");
    run_stk(sandbox, &["merge", "--all", "-y"])?;
    if !proceed()? {
        return Ok(());
    }

    banner("5/5 - nothing left but trunk");
    run_stk(sandbox, &["list"])?;
    say("That is the whole loop: new -> commit -> submit -> merge.");
    say("On a real repo the provider is detected from your remote; day to day");
    say("you mostly run `git stk new`, `git stk submit --stack`, and");
    say("`git stk merge --all`. `git stk status` and the hints fill the gaps.");
    Ok(())
}

fn conflicts(sandbox: &Path) -> Result<()> {
    banner("1/3 - set up a collision");
    say("A two-branch stack where both branches touch the same line:");
    run_stk(sandbox, &["new", "feature/payment"])?;
    commit(
        sandbox,
        "notes.txt",
        "use stripe\n",
        "choose payment provider",
    )?;
    run_stk(sandbox, &["new", "feature/receipts"])?;
    commit(
        sandbox,
        "notes.txt",
        "use stripe with receipts\n",
        "email receipts",
    )?;
    say("Now the parent changes its mind about that very line:");
    run_stk(sandbox, &["down"])?;
    commit(sandbox, "notes.txt", "use paypal\n", "switch to paypal")?;
    if !proceed()? {
        return Ok(());
    }

    banner("2/3 - the restack stops, with context");
    say("Replaying the child onto the rewritten parent cannot succeed; the");
    say("restack stops, shows git's conflict output, and says what to do:");
    run_stk_failing(sandbox, &["restack"])?;
    if !proceed()? {
        return Ok(());
    }

    banner("3/3 - resolve, then continue");
    say("Fix the file and stage it, exactly like any rebase conflict:");
    resolve(sandbox, "notes.txt", "use paypal with receipts\n")?;
    say("`continue` picks the restack back up where it stopped");
    say("(`git stk abort` would have unwound it instead):");
    run_stk(sandbox, &["continue"])?;
    run_stk(sandbox, &["list"])?;
    say("Conflicts interrupt the restack, never break it: resolve, continue,");
    say("and the rest of the stack follows.");
    Ok(())
}

fn repair(sandbox: &Path) -> Result<()> {
    banner("1/3 - a healthy stack");
    run_stk(sandbox, &["new", "feature/api"])?;
    commit(sandbox, "api.txt", "endpoints\n", "add api")?;
    run_stk(sandbox, &["new", "feature/ui"])?;
    commit(sandbox, "ui.txt", "buttons\n", "add ui")?;
    run_stk(sandbox, &["submit", "--stack"])?;
    if !proceed()? {
        return Ok(());
    }

    banner("2/3 - the metadata vanishes");
    say("Stack parents are plain `branch.<name>.stkParent` entries in");
    say(".git/config - annotations, not state. Suppose one gets lost:");
    shell_step("git config --unset branch.feature/ui.stkParent");
    git(
        sandbox,
        &["config", "--unset", "branch.feature/ui.stkParent"],
    )?;
    say("The stack no longer knows feature/ui belongs to it:");
    run_stk(sandbox, &["list"])?;
    if !proceed()? {
        return Ok(());
    }

    banner("3/3 - repair rebuilds it");
    say("`repair` re-derives parents from review bases (when a provider is");
    say("reachable) and branch ancestry, and verifies recorded fork points:");
    run_stk(sandbox, &["repair", "--dry-run"])?;
    run_stk(sandbox, &["repair"])?;
    run_stk(sandbox, &["list"])?;
    say("Branches are the real state; metadata is always recoverable.");
    say("Anything repair cannot resolve safely, it reports for a manual");
    say("`git stk adopt`.");
    Ok(())
}

fn absorb(sandbox: &Path) -> Result<()> {
    banner("1/3 - fixes scattered across the stack");
    say("A two-branch stack, each branch owning one file:");
    run_stk(sandbox, &["new", "feature/login"])?;
    commit(
        sandbox,
        "login.txt",
        "username + password form\n",
        "add login form",
    )?;
    run_stk(sandbox, &["new", "feature/avatar"])?;
    commit(sandbox, "avatar.txt", "round avatars\n", "add avatars")?;
    say("Review comes back: two small fixes, one on each branch's file.");
    say("You make both edits from the top and stage them, as usual:");
    stage_fix(sandbox, "login.txt", "username + password form, with 2FA\n")?;
    stage_fix(sandbox, "avatar.txt", "round avatars, lazy-loaded\n")?;
    say("Both fixes sit staged together, but each belongs to a different commit");
    say("further down the stack:");
    run_stk(sandbox, &["status"])?;
    if !proceed()? {
        return Ok(());
    }

    banner("2/3 - preview where each hunk lands");
    say("`absorb` blames every staged hunk and routes it to the commit that");
    say("introduced the lines it touches. `--dry-run` shows the plan first:");
    run_stk(sandbox, &["absorb", "--dry-run"])?;
    if !proceed()? {
        return Ok(());
    }

    banner("3/3 - fold them in");
    say("Run it for real: each fix becomes a `fixup!` of its owning commit, an");
    say("autosquash rebase folds them in, and every branch ref rides along:");
    run_stk(sandbox, &["absorb"])?;
    say("The history reads as if the fixes were always there - no extra commits:");
    shell_step("git log --oneline main..feature/avatar");
    git(
        sandbox,
        &["--no-pager", "log", "--oneline", "main..feature/avatar"],
    )?;
    println!();
    say("Hunks that cannot be attributed - brand-new lines, trunk-owned lines, a");
    say("hunk spanning two commits - are left staged and reported, never guessed.");
    Ok(())
}

fn setup_sandbox(sandbox: &Path) -> Result<()> {
    fs::create_dir_all(sandbox).context("failed to create the sandbox")?;
    git(sandbox, &["init", "-q", "-b", "main"])?;
    git(sandbox, &["config", "user.email", "guide@git-stk.dev"])?;
    git(sandbox, &["config", "user.name", "git-stk guide"])?;
    git(sandbox, &["config", "stk.provider", "demo"])?;
    git(sandbox, &["config", "stk.noUpdateCheck", "true"])?;
    fs::write(sandbox.join("README.md"), "# guide sandbox\n").context("failed to seed sandbox")?;
    git(sandbox, &["add", "README.md"])?;
    git(sandbox, &["commit", "-q", "-m", "initial commit"])?;
    Ok(())
}

/// Run the tool itself inside the sandbox, narrating the invocation. The
/// child inherits the terminal so its colors come through.
fn run_stk(sandbox: &Path, args: &[&str]) -> Result<()> {
    anstream::println!(
        "{} {}",
        style::paint(style::DIM, "$ git stk"),
        args.join(" ")
    );
    let binary = env::current_exe().context("failed to locate the running binary")?;
    let status = isolated(Command::new(binary).args(args).current_dir(sandbox))
        .status()
        .context("failed to run git-stk in the sandbox")?;
    if !status.success() {
        bail!("`git stk {}` failed in the sandbox", args.join(" "));
    }
    println!();
    Ok(())
}

/// Like [`run_stk`], for the step that is supposed to stop (the conflict).
fn run_stk_failing(sandbox: &Path, args: &[&str]) -> Result<()> {
    anstream::println!(
        "{} {}",
        style::paint(style::DIM, "$ git stk"),
        args.join(" ")
    );
    let binary = env::current_exe().context("failed to locate the running binary")?;
    let status = isolated(Command::new(binary).args(args).current_dir(sandbox))
        .status()
        .context("failed to run git-stk in the sandbox")?;
    if status.success() {
        bail!(
            "`git stk {}` was expected to stop on the conflict",
            args.join(" ")
        );
    }
    println!();
    Ok(())
}

/// Resolve a conflicted file: write the merged contents and stage them.
fn resolve(sandbox: &Path, file: &str, contents: &str) -> Result<()> {
    shell_step(&format!("edit {file}, then git add {file}"));
    fs::write(sandbox.join(file), contents).context("failed to write sandbox file")?;
    git(sandbox, &["add", file])
}

/// Edit a tracked file and stage the change, without committing - a review
/// fix waiting to be absorbed.
fn stage_fix(sandbox: &Path, file: &str, contents: &str) -> Result<()> {
    shell_step(&format!("edit {file}, then git add {file}"));
    fs::write(sandbox.join(file), contents).context("failed to write sandbox file")?;
    git(sandbox, &["add", file])
}

fn shell_step(narration: &str) {
    anstream::println!("{} {narration}", style::paint(style::DIM, "$"));
}

fn commit(sandbox: &Path, file: &str, contents: &str, message: &str) -> Result<()> {
    anstream::println!(
        "{} edit {file}, then git commit -m {message:?}",
        style::paint(style::DIM, "$"),
    );
    fs::write(sandbox.join(file), contents).context("failed to write sandbox file")?;
    git(sandbox, &["add", file])?;
    git(sandbox, &["commit", "-q", "-m", message])?;
    Ok(())
}

fn git(sandbox: &Path, args: &[&str]) -> Result<()> {
    let status = isolated(Command::new("git").args(args).current_dir(sandbox))
        .status()
        .context("failed to run git in the sandbox")?;
    if !status.success() {
        bail!("`git {}` failed in the sandbox", args.join(" "));
    }
    Ok(())
}

/// The user's global git config (e.g. stk.pushOnSubmit) must not leak into
/// the tour.
fn isolated(command: &mut Command) -> &mut Command {
    command
        .env("GIT_CONFIG_GLOBAL", nul_device())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_EDITOR", "true")
}

fn nul_device() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from("NUL")
    } else {
        PathBuf::from("/dev/null")
    }
}

fn proceed() -> Result<bool> {
    println!();
    Ok(Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("continue?")
        .default(true)
        .interact()
        .unwrap_or(false))
}

fn banner(title: &str) {
    anstream::println!("{}", style::paint(style::CURRENT, title));
}

fn say(line: &str) {
    anstream::println!("{}", style::paint(style::DIM, line));
}
