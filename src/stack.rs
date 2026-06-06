use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};

use anyhow::{Context, Result, bail};

use crate::cli::{PushMode, UpdateRefsMode};
use crate::git;

const PARENT_KEY: &str = "stkParent";
const BASE_KEY: &str = "stkBase";
const STATE_FILE: &str = "stack-state";
const PUSH_ON_RESTACK_KEY: &str = "stk.pushOnRestack";
const UPDATE_REFS_KEY: &str = "stk.updateRefs";
const REMOTE_KEY: &str = "stk.remote";
const DEFAULT_REMOTE: &str = "origin";

pub fn create_branch(branch: &str) -> Result<()> {
    let parent = git::current_branch()?;
    git::create_branch(branch)?;
    set_parent(branch, &parent)?;
    record_base(branch, &parent);
    println!("created {branch} with parent {parent}");
    Ok(())
}

pub fn print_parent(branch: Option<&str>) -> Result<()> {
    let branch = branch
        .map(str::to_owned)
        .map_or_else(git::current_branch, Ok)?;
    match parent_of(&branch)? {
        Some(parent) => println!("{parent}"),
        None => bail!("{branch} has no stack parent"),
    }
    Ok(())
}

pub fn print_children(branch: Option<&str>) -> Result<()> {
    let branch = branch
        .map(str::to_owned)
        .map_or_else(git::current_branch, Ok)?;
    for child in children_of(&branch)? {
        println!("{child}");
    }
    Ok(())
}

pub fn checkout_parent() -> Result<()> {
    let current = git::current_branch()?;
    let Some(parent) = parent_of(&current)? else {
        bail!("{current} has no stack parent");
    };

    git::checkout(&parent)
}

pub fn checkout_child(branch: Option<&str>) -> Result<()> {
    let current = git::current_branch()?;
    let children = children_of(&current)?;
    let child = match (branch, children.as_slice()) {
        (Some(branch), _) => {
            if children.iter().any(|child| child == branch) {
                branch.to_owned()
            } else {
                bail!("{branch} is not a stack child of {current}");
            }
        }
        (None, [child]) => child.to_owned(),
        (None, []) => bail!("{current} has no stack children"),
        (None, _) => {
            eprintln!("{current} has multiple stack children:");
            for child in children {
                eprintln!("  {child}");
            }
            bail!("choose one with `git stk up <branch>`");
        }
    };

    git::checkout(&child)
}

pub fn print_stack() -> Result<()> {
    let current = git::current_branch()?;
    let parents = parent_map()?;
    let root = root_for(&current, &parents);
    let children = children_map(&parents);
    let trunk = trunk_branch(&git::local_branches()?);

    let mut lines = Vec::new();
    collect_tree_lines(
        &root,
        &current,
        trunk.as_deref(),
        &children,
        0,
        &mut BTreeSet::new(),
        &mut lines,
    );

    // Leaf-first, trunk last: the stack reads like a pile sitting on its
    // base, matching the up/down direction of navigation.
    for line in lines.iter().rev() {
        println!("{line}");
    }
    Ok(())
}

/// The trunk branch: the remote's default branch when known locally,
/// otherwise a conventional name that exists.
pub fn trunk_branch(branches: &[String]) -> Option<String> {
    let remote = git::config_get(REMOTE_KEY)
        .ok()
        .flatten()
        .unwrap_or_else(|| DEFAULT_REMOTE.to_owned());
    if let Some(default) = git::remote_default_branch(&remote) {
        return Some(default);
    }

    ["main", "master"]
        .iter()
        .find(|name| branches.iter().any(|branch| branch == *name))
        .map(|name| (*name).to_owned())
}

pub fn adopt_branch(branch: &str, parent: &str) -> Result<()> {
    if branch == parent {
        bail!("a branch cannot be its own stack parent");
    }

    let branches: BTreeSet<_> = git::local_branches()?.into_iter().collect();
    if !branches.contains(branch) {
        bail!("branch {branch} does not exist");
    }
    if !branches.contains(parent) {
        bail!("parent branch {parent} does not exist");
    }

    set_parent(branch, parent)?;
    record_base(branch, parent);
    println!("attached {branch} to {parent}");
    Ok(())
}

