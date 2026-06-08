use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result, anyhow, bail};

static VERBOSE: AtomicBool = AtomicBool::new(false);

/// Pass raw git output through instead of capturing it.
pub fn set_verbose(verbose: bool) {
    VERBOSE.store(verbose, Ordering::Relaxed);
}

fn verbose() -> bool {
    VERBOSE.load(Ordering::Relaxed)
}

pub fn current_branch() -> Result<String> {
    output(&["symbolic-ref", "--quiet", "--short", "HEAD"])
        .context("failed to determine current branch")
}

pub fn local_branches() -> Result<Vec<String>> {
    let output = output(&["for-each-ref", "--format=%(refname:short)", "refs/heads"])?;
    Ok(output.lines().map(str::to_owned).collect())
}

pub fn git_path(path: &str) -> Result<String> {
    output(&["rev-parse", "--git-path", path])
}

pub fn remote_url(remote: &str) -> Result<Option<String>> {
    let output = Command::new("git")
        .args(["remote", "get-url", remote])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to read git remote {remote}"))?;

    match output.status.code() {
        Some(0) => Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        )),
        Some(2) => Ok(None),
        _ => Err(command_error("git remote get-url", &output.stderr)),
    }
}

pub fn checkout(branch: &str) -> Result<()> {
    status(&["switch", branch]).with_context(|| format!("failed to check out {branch}"))?;
    anstream::println!(
        "switched to {}",
        crate::style::paint(crate::style::BRANCH, branch)
    );
    Ok(())
}

pub fn create_branch(branch: &str) -> Result<()> {
    status(&["switch", "-c", branch]).with_context(|| format!("failed to create branch {branch}"))
}

/// Force-delete a branch. Callers are expected to have verified the branch
/// landed through review state: after a squash merge its commits are not
/// ancestry-merged, so `git branch -d` can refuse even though the work is in.
pub fn delete_branch(branch: &str) -> Result<()> {
    status(&["branch", "-D", branch]).with_context(|| format!("failed to delete branch {branch}"))
}

/// Rename a branch; git moves its `branch.<name>.*` config along with it.
pub fn rename_branch(old: &str, new: &str) -> Result<()> {
    status(&["branch", "-m", old, new]).with_context(|| format!("failed to rename {old} to {new}"))
}

/// Fast-forward a local branch from its remote without checking it out.
pub fn fetch_branch(remote: &str, branch: &str) -> Result<()> {
    let refspec = format!("{branch}:{branch}");
    status(&["fetch", remote, &refspec])
        .with_context(|| format!("failed to fetch {branch} from {remote}"))
}

pub fn pull_ff_only() -> Result<()> {
    status(&["pull", "--ff-only"]).context("failed to fast-forward from the remote")
}

pub fn push_force_with_lease(remote: &str, branches: &[String]) -> Result<()> {
    let mut args = vec!["push", "--force-with-lease", remote];
    args.extend(branches.iter().map(String::as_str));

    status(&args).with_context(|| format!("failed to push branches to {remote}"))
}

/// Push branches and set upstream tracking; used before submitting so new
/// branches exist remotely and rebased ones are safely updated.
pub fn push_set_upstream_force_with_lease(remote: &str, branches: &[String]) -> Result<()> {
    let mut args = vec!["push", "--set-upstream", "--force-with-lease", remote];
    args.extend(branches.iter().map(String::as_str));

    status(&args).with_context(|| format!("failed to push branches to {remote}"))
}

pub fn rebase(parent: &str, branch: &str, update_refs: bool) -> Result<()> {
    let mut args = vec!["rebase"];
    if update_refs {
        args.push("--update-refs");
    }
    args.extend([parent, branch]);

    status(&args).with_context(|| format!("failed to rebase {branch} onto {parent}"))
}

