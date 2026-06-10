//! Stack metadata: the `branch.<name>.stkParent`/`stkBase` annotations and
//! the structural queries built on them. Navigation lives in [`nav`], the
//! rebase engine in [`restack`].

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};

use crate::git;
use crate::settings;
use crate::style;

mod nav;
mod restack;
mod snapshot;

pub use nav::{
    behind_parent_hint, checkout_bottom, checkout_child, checkout_parent, checkout_top,
    print_all_stacks, print_children, print_parent, print_stack,
};
pub use restack::{abort_restack, continue_restack, restack};
pub use snapshot::{take as snapshot, undo};

const PARENT_KEY: &str = "stkParent";
const BASE_KEY: &str = "stkBase";

pub fn create_branch(branch: &str) -> Result<()> {
    let parent = git::current_branch()?;
    // `new` creates the branch; an existing one is an adopt, not a create.
    if git::local_branches()?
        .iter()
        .any(|existing| existing == branch)
    {
        bail!(
            "branch {branch} already exists - adopt it onto {parent} \
             with `git stk adopt {branch} --parent {parent}`"
        );
    }
    git::create_branch(branch)?;
    set_parent(branch, &parent)?;
    record_base(branch, &parent);
    anstream::println!(
        "created {} with parent {}",
        style::branch(branch),
        style::branch(&parent)
    );
    Ok(())
}

/// The trunk branch: the remote's default branch when known locally,
/// otherwise a conventional name that exists.
pub fn trunk_branch(branches: &[String]) -> Option<String> {
    let remote = settings::remote().unwrap_or_else(|_| settings::DEFAULT_REMOTE.to_owned());
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
    anstream::println!(
        "attached {} to {}",
        style::branch(branch),
        style::branch(parent)
    );
    Ok(())
}

pub fn detach_branch(branch: Option<&str>) -> Result<()> {
    let branch = branch
        .map(str::to_owned)
        .map_or_else(git::current_branch, Ok)?;
    unset_parent(&branch)?;
    unset_base(&branch)?;
    anstream::println!("detached {}", style::branch(&branch));
    Ok(())
}

/// Rename a branch and keep the stack intact. Git moves the branch's own
/// metadata with the rename; children pointing at the old name are
/// retargeted here.
pub fn rename_branch(old: &str, new: &str, dry_run: bool) -> Result<()> {
    let children = children_for_branch(old)?;

    if !dry_run {
        snapshot::take("rename");
        git::rename_branch(old, new)?;
    }
    anstream::println!(
        "{} {} -> {}",
        if dry_run { "would rename" } else { "renamed" },
        style::branch(old),
        style::branch(new)
    );

    for child in &children {
        if !dry_run {
            set_parent_for_branch(child, new)?;
        }
        anstream::println!(
            "{} {} -> {}",
            if dry_run {
                "would retarget"
            } else {
                "retargeted"
            },
            style::branch(child),
            style::branch(new)
        );
    }
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

/// The stack path from the bottom up to (and including) `branch`,
/// parent-first; descendants above it are left out.
pub fn path_from_root(branch: &str) -> Result<Vec<String>> {
    let trunk = trunk_branch(&git::local_branches()?);
    let mut path = vec![branch.to_owned()];
    let mut seen = BTreeSet::from([branch.to_owned()]);

    let mut cursor = branch.to_owned();
    while let Some(parent) = parent_of(&cursor)? {
        if Some(&parent) == trunk.as_ref() || !seen.insert(parent.clone()) {
            break;
        }
        path.push(parent.clone());
        cursor = parent;
    }

    path.reverse();
    Ok(path)
}

/// (branch, parent) pairs for the branches that have a stack parent;
/// branches without one are skipped.
pub fn branch_parents(branches: &[String]) -> Result<Vec<(String, String)>> {
    let mut pairs = Vec::new();
    for branch in branches {
        if let Some(parent) = parent_of(branch)? {
            pairs.push((branch.clone(), parent));
        }
    }
    Ok(pairs)
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
