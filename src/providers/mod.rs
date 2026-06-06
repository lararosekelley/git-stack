use std::{fmt, process::Command};

use anyhow::{Context, Result, anyhow, bail};

use crate::{git, stack};

mod github;
mod gitlab;
mod json;

use github::GitHubProvider;
use gitlab::GitLabProvider;

const PROVIDER_KEY: &str = "stk.provider";
const REMOTE_KEY: &str = "stk.remote";
const PUSH_ON_SUBMIT_KEY: &str = "stk.pushOnSubmit";
const DEFAULT_REMOTE: &str = "origin";

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ProviderKind {
    GitHub,
    GitLab,
}

impl ProviderKind {
    fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "github" | "gh" => Some(Self::GitHub),
            "gitlab" | "glab" => Some(Self::GitLab),
            _ => None,
        }
    }
}

impl fmt::Display for ProviderKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GitHub => write!(formatter, "github"),
            Self::GitLab => write!(formatter, "gitlab"),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct DetectedProvider {
    pub kind: ProviderKind,
    pub source: ProviderSource,
}

#[derive(Debug, Eq, PartialEq)]
pub enum ProviderSource {
    Config,
    Remote { remote: String, url: String },
}

impl fmt::Display for ProviderSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config => write!(formatter, "config"),
            Self::Remote { remote, url } => write!(formatter, "remote {remote} ({url})"),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum ReviewState {
    Open,
    Merged,
    Closed,
    Unknown(String),
}

#[derive(Debug, Eq, PartialEq)]
pub struct ReviewRequest {
    pub id: String,
    pub branch: String,
    pub base: String,
    pub state: ReviewState,
    pub url: String,
    pub title: String,
}

pub trait ReviewProvider {
    fn review_for_branch(&self, branch: &str) -> Result<Option<ReviewRequest>>;

    fn create_review(&self, branch: &str, base: &str) -> Result<String>;

    fn update_review_base(&self, review: &ReviewRequest, base: &str) -> Result<String>;

    fn review_body(&self, review: &ReviewRequest) -> Result<String>;

    fn update_review_body(&self, review: &ReviewRequest, body: &str) -> Result<String>;
}

pub fn print_provider() -> Result<()> {
    let provider = detect_provider()?;
    println!("{} ({})", provider.kind, provider.source);
    Ok(())
}

pub fn print_review(branch: Option<&str>) -> Result<()> {
    let branch = branch
        .map(str::to_owned)
        .map_or_else(git::current_branch, Ok)?;
    let provider = detect_provider()?;
    let review_provider = review_provider(provider.kind);

    let Some(review) = review_provider.review_for_branch(&branch)? else {
        bail!("no {} review found for {branch}", provider.kind);
    };

    println!(
        "{} {} -> {} {} {}",
        review.id, review.branch, review.base, review.state, review.url
    );
    Ok(())
}

pub fn print_status(branch: Option<&str>) -> Result<()> {
    let branch = branch
        .map(str::to_owned)
        .map_or_else(git::current_branch, Ok)?;
    let parent = stack::parent_for_branch(&branch)?;
    let children = stack::children_for_branch(&branch)?;

    println!("branch: {branch}");
    match parent.as_deref() {
        Some(parent) => println!("parent: {parent}"),
        None => println!("parent: none"),
    }
    if children.is_empty() {
        println!("children: none");
    } else {
        println!("children: {}", children.join(", "));
    }

    let provider = detect_provider()?;
    println!("provider: {} ({})", provider.kind, provider.source);
    let review_provider = review_provider(provider.kind);

    let Some(review) = review_provider.review_for_branch(&branch)? else {
        println!("review: none");
        return Ok(());
    };

    println!(
        "review: {} {} {} -> {}",
        review.id, review.state, review.branch, review.base
    );
    println!("url: {}", review.url);

    if let Some(parent) = parent
        && parent != review.base
    {
        println!(
            "warning: review base is {}, local parent is {}",
            review.base, parent
        );
    }

    Ok(())
}