pub fn detach_branch(branch: Option<&str>) -> Result<()> {
    let branch = branch
        .map(str::to_owned)
        .map_or_else(git::current_branch, Ok)?;
    unset_parent(&branch)?;
    unset_base(&branch)?;
    println!("detached {branch}");
    Ok(())
}

pub fn restack(update_refs_mode: UpdateRefsMode, push_mode: PushMode) -> Result<()> {
    let current = git::current_branch()?;
    let parents = parent_map()?;
    let branches = restack_order(&current, &parents);

    if branches.is_empty() {
        println!("nothing to restack");
        return Ok(());
    }

    let update_refs = resolve_update_refs(update_refs_mode)?;
    let push = resolve_push(push_mode)?;

    clear_state()?;
    let all = branches.clone();
    restack_branches(branches, &parents, update_refs, push, &all)
}

pub fn continue_restack() -> Result<()> {
    let Some(state) = RestackState::read()? else {
        bail!("no interrupted restack found");
    };

    if let Err(error) = git::rebase_continue() {
        eprintln!("restack still has conflicts");
        eprintln!("resolve conflicts, then run `git stk continue`");
        eprintln!("or run `git stk abort`");
        return Err(error);
    }

    record_base(&state.branch, &state.parent);

    if state.remaining.is_empty() {
        clear_state()?;
        finish_restack(&state.all, state.push)?;
        return Ok(());
    }

    let parents = parent_map()?;
    restack_branches(
        state.remaining,
        &parents,
        state.update_refs,
        state.push,
        &state.all,
    )
}

pub fn abort_restack() -> Result<()> {
    git::rebase_abort()?;
    clear_state()?;
    println!("restack aborted");
    Ok(())
}

pub fn parent_for_branch(branch: &str) -> Result<Option<String>> {
    parent_of(branch)
}

pub fn children_for_branch(branch: &str) -> Result<Vec<String>> {
    children_of(branch)
}

pub fn set_parent_for_branch(branch: &str, parent: &str) -> Result<()> {
    set_parent(branch, parent)
}

pub fn unset_parent_for_branch(branch: &str) -> Result<()> {
    unset_parent(branch)
}

pub fn base_for_branch(branch: &str) -> Result<Option<String>> {
    base_of(branch)
}

pub fn set_base_for_branch(branch: &str, base: &str) -> Result<()> {
    git::config_set(&base_key(branch), base)
}

pub fn unset_base_for_branch(branch: &str) -> Result<()> {
    unset_base(branch)
}

/// Record the fork point between a branch and its parent (best effort; e.g.
/// unrelated histories have no merge base, which is not an error here).
pub fn record_base(branch: &str, parent: &str) {
    if let Ok(base) = git::merge_base(parent, branch) {
        let _ = git::config_set(&base_key(branch), &base);
    }
}

/// The root of the stack containing `branch` (the base everything sits on).
pub fn stack_root(branch: &str) -> Result<String> {
    let parents = parent_map()?;
    Ok(root_for(branch, &parents))
}

pub fn branch_and_descendants(branch: &str) -> Result<Vec<String>> {
    let parents = parent_map()?;
    let children = children_map(&parents);
    let mut branches = vec![branch.to_owned()];
    collect_descendants(branch, &children, &mut branches);
    Ok(branches)
}

fn parent_map() -> Result<BTreeMap<String, String>> {
    let mut parents = BTreeMap::new();
    for branch in git::local_branches()? {
        if let Some(parent) = parent_of(&branch)? {
            parents.insert(branch, parent);
        }
    }
    Ok(parents)
}

fn restack_order(current: &str, parents: &BTreeMap<String, String>) -> Vec<String> {
    let children = children_map(parents);
    let mut branches = Vec::new();

    if parents.contains_key(current) {
        branches.push(current.to_owned());
    }

    collect_descendants(current, &children, &mut branches);
    branches
}

fn collect_descendants(
    branch: &str,
    children: &BTreeMap<String, Vec<String>>,
    branches: &mut Vec<String>,
) {
    if let Some(branch_children) = children.get(branch) {
        for child in branch_children {
            branches.push(child.to_owned());
            collect_descendants(child, children, branches);
        }
    }
}

