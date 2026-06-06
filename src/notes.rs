//! The stack overview maintained in every review description: build,
//! splice, and self-repair of the marker-delimited section.

use anyhow::Result;

use crate::providers::{ReviewProvider, ReviewRequest};

const STACK_NOTE_START: &str = "<!-- git-stk:stack -->";
const STACK_NOTE_END: &str = "<!-- /git-stk:stack -->";
const TOOL_URL: &str = "https://github.com/lararosekelley/git-stk";

/// Maintain a stack overview in every review body: the full PR list
/// leaf-first, the trunk at the bottom, and a pointing emoji marking the
/// review being viewed. Lives between marker comments so resubmits replace
/// it in place, and self-repairs if the markers were hand-edited away.
pub fn update_stack_notes(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ReviewState;

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