pub fn sync_stack(branch: Option<&str>, dry_run: bool) -> Result<()> {
    let branches = match branch {
        Some(branch) => vec![branch.to_owned()],
        None => git::local_branches()?,
    };

    let provider = detect_provider()?;
    let review_provider = review_provider(provider.kind);
    let mut synced = 0;
    let mut skipped = 0;

    for branch in branches {
        let Some(review) = review_provider.review_for_branch(&branch)? else {
            println!("skipped {branch}: no {} review found", provider.kind);
            skipped += 1;
            continue;
        };

        if review.branch != branch {
            println!(
                "skipped {branch}: {} review belongs to {}",
                provider.kind, review.branch
            );
            skipped += 1;
            continue;
        }

        if review.branch == review.base {
            bail!("refusing to set {branch} as its own stack parent");
        }

        if !dry_run {
            git::config_set(&parent_key(&branch), &review.base)?;
            stack::record_base(&branch, &review.base);
        }
        println!(
            "{} {} -> {} ({})",
            if dry_run { "would sync" } else { "synced" },
            review.branch,
            review.base,
            review.id
        );
        synced += 1;
    }

    println!(
        "sync complete: {synced} {}synced, {skipped} skipped",
        if dry_run { "would be " } else { "" }
    );
    Ok(())
}

pub fn submit(
    branch: Option<&str>,
    submit_stack: bool,
    dry_run: bool,
    push_mode: crate::cli::PushMode,
) -> Result<()> {
    let branch = branch
        .map(str::to_owned)
        .map_or_else(git::current_branch, Ok)?;

    let branches = if submit_stack {
        stack::branch_and_descendants(&branch)?
    } else {
        vec![branch]
    };

    let branch_parents = branch_parents(&branches)?;

    // Push after stack validation but before any provider calls: creating a
    // review requires the branch to exist remotely, and -u --force-with-lease
    // covers both first pushes and safely updating rebased branches.
    let push = match push_mode {
        crate::cli::PushMode::Config => git::config_get_bool(PUSH_ON_SUBMIT_KEY)?.unwrap_or(false),
        crate::cli::PushMode::Enabled => true,
        crate::cli::PushMode::Disabled => false,
    };
    if push {
        let remote = git::config_get(REMOTE_KEY)?.unwrap_or_else(|| DEFAULT_REMOTE.to_owned());
        if dry_run {
            println!("would push {} to {remote}", branches.join(" "));
        } else {
            git::push_set_upstream_force_with_lease(&remote, &branches)?;
            println!("pushed {} to {remote}", branches.join(" "));
        }
    }

    let provider = detect_provider()?;
    let review_provider = review_provider(provider.kind);
    let mut summary = SubmitSummary::default();

    for (branch, parent) in &branch_parents {
        summary.record(submit_branch(
            review_provider.as_ref(),
            branch,
            parent,
            dry_run,
        )?);
    }

    // After every review exists, write the stack overview into each body.
    if submit_stack {
        update_stack_notes(review_provider.as_ref(), &branch_parents, dry_run)?;
    }

    println!(
        "submit complete: {} created, {} updated, {} skipped",
        summary.created, summary.updated, summary.skipped
    );
    Ok(())
}

const STACK_NOTE_START: &str = "<!-- git-stk:stack -->";
const STACK_NOTE_END: &str = "<!-- /git-stk:stack -->";
const TOOL_URL: &str = "https://github.com/lararosekelley/git-stk";