/// Rebase only the commits after `base`, replaying `base..branch` onto
/// `parent`. Used when the recorded fork point is known so commits that
/// landed upstream by squash or rebase are not replayed.
pub fn rebase_onto(parent: &str, base: &str, branch: &str, update_refs: bool) -> Result<()> {
    let mut args = vec!["rebase"];
    if update_refs {
        args.push("--update-refs");
    }
    args.extend(["--onto", parent, base, branch]);

    status(&args).with_context(|| format!("failed to rebase {branch} onto {parent} from {base}"))
}

pub fn rev_parse(rev: &str) -> Result<String> {
    let spec = format!("{rev}^{{commit}}");
    output(&["rev-parse", "--verify", &spec]).with_context(|| format!("failed to resolve {rev}"))
}

/// The commit a branch points at, or None when the branch does not exist.
pub fn branch_sha(branch: &str) -> Option<String> {
    rev_parse(branch).ok()
}

/// Point a branch at a commit, creating it if absent. Does not touch the
/// worktree.
pub fn update_ref(branch: &str, sha: &str) -> Result<()> {
    status(&["update-ref", &format!("refs/heads/{branch}"), sha])
        .with_context(|| format!("failed to update {branch} to {sha}"))
}

/// Reset the worktree and index to HEAD. Safe to lose nothing only on a
/// clean tree; callers must check [`worktree_is_clean`] first.
pub fn reset_hard() -> Result<()> {
    status(&["reset", "--hard"]).context("failed to reset the worktree")
}

/// Whether the worktree and index have no uncommitted changes.
pub fn worktree_is_clean() -> Result<bool> {
    Ok(output(&["status", "--porcelain"])?.is_empty())
}

/// Default branch of `remote` (from its locally-known HEAD symref), if any.
pub fn remote_default_branch(remote: &str) -> Option<String> {
    let reference = format!("refs/remotes/{remote}/HEAD");
    let full = output(&["symbolic-ref", "--short", &reference]).ok()?;
    full.strip_prefix(&format!("{remote}/")).map(str::to_owned)
}

/// How many commits `parent` has that `branch` does not: nonzero means the
/// branch needs a restack.
pub fn commits_behind(branch: &str, parent: &str) -> Result<usize> {
    let range = format!("{branch}..{parent}");
    let count = output(&["rev-list", "--count", &range])
        .with_context(|| format!("failed to count commits in {range}"))?;
    count
        .trim()
        .parse()
        .context("failed to parse rev-list count")
}

pub fn merge_base(a: &str, b: &str) -> Result<String> {
    output(&["merge-base", a, b])
        .with_context(|| format!("failed to find merge base of {a} and {b}"))
}

pub fn is_ancestor(ancestor: &str, descendant: &str) -> Result<bool> {
    let output = Command::new("git")
        .args(["merge-base", "--is-ancestor", ancestor, descendant])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("failed to run git merge-base --is-ancestor")?;

    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(command_error(
            "git merge-base --is-ancestor",
            &output.stderr,
        )),
    }
}

pub fn supports_rebase_update_refs() -> Result<bool> {
    let output = Command::new("git")
        .args(["rebase", "-h"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("failed to inspect git rebase help")?;

    let help = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(help_mentions_update_refs(&help))
}

/// Whether the short help advertises --update-refs. Match the option name:
/// git renders it as `--update-refs` or `--[no-]update-refs` by version.
fn help_mentions_update_refs(help: &str) -> bool {
    help.contains("update-refs")
}

pub fn rebase_continue() -> Result<()> {
    // Passthrough: continuing a rebase can open the user's editor.
    status_passthrough(&["rebase", "--continue"]).context("failed to continue rebase")
}

pub fn rebase_abort() -> Result<()> {
    status(&["rebase", "--abort"]).context("failed to abort rebase")
}

pub fn config_get(key: &str) -> Result<Option<String>> {
    let output = Command::new("git")
        .args(["config", "--get", key])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to read git config {key}"))?;

    match output.status.code() {
        Some(0) => Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        )),
        Some(1) => Ok(None),
        _ => Err(command_error("git config --get", &output.stderr)),
    }
}