fn restack_branches(
    branches: Vec<String>,
    parents: &BTreeMap<String, String>,
    update_refs: bool,
    push: bool,
    all: &[String],
) -> Result<()> {
    for (index, branch) in branches.iter().enumerate() {
        let Some(parent) = parents.get(branch) else {
            bail!("{branch} has no stack parent");
        };

        if update_refs {
            println!("rebasing {branch} onto {parent} with --update-refs");
        } else {
            println!("rebasing {branch} onto {parent}");
        }

        // Replay only the commits after the recorded fork point so commits
        // that landed upstream via squash or rebase merges are not repeated.
        // A base that is no longer an ancestor (stale or garbage) falls back
        // to a plain rebase.
        let base = match base_of(branch)? {
            Some(base) if git::is_ancestor(&base, branch).unwrap_or(false) => Some(base),
            _ => None,
        };
        let rebase_result = match &base {
            Some(base) => git::rebase_onto(parent, base, branch, update_refs),
            None => git::rebase(parent, branch, update_refs),
        };

        if let Err(error) = rebase_result {
            let remaining = branches[index + 1..].to_vec();
            RestackState {
                branch: branch.to_owned(),
                parent: parent.to_owned(),
                remaining,
                update_refs,
                push,
                all: all.to_vec(),
            }
            .write()?;

            eprintln!("conflict while rebasing {branch} onto {parent}");
            eprintln!("resolve conflicts, then run `git stk continue`");
            eprintln!("or run `git stk abort`");
            return Err(error);
        }

        record_base(branch, parent);
    }

    clear_state()?;
    finish_restack(all, push)
}

/// After every branch has been rebased: push the rewritten branches, or print
/// the exact command so stale remote PR diffs are a copy-paste away from fixed.
fn finish_restack(branches: &[String], push: bool) -> Result<()> {
    println!("restack complete");

    let remote = git::config_get(REMOTE_KEY)?.unwrap_or_else(|| DEFAULT_REMOTE.to_owned());
    if push {
        git::push_force_with_lease(&remote, branches)?;
        println!("pushed {} to {remote}", branches.join(" "));
    } else {
        println!("remote branches may be stale; push them with:");
        println!(
            "  git push --force-with-lease {remote} {}",
            branches.join(" ")
        );
    }
    Ok(())
}

fn resolve_push(mode: PushMode) -> Result<bool> {
    match mode {
        PushMode::Config => Ok(git::config_get_bool(PUSH_ON_RESTACK_KEY)?.unwrap_or(false)),
        PushMode::Enabled => Ok(true),
        PushMode::Disabled => Ok(false),
    }
}

fn resolve_update_refs(mode: UpdateRefsMode) -> Result<bool> {
    match mode {
        UpdateRefsMode::Config => {
            let configured = git::config_get_bool(UPDATE_REFS_KEY)?.unwrap_or(false);
            if configured && !git::supports_rebase_update_refs()? {
                eprintln!("stk.updateRefs is true, but this Git does not support --update-refs");
                return Ok(false);
            }
            Ok(configured)
        }
        UpdateRefsMode::Enabled => {
            if !git::supports_rebase_update_refs()? {
                bail!("--update-refs was requested, but this Git does not support it");
            }
            Ok(true)
        }
        UpdateRefsMode::Disabled => Ok(false),
    }
}

fn children_of(parent: &str) -> Result<Vec<String>> {
    Ok(parent_map()?
        .into_iter()
        .filter_map(|(branch, branch_parent)| (branch_parent == parent).then_some(branch))
        .collect())
}

fn children_map(parents: &BTreeMap<String, String>) -> BTreeMap<String, Vec<String>> {
    let mut children: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (branch, parent) in parents {
        children
            .entry(parent.to_owned())
            .or_default()
            .push(branch.to_owned());
    }
    children
}

fn root_for(branch: &str, parents: &BTreeMap<String, String>) -> String {
    let mut root = branch.to_owned();
    let mut seen = BTreeSet::new();

    while let Some(parent) = parents.get(&root) {
        if !seen.insert(root.clone()) {
            break;
        }
        root = parent.to_owned();
    }

    root
}