/// Maintain a stack overview in every review body: the full PR list
/// leaf-first, the trunk at the bottom, and a pointing emoji marking the
/// review being viewed. Lives between marker comments so resubmits replace
/// it in place, and self-repairs if the markers were hand-edited away.
fn update_stack_notes(
    review_provider: &dyn ReviewProvider,
    branch_parents: &[(String, String)],
    dry_run: bool,
) -> Result<()> {
    // The bottom branch's parent is the base the whole stack sits on.
    let Some(trunk) = branch_parents.first().map(|(_, parent)| parent.clone()) else {
        return Ok(());
    };

    let mut entries = Vec::new();
    for (branch, _) in branch_parents {
        match review_provider.review_for_branch(branch)? {
            Some(review) if review.branch == *branch => entries.push(review),
            _ => {
                // Without every review the overview would be wrong for all of
                // them (dry runs never created the missing ones).
                if !dry_run {
                    println!("skipped stack notes: no review found for {branch}");
                }
                return Ok(());
            }
        }
    }

    for index in 0..entries.len() {
        let note = build_stack_note(&entries, index, &trunk);
        let review = &entries[index];

        if dry_run {
            println!("would update stack note in {}", review.id);
            continue;
        }

        let body = review_provider.review_body(review)?;
        let updated = body_with_stack_note(&body, &note);
        if updated == body {
            continue;
        }

        review_provider.update_review_body(review, &updated)?;
        println!("updated stack note in {}", review.id);
    }

    Ok(())
}

/// Render the overview for one review: every PR in the stack leaf-first as a
/// linked bullet, a pointer on the review being viewed, the trunk in
/// backticks at the bottom, and a footer crediting the tool.
fn build_stack_note(entries: &[ReviewRequest], current: usize, trunk: &str) -> String {
    let mut lines = Vec::new();
    for (index, entry) in entries.iter().enumerate().rev() {
        let label = if entry.title.is_empty() {
            entry.id.clone()
        } else {
            format!("{} ({})", entry.title, entry.id)
        };
        let mut line = format!("- [{label}]({})", entry.url);
        if index == current {
            line.push_str(" \u{1F448}");
        }
        lines.push(line);
    }
    lines.push(format!("- `{trunk}`"));

    format!(
        "{}\n\n---\n\nStack managed by [git-stk]({TOOL_URL})",
        lines.join("\n")
    )
}

/// Replace the marker-delimited stack note in a review body, appending it at
/// the end. Damaged markup (orphaned or reordered markers, duplicates) is
/// stripped first, so the section self-repairs on the next submit.
fn body_with_stack_note(body: &str, note: &str) -> String {
    let section = format!("{STACK_NOTE_START}\n{note}\n{STACK_NOTE_END}");
    let cleaned = strip_stack_notes(body);

    if cleaned.trim().is_empty() {
        section
    } else {
        format!("{}\n\n{section}", cleaned.trim_end())
    }
}

/// Remove every well-formed marker section and any orphaned markers.
fn strip_stack_notes(body: &str) -> String {
    let mut result = body.to_owned();

    while let Some(start) = result.find(STACK_NOTE_START) {
        match result[start..].find(STACK_NOTE_END) {
            Some(end_offset) => {
                let end = start + end_offset + STACK_NOTE_END.len();
                result.replace_range(start..end, "");
            }
            None => result.replace_range(start..start + STACK_NOTE_START.len(), ""),
        }
    }
    while let Some(start) = result.find(STACK_NOTE_END) {
        result.replace_range(start..start + STACK_NOTE_END.len(), "");
    }

    // Collapse the blank-line craters left behind by removed sections.
    while result.contains("\n\n\n") {
        result = result.replace("\n\n\n", "\n\n");
    }
    result
}

