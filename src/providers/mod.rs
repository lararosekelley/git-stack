use std::time::Duration;
use std::{fmt, process::Command};

use anyhow::{Context, Result, anyhow, bail};

use crate::git;
use crate::settings;

/// How long to keep polling a "no checks / no pipeline yet" result before
/// concluding there genuinely are none. A just-pushed branch's checks take a
/// moment to register, so concluding too early would either merge without
/// waiting or report a false failure.
pub(super) const CHECK_GRACE_POLLS: u32 = 6;

/// Delay between `wait_for_checks` polls.
pub(super) fn check_poll_interval() -> Duration {
    Duration::from_secs(5)
}

mod demo;
mod github;
mod gitlab;
mod json;

use demo::DemoProvider;
use github::GitHubProvider;
use gitlab::GitLabProvider;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ProviderKind {
    GitHub,
    GitLab,
    /// Offline stand-in: reviews in `.git`, merges as local squashes. Only
    /// ever selected explicitly via `stk.provider = demo`.
    Demo,
}

impl ProviderKind {
    fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "github" | "gh" => Some(Self::GitHub),
            "gitlab" | "glab" => Some(Self::GitLab),
            "demo" => Some(Self::Demo),
            _ => None,
        }
    }
}

impl fmt::Display for ProviderKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GitHub => write!(formatter, "github"),
            Self::GitLab => write!(formatter, "gitlab"),
            Self::Demo => write!(formatter, "demo"),
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
    pub draft: bool,
}

pub trait ReviewProvider {
    fn review_for_branch(&self, branch: &str) -> Result<Option<ReviewRequest>>;

    /// Like review_for_branch, but also finds closed reviews. Kept separate
    /// so flows that act on a review (submit, sync, cleanup) never mistake a
    /// dead review for a live one; only the stack-notes ledger wants closed
    /// state, to restyle the entry rather than drop it.
    fn review_for_branch_including_closed(&self, branch: &str) -> Result<Option<ReviewRequest>>;

    /// Open a review for the branch; with `draft`, as a draft.
    fn create_review(&self, branch: &str, base: &str, draft: bool) -> Result<String>;

    fn update_review_base(&self, review: &ReviewRequest, base: &str) -> Result<String>;

    fn review_body(&self, review: &ReviewRequest) -> Result<String>;

    fn update_review_body(&self, review: &ReviewRequest, body: &str) -> Result<String>;

    /// Merge the review with the given strategy: squash, rebase, or merge.
    /// With `auto`, schedule the merge for when required checks pass
    /// instead of merging now.
    fn merge_review(&self, review: &ReviewRequest, strategy: &str, auto: bool) -> Result<String>;

    /// Block until the review's checks settle. Ok(true) when they pass (or
    /// there are none), Ok(false) when something failed.
    fn wait_for_checks(&self, review: &ReviewRequest) -> Result<bool>;

    /// Mark a draft review as ready for review.
    fn mark_ready(&self, review: &ReviewRequest) -> Result<String>;

    /// Close the review without merging, deleting its source branch when
    /// `delete_branch`. Used to retire a review superseded by a branch rename.
    fn close_review(&self, review: &ReviewRequest, delete_branch: bool) -> Result<String>;

    /// Open the review in the user's browser.
    fn open_review(&self, review: &ReviewRequest) -> Result<String>;
}

pub fn detect_provider() -> Result<DetectedProvider> {
    if let Some(value) = git::config_get(settings::PROVIDER_KEY)? {
        let Some(kind) = ProviderKind::parse(&value) else {
            bail!("unsupported stk.provider value {value:?}; expected github, gitlab, or demo");
        };

        return Ok(DetectedProvider {
            kind,
            source: ProviderSource::Config,
        });
    }

    let remote = settings::remote()?;
    let Some(url) = git::remote_url(&remote)? else {
        bail!("could not detect provider: remote {remote:?} does not exist");
    };

    let gitlab_host = settings::gitlab_host()?;
    let Some(kind) = detect_provider_from_url(&url, gitlab_host.as_deref()) else {
        bail!("could not detect provider from remote {remote} ({url})");
    };

    Ok(DetectedProvider {
        kind,
        source: ProviderSource::Remote { remote, url },
    })
}

/// Detect the provider from a remote URL by its host. A configured
/// `stk.gitlabHost` widens GitLab detection to a self-hosted instance.
fn detect_provider_from_url(url: &str, gitlab_host: Option<&str>) -> Option<ProviderKind> {
    let normalized = url.to_ascii_lowercase();
    let host = host_of(&normalized);
    // Match the host itself or a subdomain of it, never a look-alike that
    // merely embeds the name (mygithub.com, evil.com/github.com/...).
    let is = |domain: &str| host == domain || host.ends_with(&format!(".{domain}"));

    if is("github.com") {
        Some(ProviderKind::GitHub)
    } else if is("gitlab.com") || gitlab_host.is_some_and(|host| is(&host.to_ascii_lowercase())) {
        Some(ProviderKind::GitLab)
    } else {
        None
    }
}