pub fn config_get_bool(key: &str) -> Result<Option<bool>> {
    let output = Command::new("git")
        .args(["config", "--type=bool", "--get", key])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to read git config {key}"))?;

    match output.status.code() {
        Some(0) => {
            let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            match value.as_str() {
                "true" => Ok(Some(true)),
                "false" => Ok(Some(false)),
                _ => bail!("git config {key} is not a boolean: {value}"),
            }
        }
        Some(1) => Ok(None),
        _ => Err(command_error(
            "git config --type=bool --get",
            &output.stderr,
        )),
    }
}

pub fn config_get_regexp(pattern: &str) -> Result<Vec<(String, String)>> {
    let output = Command::new("git")
        .args(["config", "--get-regexp", pattern])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to read git config matching {pattern}"))?;

    match output.status.code() {
        Some(0) => Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| {
                line.split_once(' ')
                    .map(|(key, value)| (key.to_owned(), value.to_owned()))
            })
            .collect()),
        Some(1) => Ok(Vec::new()),
        _ => Err(command_error("git config --get-regexp", &output.stderr)),
    }
}

pub fn config_set(key: &str, value: &str) -> Result<()> {
    status(&["config", key, value]).with_context(|| format!("failed to set git config {key}"))
}

pub fn config_unset(key: &str) -> Result<()> {
    let output = Command::new("git")
        .args(["config", "--unset", key])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to unset git config {key}"))?;

    match output.status.code() {
        Some(0) | Some(5) => Ok(()),
        _ => Err(command_error("git config --unset", &output.stderr)),
    }
}

fn output(args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("failed to run git")?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        Err(command_error("git", &output.stderr))
    }
}

/// Run git quietly: progress and advice only matter when something goes
/// wrong, so capture them and replay on failure. `--verbose` passes
/// everything through.
fn status(args: &[&str]) -> Result<()> {
    if verbose() {
        return status_passthrough(args);
    }

    let output = Command::new("git")
        .args(args)
        .output()
        .context("failed to run git")?;

    if output.status.success() {
        Ok(())
    } else {
        let _ = std::io::stdout().write_all(&output.stdout);
        let _ = std::io::stderr().write_all(&output.stderr);
        bail!("git exited with status {}", output.status)
    }
}

/// Inherit stdio unconditionally, for git commands that may need the
/// terminal (e.g. `rebase --continue` opening the editor).
fn status_passthrough(args: &[&str]) -> Result<()> {
    let status = Command::new("git")
        .args(args)
        .status()
        .context("failed to run git")?;

    if status.success() {
        Ok(())
    } else {
        bail!("git exited with status {status}")
    }
}

fn command_error(command: &str, stderr: &[u8]) -> anyhow::Error {
    let stderr = String::from_utf8_lossy(stderr).trim().to_owned();
    if stderr.is_empty() {
        anyhow!("{command} failed")
    } else {
        anyhow!("{command} failed: {stderr}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_mentions_update_refs_matches_pre_2_43_spelling() {
        assert!(help_mentions_update_refs(
            "    --update-refs    update branches that point to commits that are being rebased"
        ));
    }

    #[test]
    fn help_mentions_update_refs_matches_negatable_spelling() {
        assert!(help_mentions_update_refs(
            "    --[no-]update-refs    update branches that point to commits that are being rebased"
        ));
    }

    #[test]
    fn help_mentions_update_refs_rejects_help_without_the_option() {
        assert!(!help_mentions_update_refs(
            "    --[no-]autosquash    move commits that begin with squash!/fixup!"
        ));
    }

    #[test]
    fn detection_agrees_with_the_real_git_on_this_machine() {
        // Ground truth: `--update-refs -h` fails with "unknown option" on a
        // git without the flag and prints help on one that has it.
        let probe = Command::new("git")
            .args(["rebase", "--update-refs", "-h"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .expect("run git rebase probe");
        let probe_text = format!(
            "{}{}",
            String::from_utf8_lossy(&probe.stdout),
            String::from_utf8_lossy(&probe.stderr)
        );
        let real_support = !probe_text.contains("unknown option");

        assert_eq!(
            supports_rebase_update_refs().expect("detect support"),
            real_support
        );
    }
}