/// Print the stack in a copy-paste markdown format for sharing with
/// reviewers: a summary line, then the PRs as an ordered bottom-to-top list
/// (merge order) with title, link, and state. Degrades to plain branch names
/// when reviews or the provider CLI are unavailable.
pub fn list_markdown() -> Result<()> {
    let current = git::current_branch()?;
    let root = stack::stack_root(&current)?;
    let branches: Vec<String> = stack::branch_and_descendants(&root)?
        .into_iter()
        .skip(1) // the root is the base, not part of the stack
        .collect();

    if branches.is_empty() {
        println!("no stacked branches");
        return Ok(());
    }

    let review_provider = detect_provider().ok().map(|p| review_provider(p.kind));
    let entries: Vec<(String, Option<ReviewRequest>)> = branches
        .iter()
        .map(|branch| {
            let review = review_provider
                .as_ref()
                .and_then(|rp| rp.review_for_branch(branch).ok().flatten())
                .filter(|review| review.branch == *branch);
            (branch.clone(), review)
        })
        .collect();

    println!("{}", markdown_summary(&entries, &root));
    println!();
    for (index, (branch, review)) in entries.iter().enumerate() {
        let item = match review {
            Some(review) => {
                let label = if review.title.is_empty() {
                    review.id.clone()
                } else {
                    format!("{} ({})", review.title, review.id)
                };
                format!("[{label}]({}) - {}", review.url, review.state)
            }
            None => format!("`{branch}` (no review)"),
        };
        println!("{}. {item}", index + 1);
    }

    Ok(())
}

/// One-line stack summary, e.g. "3 PRs, base `main`, 2 open / 1 merged".
fn markdown_summary(entries: &[(String, Option<ReviewRequest>)], base: &str) -> String {
    let total = entries.len();
    let reviews: Vec<&ReviewRequest> = entries.iter().filter_map(|(_, r)| r.as_ref()).collect();

    let mut summary = if reviews.is_empty() {
        format!(
            "{total} branch{}, base `{base}`",
            if total == 1 { "" } else { "es" }
        )
    } else if reviews.len() == total {
        format!(
            "{total} PR{}, base `{base}`",
            if total == 1 { "" } else { "s" }
        )
    } else {
        format!(
            "{total} branches ({} with reviews), base `{base}`",
            reviews.len()
        )
    };

    if !reviews.is_empty() {
        let mut counts = Vec::new();
        for (state, label) in [
            (ReviewState::Open, "open"),
            (ReviewState::Merged, "merged"),
            (ReviewState::Closed, "closed"),
        ] {
            let count = reviews
                .iter()
                .filter(|review| review.state == state)
                .count();
            if count > 0 {
                counts.push(format!("{count} {label}"));
            }
        }
        if !counts.is_empty() {
            summary.push_str(&format!(", {}", counts.join(" / ")));
        }
    }

    summary
}

