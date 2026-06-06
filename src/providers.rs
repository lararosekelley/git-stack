use std::{fmt, process::Command};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;

use crate::{git, stack};

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
}

pub trait ReviewProvider {
    fn review_for_branch(&self, branch: &str) -> Result<Option<ReviewRequest>>;

    fn create_review(&self, branch: &str, base: &str) -> Result<String>;

    fn update_review_base(&self, review: &ReviewRequest, base: &str) -> Result<String>;

    fn review_body(&self, review: &ReviewRequest) -> Result<String>;

    fn update_review_body(&self, review: &ReviewRequest, body: &str) -> Result<String>;
}

struct GitHubProvider;

struct GitLabProvider;

impl ReviewProvider for GitHubProvider {
    fn review_for_branch(&self, branch: &str) -> Result<Option<ReviewRequest>> {
        let output = command_output(
            "gh",
            &[
                "pr",
                "list",
                "--head",
                branch,
                "--json",
                "number,state,baseRefName,headRefName,url",
            ],
        )?;
        if let Some(review) = parse_github_review(&output)? {
            return Ok(Some(review));
        }

        // gh pr list only returns open pull requests by default; check merged
        // ones too so cleanup can see landed reviews.
        let output = command_output(
            "gh",
            &[
                "pr",
                "list",
                "--head",
                branch,
                "--state",
                "merged",
                "--json",
                "number,state,baseRefName,headRefName,url",
            ],
        )?;
        parse_github_review(&output)
    }

    fn create_review(&self, branch: &str, base: &str) -> Result<String> {
        command_output(
            "gh",
            &["pr", "create", "--head", branch, "--base", base, "--fill"],
        )
    }

    fn update_review_base(&self, review: &ReviewRequest, base: &str) -> Result<String> {
        command_output("gh", &["pr", "edit", review.id_value(), "--base", base])
    }

    fn review_body(&self, review: &ReviewRequest) -> Result<String> {
        let output = command_output("gh", &["pr", "view", review.id_value(), "--json", "body"])?;
        parse_body_field(&output, "body")
    }

    fn update_review_body(&self, review: &ReviewRequest, body: &str) -> Result<String> {
        command_output("gh", &["pr", "edit", review.id_value(), "--body", body])
    }
}

impl ReviewProvider for GitLabProvider {
    fn review_for_branch(&self, branch: &str) -> Result<Option<ReviewRequest>> {
        let output = command_output(
            "glab",
            &["mr", "list", "--source-branch", branch, "--output", "json"],
        )?;
        if let Some(review) = parse_gitlab_review(&output)? {
            return Ok(Some(review));
        }

        // glab mr list only returns open merge requests by default; check
        // merged ones too so cleanup can see landed reviews.
        let output = command_output(
            "glab",
            &[
                "mr",
                "list",
                "--source-branch",
                branch,
                "--merged",
                "--output",
                "json",
            ],
        )?;
        parse_gitlab_review(&output)
    }

    fn create_review(&self, branch: &str, base: &str) -> Result<String> {
        command_output(
            "glab",
            &[
                "mr",
                "create",
                "--source-branch",
                branch,
                "--target-branch",
                base,
                "--fill",
            ],
        )
    }

    fn update_review_base(&self, review: &ReviewRequest, base: &str) -> Result<String> {
        command_output(
            "glab",
            &["mr", "update", review.id_value(), "--target-branch", base],
        )
    }

    fn review_body(&self, review: &ReviewRequest) -> Result<String> {
        let output = command_output(
            "glab",
            &["mr", "view", review.id_value(), "--output", "json"],
        )?;
        parse_body_field(&output, "description")
    }

    fn update_review_body(&self, review: &ReviewRequest, body: &str) -> Result<String> {
        command_output(
            "glab",
            &["mr", "update", review.id_value(), "--description", body],
        )
    }
}

fn parse_body_field(output: &str, field: &str) -> Result<String> {
    let value: serde_json::Value =
        serde_json::from_str(output).context("failed to parse provider JSON")?;
    Ok(value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned())
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

    for (branch, parent) in branch_parents {
        summary.record(submit_branch(
            review_provider.as_ref(),
            &branch,
            &parent,
            dry_run,
        )?);

        if submit_stack {
            ensure_stack_note(review_provider.as_ref(), &branch, &parent, dry_run)?;
        }
    }

    println!(
        "submit complete: {} created, {} updated, {} skipped",
        summary.created, summary.updated, summary.skipped
    );
    Ok(())
}

const STACK_NOTE_START: &str = "<!-- git-stk:stack -->";
const STACK_NOTE_END: &str = "<!-- /git-stk:stack -->";

