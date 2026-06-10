use std::collections::BTreeMap;

use anyhow::{Result, bail};
use clap::ArgAction;

use crate::commands::Run;
use crate::{git, settings, stack, style};

/// Amend staged fixes into the stack commits that introduced the lines they
/// touch. Each hunk is routed to the commit a `git blame` attributes its
/// lines to; hunks that cannot be attributed are left in place.
#[derive(Debug, clap::Args)]
pub struct Absorb {
    /// Show the hunk -> commit routing without changing anything.
    #[arg(long, action = ArgAction::SetTrue)]
    dry_run: bool,
    /// Also absorb unstaged tracked changes, not just staged ones
    /// (overrides stk.absorbIncludeUnstaged).
    #[arg(long, action = ArgAction::SetTrue)]
    include_unstaged: bool,
}

impl Run for Absorb {
    fn run(self) -> Result<()> {
        let include_unstaged =
            self.include_unstaged || settings::bool_setting(settings::ABSORB_INCLUDE_UNSTAGED_KEY)?;
        let cached = !include_unstaged;

        let diff = git::diff_against_head(cached)?;
        if diff.trim().is_empty() {
            bail!(
                "no {} changes to absorb",
                if cached { "staged" } else { "tracked" }
            );
        }

        let current = git::current_branch()?;
        let owners = commit_owners(&current)?;
        let routes: Vec<Route> = parse_hunks(&diff)
            .into_iter()
            .map(|hunk| route(hunk, &owners))
            .collect::<Result<_>>()?;

        if self.dry_run {
            print_plan(&routes);
            return Ok(());
        }

        // TODO(absorb apply): the fixup + autosquash + restack apply lands in
        // the next change; until then preview with --dry-run.
        let _ = routes;
        bail!("applying absorbed changes is not wired up yet - re-run with --dry-run to preview");
    }
}

/// A single diff hunk, located by the lines it touches in HEAD.
struct Hunk {
    file: String,
    /// First HEAD line the hunk modifies (1-based).
    pre_start: usize,
    /// How many HEAD lines it modifies; zero for a pure insertion.
    pre_len: usize,
}

enum Route {
    Absorb {
        file: String,
        line: usize,
        branch: String,
        sha: String,
        subject: String,
    },
    Skip {
        file: String,
        line: usize,
        reason: String,
    },
}

/// Map every stack commit (current branch and below) to the branch that owns
/// it, so a blamed sha resolves to a branch in the routing table. Commits
/// outside this map - the trunk's, or older - are not absorbable.
fn commit_owners(current: &str) -> Result<BTreeMap<String, String>> {
    let path = stack::path_from_root(current)?; // bottom -> current, parent-first
    let mut owners = BTreeMap::new();

    for (index, branch) in path.iter().enumerate() {
        let parent = if index == 0 {
            stack::parent_for_branch(branch)?
        } else {
            Some(path[index - 1].clone())
        };
        let range = match parent {
            Some(parent) => format!("{parent}..{branch}"),
            // A rootless bottom: fall back to its recorded fork point.
            None => match stack::base_for_branch(branch)? {
                Some(base) => format!("{base}..{branch}"),
                None => continue,
            },
        };
        for sha in git::rev_list(&range)? {
            owners.entry(sha).or_insert_with(|| branch.clone());
        }
    }
    Ok(owners)
}

fn route(hunk: Hunk, owners: &BTreeMap<String, String>) -> Result<Route> {
    let skip = |reason: &str| Route::Skip {
        file: hunk.file.clone(),
        line: hunk.pre_start,
        reason: reason.to_owned(),
    };

    if hunk.pre_len == 0 {
        return Ok(skip("added lines - no commit to attribute"));
    }

    let shas = git::blame_line_shas(&hunk.file, hunk.pre_start, hunk.pre_len)?;
    match shas.as_slice() {
        [] => Ok(skip("could not attribute")),
        [sha] => match owners.get(sha) {
            Some(branch) => Ok(Route::Absorb {
                file: hunk.file,
                line: hunk.pre_start,
                branch: branch.clone(),
                sha: sha.clone(),
                subject: git::commit_subject(sha)?,
            }),
            None => Ok(skip("owned by a commit outside the stack")),
        },
        _ => Ok(skip("spans multiple commits")),
    }
}

/// Parse a `git diff --unified=0` into hunks. The pre-image range `-A,B` of
/// each `@@` header is the slice of HEAD the hunk touches.
fn parse_hunks(diff: &str) -> Vec<Hunk> {
    let mut hunks = Vec::new();
    let mut from_path = String::new();
    let mut file = String::new();

    for line in diff.lines() {
        if let Some(path) = line.strip_prefix("--- ") {
            from_path = strip_diff_prefix(path);
        } else if let Some(path) = line.strip_prefix("+++ ") {
            // A deletion targets /dev/null; attribute it to the old path.
            file = match strip_diff_prefix(path).as_str() {
                "/dev/null" => from_path.clone(),
                resolved => resolved.to_owned(),
            };
        } else if let Some(rest) = line.strip_prefix("@@ ")
            && let Some((pre_start, pre_len)) = parse_pre_image(rest)
            && !file.is_empty()
        {
            hunks.push(Hunk {
                file: file.clone(),
                pre_start,
                pre_len,
            });
        }
    }
    hunks
}

/// `a/foo`, `b/foo`, or `/dev/null` -> the bare path.
fn strip_diff_prefix(path: &str) -> String {
    path.strip_prefix("a/")
        .or_else(|| path.strip_prefix("b/"))
        .unwrap_or(path)
        .to_owned()
}

/// From a hunk header body like "-12,3 +12,2 @@ ...", read the pre-image
/// `(start, len)`. A missing length means one line; a zero start (pure add
/// against an empty side) keeps len zero.
fn parse_pre_image(rest: &str) -> Option<(usize, usize)> {
    let token = rest.split_whitespace().next()?.strip_prefix('-')?;
    let (start, len) = match token.split_once(',') {
        Some((start, len)) => (start.parse().ok()?, len.parse().ok()?),
        None => (token.parse().ok()?, 1),
    };
    Some((start, len))
}

fn print_plan(routes: &[Route]) {
    let absorbed = routes
        .iter()
        .filter(|route| matches!(route, Route::Absorb { .. }))
        .count();
    anstream::println!(
        "absorb plan ({absorbed} of {} hunk{})",
        routes.len(),
        if routes.len() == 1 { "" } else { "s" }
    );

    for route in routes {
        if let Route::Absorb {
            file,
            line,
            branch,
            sha,
            subject,
        } = route
        {
            anstream::println!(
                "  {}:{line} -> {} {}",
                file,
                style::branch(branch),
                style::dim(&format!("{} {subject}", &sha[..7.min(sha.len())]))
            );
        }
    }

    let skipped: Vec<&Route> = routes
        .iter()
        .filter(|route| matches!(route, Route::Skip { .. }))
        .collect();
    if !skipped.is_empty() {
        anstream::println!("{}", style::dim("unabsorbed (left in place):"));
        for route in skipped {
            if let Route::Skip { file, line, reason } = route {
                anstream::println!("  {file}:{line} {}", style::dim(reason));
            }
        }
    }
}
