use anyhow::Result;
use clap::ArgAction;

use crate::commands::Run;
use crate::providers::{ReviewRequest, ReviewState, detect_provider, review_provider};
use crate::{git, stack};

/// Print the current stack.
#[derive(Debug, clap::Args)]
pub struct List {
    /// Print a shareable markdown summary with PR links and states.
    #[arg(long, action = ArgAction::SetTrue)]
    markdown: bool,
}

impl Run for List {
    fn run(self) -> Result<()> {
        if self.markdown {
            list_markdown()
        } else {
            crate::stack::print_stack()
        }
    }
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