/// Maintain a "Depends on" line in the review body so reviewers can see the
/// stack relationship. The line lives between marker comments, so resubmits
/// update it in place instead of appending duplicates.
fn ensure_stack_note(
    review_provider: &dyn ReviewProvider,
    branch: &str,
    parent: &str,
    dry_run: bool,
) -> Result<()> {
    // The bottom of a stack targets a branch without a review (e.g. main);
    // nothing to link in that case.
    let Some(parent_review) = review_provider.review_for_branch(parent)? else {
        return Ok(());
    };
    let Some(review) = review_provider.review_for_branch(branch)? else {
        return Ok(());
    };

    let note = format!("Depends on {}", parent_review.id);

    if dry_run {
        println!("would note '{note}' in {}", review.id);
        return Ok(());
    }

    let body = review_provider.review_body(&review)?;
    let updated = body_with_stack_note(&body, &note);
    if updated == body {
        return Ok(());
    }

    review_provider.update_review_body(&review, &updated)?;
    println!("noted '{note}' in {}", review.id);
    Ok(())
}

/// Insert or replace the marker-delimited stack note in a review body.
fn body_with_stack_note(body: &str, note: &str) -> String {
    let section = format!("{STACK_NOTE_START}\n{note}\n{STACK_NOTE_END}");

    if let (Some(start), Some(end)) = (body.find(STACK_NOTE_START), body.find(STACK_NOTE_END))
        && start < end
    {
        let mut updated = String::new();
        updated.push_str(&body[..start]);
        updated.push_str(&section);
        updated.push_str(&body[end + STACK_NOTE_END.len()..]);
        return updated;
    }

    if body.trim().is_empty() {
        section
    } else {
        format!("{}\n\n{section}", body.trim_end())
    }
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

fn parse_github_review(output: &str) -> Result<Option<ReviewRequest>> {
    let Some(review) = first_json_item(output)? else {
        return Ok(None);
    };

    Ok(Some(ReviewRequest {
        id: format!("#{}", required_string(&review, &["number"])?),
        branch: required_string(&review, &["headRefName"])?,
        base: required_string(&review, &["baseRefName"])?,
        state: parse_state(&required_string(&review, &["state"])?),
        url: required_string(&review, &["url"])?,
    }))
}

fn parse_gitlab_review(output: &str) -> Result<Option<ReviewRequest>> {
    let Some(review) = first_json_item(output)? else {
        return Ok(None);
    };

    Ok(Some(ReviewRequest {
        id: format!("!{}", required_string(&review, &["iid", "id"])?),
        branch: required_string(&review, &["source_branch", "sourceBranch"])?,
        base: required_string(&review, &["target_branch", "targetBranch"])?,
        state: parse_state(&required_string(&review, &["state"])?),
        url: required_string(&review, &["web_url", "webUrl", "url"])?,
    }))
}

fn first_json_item(output: &str) -> Result<Option<Value>> {
    let value: Value = serde_json::from_str(output).context("failed to parse provider JSON")?;
    match value {
        Value::Array(items) => Ok(items.into_iter().next()),
        Value::Object(_) => Ok(Some(value)),
        _ => bail!("provider JSON must be an object or array"),
    }
}

fn required_string(value: &Value, keys: &[&str]) -> Result<String> {
    for key in keys {
        if let Some(field) = value.get(*key) {
            if let Some(value) = field.as_str() {
                return Ok(value.to_owned());
            }
            if let Some(value) = field.as_i64() {
                return Ok(value.to_string());
            }
            if let Some(value) = field.as_u64() {
                return Ok(value.to_string());
            }
        }
    }

    bail!(
        "provider JSON missing required field: {}",
        keys.join(" or ")
    )
}

fn parse_state(state: &str) -> ReviewState {
    match state.to_ascii_lowercase().as_str() {
        "open" | "opened" => ReviewState::Open,
        "merged" => ReviewState::Merged,
        "closed" => ReviewState::Closed,
        _ => ReviewState::Unknown(state.to_owned()),
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

    #[test]
    fn parse_github_review_reads_first_array_item() {
        let review = parse_github_review(
            r#"[{"number":12,"state":"OPEN","baseRefName":"main","headRefName":"feature/a","url":"https://github.com/owner/repo/pull/12"}]"#,
        )
        .expect("parse review")
        .expect("review exists");

        assert_eq!(
            review,
            ReviewRequest {
                id: "#12".to_owned(),
                branch: "feature/a".to_owned(),
                base: "main".to_owned(),
                state: ReviewState::Open,
                url: "https://github.com/owner/repo/pull/12".to_owned(),
            }
        );
    }

    #[test]
    fn parse_gitlab_review_reads_snake_case_fields() {
        let review = parse_gitlab_review(
            r#"[{"iid":34,"state":"merged","target_branch":"feature/a","source_branch":"feature/b","web_url":"https://gitlab.com/owner/repo/-/merge_requests/34"}]"#,
        )
        .expect("parse review")
        .expect("review exists");

        assert_eq!(
            review,
            ReviewRequest {
                id: "!34".to_owned(),
                branch: "feature/b".to_owned(),
                base: "feature/a".to_owned(),
                state: ReviewState::Merged,
                url: "https://gitlab.com/owner/repo/-/merge_requests/34".to_owned(),
            }
        );
    }

    #[test]
    fn parse_gitlab_review_reads_camel_case_fields() {
        let review = parse_gitlab_review(
            r#"[{"id":34,"state":"closed","targetBranch":"feature/a","sourceBranch":"feature/b","webUrl":"https://gitlab.com/owner/repo/-/merge_requests/34"}]"#,
        )
        .expect("parse review")
        .expect("review exists");

        assert_eq!(review.id, "!34");
        assert_eq!(review.branch, "feature/b");
        assert_eq!(review.base, "feature/a");
        assert_eq!(review.state, ReviewState::Closed);
        assert_eq!(
            review.url,
            "https://gitlab.com/owner/repo/-/merge_requests/34"
        );
    }

    #[test]
    fn parse_review_accepts_object_output() {
        let review = parse_github_review(
            r#"{"number":12,"state":"OPEN","baseRefName":"main","headRefName":"feature/a","url":"https://github.com/owner/repo/pull/12"}"#,
        )
        .expect("parse review")
        .expect("review exists");

        assert_eq!(review.id, "#12");
    }

    #[test]
    fn parse_review_empty_array_returns_none() {
        assert_eq!(parse_github_review("[]").expect("parse review"), None);
        assert_eq!(parse_gitlab_review("[]").expect("parse review"), None);
    }

    #[test]
    fn parse_review_errors_on_missing_required_field() {
        let error = parse_github_review(
            r#"[{"number":12,"state":"OPEN","baseRefName":"main","url":"https://github.com/owner/repo/pull/12"}]"#,
        )
        .expect_err("missing head branch should fail");

        assert!(
            error
                .to_string()
                .contains("provider JSON missing required field: headRefName"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn parse_review_preserves_unknown_state() {
        let review = parse_github_review(
            r#"[{"number":12,"state":"READY_FOR_REVIEW","baseRefName":"main","headRefName":"feature/a","url":"https://github.com/owner/repo/pull/12"}]"#,
        )
        .expect("parse review")
        .expect("review exists");

        assert_eq!(
            review.state,
            ReviewState::Unknown("READY_FOR_REVIEW".to_owned())
        );
    }

    #[test]
    fn body_with_stack_note_appends_to_existing_body() {
        let updated = body_with_stack_note("Some PR description.\n", "Depends on #12");
        assert_eq!(
            updated,
            "Some PR description.\n\n<!-- git-stk:stack -->\nDepends on #12\n<!-- /git-stk:stack -->"
        );
    }

    #[test]
    fn body_with_stack_note_fills_empty_body() {
        let updated = body_with_stack_note("", "Depends on !34");
        assert_eq!(
            updated,
            "<!-- git-stk:stack -->\nDepends on !34\n<!-- /git-stk:stack -->"
        );
    }

    #[test]
    fn body_with_stack_note_replaces_existing_note() {
        let body =
            "Intro.\n\n<!-- git-stk:stack -->\nDepends on #12\n<!-- /git-stk:stack -->\n\nOutro.";
        let updated = body_with_stack_note(body, "Depends on #99");
        assert_eq!(
            updated,
            "Intro.\n\n<!-- git-stk:stack -->\nDepends on #99\n<!-- /git-stk:stack -->\n\nOutro."
        );
    }

    #[test]
    fn body_with_stack_note_is_idempotent() {
        let body = body_with_stack_note("Description.", "Depends on #12");
        assert_eq!(body_with_stack_note(&body, "Depends on #12"), body);
    }

    #[test]
    fn parse_body_field_reads_field_and_defaults_empty() {
        assert_eq!(
            parse_body_field(r#"{"body":"hello"}"#, "body").expect("parse body"),
            "hello"
        );
        assert_eq!(
            parse_body_field(r#"{"description":null}"#, "description").expect("parse body"),
            ""
        );
        assert_eq!(parse_body_field(r#"{}"#, "body").expect("parse body"), "");
    }
}