/// The host of a git remote URL: the part after any `scheme://` and `user@`,
/// up to the path, port, or scp-style `:`. Covers `https://host/owner/repo`,
/// `ssh://git@host/owner/repo`, and scp-like `git@host:owner/repo`.
fn host_of(url: &str) -> &str {
    let after_scheme = url.split_once("://").map_or(url, |(_, rest)| rest);
    let after_user = after_scheme
        .split_once('@')
        .map_or(after_scheme, |(_, rest)| rest);
    after_user.split(['/', ':']).next().unwrap_or(after_user)
}

pub(crate) fn review_provider(kind: ProviderKind) -> Box<dyn ReviewProvider> {
    match kind {
        ProviderKind::GitHub => Box::new(GitHubProvider),
        ProviderKind::GitLab => Box::new(GitLabProvider),
        ProviderKind::Demo => Box::new(DemoProvider),
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

/// Attempts and the pause between them for a merge the platform briefly
/// rejects because it has not finished recomputing the moved base. Landing a
/// tall stack moves the trunk on every merge, so this race is common.
const MERGE_ATTEMPTS: u32 = 3;
const MERGE_RETRY_BACKOFF: Duration = Duration::from_millis(1500);

/// Whether a failed merge is the platform transiently rejecting against a base
/// it has not settled - worth retrying - rather than a real failure (conflict,
/// failed check, closed review), which must surface immediately.
fn is_transient_merge_error(error: &anyhow::Error) -> bool {
    let text = error.to_string().to_lowercase();
    [
        "base branch was modified",
        "head branch was modified",
        "try the merge again",
    ]
    .iter()
    .any(|signature| text.contains(signature))
}

/// Run a merge, retrying while it fails transiently so the "base branch was
/// modified" race does not stop a `merge --all` loop.
fn merge_with_retry(attempt: impl FnMut() -> Result<String>) -> Result<String> {
    retry_transient_merge(MERGE_ATTEMPTS, MERGE_RETRY_BACKOFF, attempt)
}

fn retry_transient_merge(
    attempts: u32,
    backoff: Duration,
    mut attempt: impl FnMut() -> Result<String>,
) -> Result<String> {
    for remaining in (0..attempts).rev() {
        match attempt() {
            Ok(output) => return Ok(output),
            Err(error) if remaining > 0 && is_transient_merge_error(&error) => {
                std::thread::sleep(backoff);
            }
            Err(error) => return Err(error),
        }
    }
    // attempts is always nonzero, so the final iteration returns above.
    Err(anyhow!("merge retried with no attempts left"))
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
    pub(crate) fn id_value(&self) -> &str {
        self.id
            .strip_prefix('#')
            .or_else(|| self.id.strip_prefix('!'))
            .unwrap_or(&self.id)
    }

    /// "Title (#12)", or just the id when there is no title.
    pub fn label(&self) -> String {
        label(&self.title, &self.id)
    }
}

/// The display label for a review: "Title (#12)", or the bare id.
pub(crate) fn label(title: &str, id: &str) -> String {
    if title.is_empty() {
        id.to_owned()
    } else {
        format!("{title} ({id})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_error_is_retried_then_succeeds() {
        let mut calls = 0;
        let result = retry_transient_merge(3, Duration::ZERO, || {
            calls += 1;
            if calls < 2 {
                Err(anyhow!(
                    "gh failed: GraphQL: Base branch was modified. Review and try the merge again."
                ))
            } else {
                Ok("merged".to_owned())
            }
        });
        assert_eq!(result.unwrap(), "merged");
        assert_eq!(calls, 2, "should retry once then succeed");
    }

    #[test]
    fn a_persistent_transient_error_gives_up_after_the_attempt_budget() {
        let mut calls = 0;
        let result = retry_transient_merge(3, Duration::ZERO, || {
            calls += 1;
            Err(anyhow!("gh failed: Base branch was modified"))
        });
        assert!(result.is_err());
        assert_eq!(calls, 3, "should try exactly the budgeted number of times");
    }

    #[test]
    fn a_real_failure_is_not_retried() {
        let mut calls = 0;
        let result = retry_transient_merge(3, Duration::ZERO, || {
            calls += 1;
            Err(anyhow!(
                "gh failed: Pull request is not mergeable: conflicts"
            ))
        });
        assert!(result.is_err());
        assert_eq!(calls, 1, "a non-transient error must surface immediately");
    }
}