#[allow(clippy::too_many_arguments)]
fn collect_tree_lines(
    branch: &str,
    current: &str,
    trunk: Option<&str>,
    children: &BTreeMap<String, Vec<String>>,
    depth: usize,
    seen: &mut BTreeSet<String>,
    lines: &mut Vec<String>,
) {
    let mut line = format!("{}{}", "  ".repeat(depth), branch);
    if Some(branch) == trunk {
        line.push_str(" (trunk)");
    }
    if branch == current {
        line.push_str(" *");
    }
    lines.push(line);

    if !seen.insert(branch.to_owned()) {
        lines.push(format!("{}<cycle detected>", "  ".repeat(depth + 1)));
        return;
    }

    if let Some(branch_children) = children.get(branch) {
        for child in branch_children {
            collect_tree_lines(child, current, trunk, children, depth + 1, seen, lines);
        }
    }
}

fn parent_of(branch: &str) -> Result<Option<String>> {
    git::config_get(&parent_key(branch))
}

fn base_of(branch: &str) -> Result<Option<String>> {
    git::config_get(&base_key(branch))
}

fn set_parent(branch: &str, parent: &str) -> Result<()> {
    git::config_set(&parent_key(branch), parent)
}

fn unset_parent(branch: &str) -> Result<()> {
    git::config_unset(&parent_key(branch))
}

fn unset_base(branch: &str) -> Result<()> {
    git::config_unset(&base_key(branch))
}

fn parent_key(branch: &str) -> String {
    format!("branch.{branch}.{PARENT_KEY}")
}

fn base_key(branch: &str) -> String {
    format!("branch.{branch}.{BASE_KEY}")
}

#[derive(Debug, Eq, PartialEq)]
struct RestackState {
    branch: String,
    parent: String,
    remaining: Vec<String>,
    update_refs: bool,
    push: bool,
    /// Every branch in the interrupted restack, so the post-restack push (or
    /// push hint) can cover branches rebased before the conflict too.
    all: Vec<String>,
}

impl RestackState {
    fn read() -> Result<Option<Self>> {
        let path = state_path()?;
        if !path.exists() {
            return Ok(None);
        }

        let contents = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let mut branch = None;
        let mut parent = None;
        let mut remaining = Vec::new();
        let mut update_refs = false;
        let mut push = false;
        let mut all = Vec::new();

        for line in contents.lines() {
            if let Some(value) = line.strip_prefix("branch=") {
                branch = Some(value.to_owned());
            } else if let Some(value) = line.strip_prefix("parent=") {
                parent = Some(value.to_owned());
            } else if let Some(value) = line.strip_prefix("updateRefs=") {
                update_refs = value == "true";
            } else if let Some(value) = line.strip_prefix("push=") {
                push = value == "true";
            } else if let Some(value) = line.strip_prefix("remaining=") {
                remaining = value
                    .split('\t')
                    .filter(|branch| !branch.is_empty())
                    .map(str::to_owned)
                    .collect();
            } else if let Some(value) = line.strip_prefix("all=") {
                all = value
                    .split('\t')
                    .filter(|branch| !branch.is_empty())
                    .map(str::to_owned)
                    .collect();
            }
        }

        let Some(branch) = branch else {
            bail!("restack state is missing current branch");
        };
        let Some(parent) = parent else {
            bail!("restack state is missing parent branch");
        };

        Ok(Some(Self {
            branch,
            parent,
            remaining,
            update_refs,
            push,
            all,
        }))
    }

    fn write(&self) -> Result<()> {
        let path = state_path()?;
        let contents = format!(
            "branch={}\nparent={}\nupdateRefs={}\npush={}\nremaining={}\nall={}\n",
            self.branch,
            self.parent,
            self.update_refs,
            self.push,
            self.remaining.join("\t"),
            self.all.join("\t")
        );
        fs::write(&path, contents).with_context(|| format!("failed to write {}", path.display()))
    }
}

fn clear_state() -> Result<()> {
    let path = state_path()?;
    if path.exists() {
        fs::remove_file(&path).with_context(|| format!("failed to remove {}", path.display()))?;
    }
    Ok(())
}

fn state_path() -> Result<PathBuf> {
    Ok(PathBuf::from(git::git_path(STATE_FILE)?))
}