/// Rebuild or verify local stack metadata. For branches missing a parent,
/// try the provider's review base first, then nearest-ancestor inference.
/// For branches with a parent, verify it exists and the recorded fork point
/// is still valid, re-deriving it when stale.
pub fn repair(dry_run: bool) -> Result<()> {
    let branches = git::local_branches()?;
    let trunk = stack::trunk_branch(&branches);

    // Provider lookup is best effort: repair must work without a remote or
    // an authenticated gh/glab.
    let provider = detect_provider()
        .ok()
        .map(|provider| (provider.kind, review_provider(provider.kind)));

    let mut repaired = 0;
    let mut verified = 0;
    let mut unresolved = 0;

    for branch in &branches {
        if Some(branch.as_str()) == trunk.as_deref() {
            continue;
        }

        if let Some(parent) = stack::parent_for_branch(branch)? {
            if !branches.contains(&parent) {
                println!(
                    "{branch}: parent {parent} does not exist locally; \
                     fix with `git stk adopt` or `git stk detach {branch}`"
                );
                unresolved += 1;
                continue;
            }

            let base_valid = matches!(
                stack::base_for_branch(branch)?,
                Some(base) if git::is_ancestor(&base, branch).unwrap_or(false)
            );
            if base_valid {
                verified += 1;
            } else {
                println!(
                    "{branch}: {} fork point from {parent}",
                    if dry_run {
                        "would re-record"
                    } else {
                        "re-recorded"
                    }
                );
                if !dry_run {
                    stack::record_base(branch, &parent);
                }
                repaired += 1;
            }
            continue;
        }

        let mut found: Option<(String, String)> = None;
        if let Some((kind, review_provider)) = &provider
            && let Ok(Some(review)) = review_provider.review_for_branch(branch)
            && review.branch == *branch
            && review.base != *branch
        {
            if branches.contains(&review.base) {
                found = Some((review.base.clone(), format!("{kind} review {}", review.id)));
            } else {
                println!(
                    "{branch}: review {} targets {}, which is not a local branch",
                    review.id, review.base
                );
            }
        }

        if found.is_none() {
            match nearest_ancestor_branch(branch, &branches)? {
                Ancestry::One(parent) => found = Some((parent, "ancestry".to_owned())),
                Ancestry::None => {
                    println!(
                        "{branch}: no parent found; attach manually with \
                         `git stk adopt {branch} --parent <parent>`"
                    );
                }
                Ancestry::Ambiguous(candidates) => {
                    println!(
                        "{branch}: ambiguous parent candidates ({}); attach manually with \
                         `git stk adopt`",
                        candidates.join(", ")
                    );
                }
            }
        }

        match found {
            Some((parent, source)) => {
                println!(
                    "{branch}: {} parent {parent} (from {source})",
                    if dry_run { "would set" } else { "set" }
                );
                if !dry_run {
                    stack::set_parent_for_branch(branch, &parent)?;
                    stack::record_base(branch, &parent);
                }
                repaired += 1;
            }
            None => unresolved += 1,
        }
    }

    println!(
        "repair complete: {repaired} {}repaired, {verified} verified, {unresolved} unresolved",
        if dry_run { "would be " } else { "" }
    );
    Ok(())
}

enum Ancestry {
    One(String),
    None,
    Ambiguous(Vec<String>),
}

/// Find the nearest other local branch whose tip is a strict ancestor of
/// `branch` - the best guess at its stack parent.
fn nearest_ancestor_branch(branch: &str, branches: &[String]) -> Result<Ancestry> {
    let tip = git::rev_parse(branch)?;

    let mut candidates: Vec<(String, String)> = Vec::new();
    for other in branches {
        if other == branch {
            continue;
        }
        let other_tip = git::rev_parse(other)?;
        // Equal tips (e.g. a just-created branch) leave the direction
        // ambiguous, so they are not usable candidates.
        if other_tip != tip && git::is_ancestor(other, branch)? {
            candidates.push((other.clone(), other_tip));
        }
    }

    // Keep only the nearest candidates: drop any that are ancestors of
    // another candidate (i.e. further from the branch).
    let nearest: Vec<String> = candidates
        .iter()
        .filter(|(candidate, candidate_tip)| {
            !candidates.iter().any(|(other, other_tip)| {
                other != candidate
                    && other_tip != candidate_tip
                    && git::is_ancestor(candidate, other).unwrap_or(false)
            })
        })
        .map(|(candidate, _)| candidate.clone())
        .collect();

    Ok(match nearest.len() {
        0 => Ancestry::None,
        1 => Ancestry::One(nearest.into_iter().next().expect("one candidate")),
        _ => Ancestry::Ambiguous(nearest),
    })
}

pub fn cleanup(branch: Option<&str>, dry_run: bool, delete_branch: bool) -> Result<()> {
    let branch = branch
        .map(str::to_owned)
        .map_or_else(git::current_branch, Ok)?;
    let branches = stack::branch_and_descendants(&branch)?;
    let current_branch = git::current_branch()?;
    let provider = detect_provider()?;
    let review_provider = review_provider(provider.kind);
    let mut cleaned = 0;
    let mut skipped = 0;

    for branch in branches {
        let Some(review) = review_provider.review_for_branch(&branch)? else {
            println!("skipped {branch}: no {} review found", provider.kind);
            skipped += 1;
            continue;
        };

        if review.state != ReviewState::Merged {
            println!("skipped {branch}: review {} is {}", review.id, review.state);
            skipped += 1;
            continue;
        }

        cleanup_merged_branch(review_provider.as_ref(), &branch, dry_run)?;
        cleanup_branch_deletion(&branch, &current_branch, dry_run, delete_branch)?;
        cleaned += 1;
    }

    println!("cleanup complete: {cleaned} cleaned, {skipped} skipped");
    Ok(())
}

fn cleanup_merged_branch(
    review_provider: &dyn ReviewProvider,
    branch: &str,
    dry_run: bool,
) -> Result<()> {
    let parent = stack::parent_for_branch(branch)?;
    let descendants = stack::branch_and_descendants(branch)?;
    let direct_children: Vec<_> = descendants
        .into_iter()
        .skip(1)
        .filter_map(|child| match stack::parent_for_branch(&child) {
            Ok(Some(child_parent)) if child_parent == branch => Some(Ok(child)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<_>>()?;

    for child in direct_children {
        match parent.as_deref() {
            Some(parent) => {
                println!(
                    "{} retarget {child} -> {parent}",
                    if dry_run { "would" } else { "will" }
                );
                update_child_review_base(review_provider, &child, parent, dry_run)?;
                if !dry_run {
                    // Record the fork point off the merged branch before
                    // retargeting, so the next restack replays only the
                    // child's own commits even after a squash merge.
                    if let Ok(base) = git::merge_base(branch, &child) {
                        stack::set_base_for_branch(&child, &base)?;
                    }
                    stack::set_parent_for_branch(&child, parent)?;
                }
            }
            None => {
                println!("{} detach {child}", if dry_run { "would" } else { "will" });
                if !dry_run {
                    stack::unset_parent_for_branch(&child)?;
                    stack::unset_base_for_branch(&child)?;
                }
            }
        }
    }

    println!("{} detach {branch}", if dry_run { "would" } else { "will" });
    if !dry_run {
        stack::unset_parent_for_branch(branch)?;
        stack::unset_base_for_branch(branch)?;
    }

    Ok(())
}

fn cleanup_branch_deletion(
    branch: &str,
    current_branch: &str,
    dry_run: bool,
    delete_branch: bool,
) -> Result<()> {
    if !delete_branch {
        return Ok(());
    }

    if branch == current_branch {
        bail!("refusing to delete currently checked out branch {branch}");
    }

    println!(
        "{} delete branch {branch}",
        if dry_run { "would" } else { "will" }
    );
    if !dry_run {
        git::delete_branch(branch)?;
    }

    Ok(())
}

fn update_child_review_base(
    review_provider: &dyn ReviewProvider,
    child: &str,
    parent: &str,
    dry_run: bool,
) -> Result<()> {
    let Some(review) = review_provider.review_for_branch(child)? else {
        return Ok(());
    };

    if review.state == ReviewState::Merged || review.base == parent {
        return Ok(());
    }

    println!(
        "{} update review {} -> {} ({})",
        if dry_run { "would" } else { "will" },
        review.branch,
        parent,
        review.id
    );
    if !dry_run {
        let output = review_provider.update_review_base(&review, parent)?;
        if !output.is_empty() {
            println!("{output}");
        }
    }

    Ok(())
}

fn branch_parents(branches: &[String]) -> Result<Vec<(String, String)>> {
    let mut branch_parents = Vec::new();
    for branch in branches {
        let Some(parent) = stack::parent_for_branch(branch)? else {
            bail!("{branch} has no stack parent; run `git stk adopt` or `git stk sync` first");
        };
        branch_parents.push((branch.to_owned(), parent));
    }
    Ok(branch_parents)
}

fn submit_branch(
    review_provider: &dyn ReviewProvider,
    branch: &str,
    parent: &str,
    dry_run: bool,
) -> Result<SubmitAction> {
    if let Some(review) = review_provider.review_for_branch(branch)? {
        if review.base == parent {
            if dry_run {
                println!(
                    "would skip {} -> {} ({})",
                    review.branch, review.base, review.id
                );
            } else {
                println!(
                    "{} already targets {} ({})",
                    review.branch, review.base, review.id
                );
            }
            return Ok(SubmitAction::Skipped);
        }

        let output = if dry_run {
            String::new()
        } else {
            review_provider.update_review_base(&review, parent)?
        };
        println!(
            "{} {} -> {} ({})",
            if dry_run { "would update" } else { "updated" },
            review.branch,
            parent,
            review.id
        );
        if !output.is_empty() {
            println!("{output}");
        }
    } else {
        let output = if dry_run {
            String::new()
        } else {
            review_provider.create_review(branch, parent)?
        };
        println!(
            "{} {branch} -> {parent}",
            if dry_run { "would create" } else { "created" }
        );
        if !output.is_empty() {
            println!("{output}");
        }
        return Ok(SubmitAction::Created);
    }

    Ok(SubmitAction::Updated)
}

#[derive(Debug, Default)]
struct SubmitSummary {
    created: usize,
    updated: usize,
    skipped: usize,
}

impl SubmitSummary {
    fn record(&mut self, action: SubmitAction) {
        match action {
            SubmitAction::Created => self.created += 1,
            SubmitAction::Updated => self.updated += 1,
            SubmitAction::Skipped => self.skipped += 1,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum SubmitAction {
    Created,
    Updated,
    Skipped,
}

pub fn detect_provider() -> Result<DetectedProvider> {
    if let Some(value) = git::config_get(PROVIDER_KEY)? {
        let Some(kind) = ProviderKind::parse(&value) else {
            bail!("unsupported stk.provider value {value:?}; expected github or gitlab");
        };

        return Ok(DetectedProvider {
            kind,
            source: ProviderSource::Config,
        });
    }

    let remote = git::config_get(REMOTE_KEY)?.unwrap_or_else(|| DEFAULT_REMOTE.to_owned());
    let Some(url) = git::remote_url(&remote)? else {
        bail!("could not detect provider: remote {remote:?} does not exist");
    };

    let Some(kind) = detect_provider_from_url(&url) else {
        bail!("could not detect provider from remote {remote} ({url})");
    };

    Ok(DetectedProvider {
        kind,
        source: ProviderSource::Remote { remote, url },
    })
}

fn detect_provider_from_url(url: &str) -> Option<ProviderKind> {
    let normalized = url.to_ascii_lowercase();

    if normalized.contains("github.com:") || normalized.contains("github.com/") {
        Some(ProviderKind::GitHub)
    } else if normalized.contains("gitlab.com:") || normalized.contains("gitlab.com/") {
        Some(ProviderKind::GitLab)
    } else {
        None
    }
}

fn review_provider(kind: ProviderKind) -> Box<dyn ReviewProvider> {
    match kind {
        ProviderKind::GitHub => Box::new(GitHubProvider),
        ProviderKind::GitLab => Box::new(GitLabProvider),
    }
}

fn command_output(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("failed to run {program}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if stderr.is_empty() {
            Err(anyhow!("{program} exited with status {}", output.status))
        } else {
            Err(anyhow!("{program} failed: {stderr}"))
        }
    }
}

fn parent_key(branch: &str) -> String {
    format!("branch.{branch}.stkParent")
}

impl fmt::Display for ReviewState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open => write!(formatter, "open"),
            Self::Merged => write!(formatter, "merged"),
            Self::Closed => write!(formatter, "closed"),
            Self::Unknown(state) => write!(formatter, "{state}"),
        }
    }
}

impl ReviewRequest {
    fn id_value(&self) -> &str {
        self.id
            .strip_prefix('#')
            .or_else(|| self.id.strip_prefix('!'))
            .unwrap_or(&self.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn review(id: &str, title: &str, url: &str) -> ReviewRequest {
        ReviewRequest {
            id: id.to_owned(),
            branch: String::new(),
            base: String::new(),
            state: ReviewState::Open,
            url: url.to_owned(),
            title: title.to_owned(),
        }
    }

    #[test]
    fn build_stack_note_lists_stack_leaf_first_with_pointer_and_trunk() {
        let entries = vec![
            review("#12", "Bottom change", "https://example.com/12"),
            review("#13", "Top change", "https://example.com/13"),
        ];

        let note = build_stack_note(&entries, 0, "main");
        assert_eq!(
            note,
            "- [Top change (#13)](https://example.com/13)\n\
             - [Bottom change (#12)](https://example.com/12) \u{1F448}\n\
             - `main`\n\n\
             ---\n\n\
             Stack managed by [git-stk](https://github.com/lararosekelley/git-stk)"
        );
    }

    #[test]
    fn build_stack_note_falls_back_to_id_without_title() {
        let entries = vec![review("#12", "", "https://example.com/12")];
        let note = build_stack_note(&entries, 0, "main");
        assert!(note.contains("- [#12](https://example.com/12) \u{1F448}"));
    }

    #[test]
    fn body_with_stack_note_appends_to_existing_body() {
        let updated = body_with_stack_note("Some PR description.\n", "stack list");
        assert_eq!(
            updated,
            "Some PR description.\n\n<!-- git-stk:stack -->\nstack list\n<!-- /git-stk:stack -->"
        );
    }

    #[test]
    fn body_with_stack_note_fills_empty_body() {
        let updated = body_with_stack_note("", "stack list");
        assert_eq!(
            updated,
            "<!-- git-stk:stack -->\nstack list\n<!-- /git-stk:stack -->"
        );
    }

    #[test]
    fn body_with_stack_note_replaces_existing_note() {
        let body = "Intro.\n\n<!-- git-stk:stack -->\nold list\n<!-- /git-stk:stack -->\n\nOutro.";
        let updated = body_with_stack_note(body, "new list");
        assert_eq!(
            updated,
            "Intro.\n\nOutro.\n\n<!-- git-stk:stack -->\nnew list\n<!-- /git-stk:stack -->"
        );
    }

    #[test]
    fn body_with_stack_note_is_idempotent() {
        let body = body_with_stack_note("Description.", "stack list");
        assert_eq!(body_with_stack_note(&body, "stack list"), body);
    }

    #[test]
    fn body_with_stack_note_repairs_orphaned_start_marker() {
        let body = "Intro.\n\n<!-- git-stk:stack -->\nleftover text";
        let updated = body_with_stack_note(body, "fresh list");
        assert_eq!(
            updated,
            "Intro.\n\nleftover text\n\n<!-- git-stk:stack -->\nfresh list\n<!-- /git-stk:stack -->"
        );
    }

    #[test]
    fn body_with_stack_note_repairs_orphaned_end_marker() {
        let body = "Intro.\nstray\n<!-- /git-stk:stack -->\nOutro.";
        let updated = body_with_stack_note(body, "fresh list");
        assert!(updated.matches("<!-- git-stk:stack -->").count() == 1);
        assert!(updated.matches("<!-- /git-stk:stack -->").count() == 1);
        assert!(updated.contains("Intro.\nstray"));
        assert!(updated.ends_with("<!-- /git-stk:stack -->"));
    }

    #[test]
    fn body_with_stack_note_repairs_reversed_and_duplicate_markers() {
        let body = "<!-- /git-stk:stack -->\nA\n<!-- git-stk:stack -->\nB\n\
                    <!-- git-stk:stack -->\nC\n<!-- /git-stk:stack -->\nD";
        let updated = body_with_stack_note(body, "fresh list");
        assert_eq!(updated.matches("<!-- git-stk:stack -->").count(), 1);
        assert_eq!(updated.matches("<!-- /git-stk:stack -->").count(), 1);
        assert!(updated.contains("fresh list"));
        assert!(updated.ends_with("<!-- /git-stk:stack -->"));
    }
}
