//! Marker-delimited sections maintained in every review description: the
//! stack-overview ledger and issue-closing links. Build, splice, parse, and
//! self-repair of `<!-- git-stk:NAME -->` sections.

use anyhow::Result;
use serde_json::{Value, json};

use crate::providers::{ReviewProvider, ReviewRequest, ReviewState};

const STACK_SECTION: &str = "stack";
const CLOSES_SECTION: &str = "closes";
const DESCRIPTION_SECTION: &str = "description";
const DATA_PREFIX: &str = "<!-- git-stk:data ";
const COMMENT_END: &str = "-->";
const TOOL_URL: &str = "https://github.com/lararosekelley/git-stk";
const LOGO_URL: &str =
    "https://raw.githubusercontent.com/lararosekelley/git-stk/main/assets/logo.svg";

fn marker_start(name: &str) -> String {
    format!("<!-- git-stk:{name} -->")
}

fn marker_end(name: &str) -> String {
    format!("<!-- /git-stk:{name} -->")
}

/// One row of the stack-overview ledger. Live rows come from the provider;
/// merged and closed rows outlive their local branches and are carried
/// forward from the previous note, so the ledger is append-only history
/// rather than a snapshot of the live stack.
#[derive(Debug, Clone, PartialEq, Eq)]
struct NoteEntry {
    id: String,
    url: String,
    title: String,
    state: String,
}

impl NoteEntry {
    fn from_review(review: &ReviewRequest) -> Self {
        Self {
            id: review.id.clone(),
            url: review.url.clone(),
            title: review.title.clone(),
            state: review.state.to_string(),
        }
    }

    /// A review the ledger only knows by its row: enough identity to fetch
    /// and update the body, nothing more.
    fn to_review(&self) -> ReviewRequest {
        let state = match self.state.as_str() {
            "open" => ReviewState::Open,
            "merged" => ReviewState::Merged,
            "closed" => ReviewState::Closed,
            other => ReviewState::Unknown(other.to_owned()),
        };
        ReviewRequest {
            id: self.id.clone(),
            branch: String::new(),
            base: String::new(),
            state,
            url: self.url.clone(),
            title: self.title.clone(),
        }
    }

    /// Rows recovered from a hand-edited note may be missing the id, so the
    /// URL doubles as identity.
    fn matches(&self, other: &Self) -> bool {
        (!self.id.is_empty() && self.id == other.id)
            || (!self.url.is_empty() && self.url == other.url)
    }
}

/// Maintain a stack overview in every review body: the full ledger
/// leaf-first, the trunk at the bottom, and a pointing emoji marking the
/// review being viewed. Lives between marker comments so refreshes replace
/// it in place, and self-repairs if the markers were hand-edited away.
/// Merged and closed entries are preserved from the previous note and
/// restyled instead of dropped.
pub fn update_stack_notes(
    review_provider: &dyn ReviewProvider,
    branch_parents: &[(String, String)],
    dry_run: bool,
) -> Result<()> {
    // The bottom branch's parent is the base the whole stack sits on.
    let Some(trunk) = branch_parents.first().map(|(_, parent)| parent.clone()) else {
        return Ok(());
    };

    let mut live = Vec::new();
    for (branch, _) in branch_parents {
        // The closed-inclusive lookup is deliberate: a review closed on the
        // platform should show up red in the ledger, even though every flow
        // that acts on a review treats it as gone.
        match review_provider.review_for_branch_including_closed(branch)? {
            Some(review) if review.branch == *branch => live.push(review),
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

    if dry_run {
        for review in &live {
            println!("would update stack note in {}", review.id);
        }
        return Ok(());
    }

    // Fetch every live body up front: each carries its own copy of the
    // ledger, and the union keeps history alive even in bodies that have
    // never seen it (e.g. a review created after earlier entries merged).
    let mut bodies = Vec::new();
    for review in &live {
        bodies.push(review_provider.review_body(review)?);
    }

    let live_entries: Vec<NoteEntry> = live.iter().map(NoteEntry::from_review).collect();
    let mut historical: Vec<NoteEntry> = Vec::new();
    for body in &bodies {
        let Some(section) = extract_section(body, STACK_SECTION) else {
            continue;
        };
        for entry in parse_ledger(section) {
            let known = live_entries.iter().chain(historical.iter());
            if !known
                .into_iter()
                .any(|entry_known| entry_known.matches(&entry))
            {
                historical.push(entry);
            }
        }
    }

    // Bottom-first, like the stack itself: already-landed history below,
    // the live stack on top of it.
    let mut entries = historical.clone();
    entries.extend(live_entries);

    for (offset, review) in live.iter().enumerate() {
        let note = build_stack_note(&entries, historical.len() + offset, &trunk);
        let updated = body_with_section(&bodies[offset], STACK_SECTION, &note);
        if updated == bodies[offset] {
            continue;
        }

        review_provider.update_review_body(review, &updated)?;
        println!("updated stack note in {}", review.id);
    }

    // Historical reviews get the refreshed ledger too, so a just-merged
    // review stops presenting the stack as it was. Failures are non-fatal:
    // an old review may have become unreachable.
    for (index, entry) in historical.iter().enumerate() {
        if entry.id.is_empty() {
            continue;
        }
        let review = entry.to_review();
        let Ok(body) = review_provider.review_body(&review) else {
            println!("skipped stack note in {}: could not read body", review.id);
            continue;
        };

        let note = build_stack_note(&entries, index, &trunk);
        let updated = body_with_section(&body, STACK_SECTION, &note);
        if updated == body {
            continue;
        }

        if review_provider
            .update_review_body(&review, &updated)
            .is_err()
        {
            println!("skipped stack note in {}: could not update body", review.id);
            continue;
        }
        println!("updated stack note in {}", review.id);
    }

    Ok(())
}

/// Add a `Closes #N` line to each branch's review when the branch name
/// references an issue (e.g. `123-fix-thing`, `fix/issue-123`), so the
/// platform closes the issue when the review merges. Branches without an
/// issue reference are passed over silently.
pub fn update_closes_notes(
    review_provider: &dyn ReviewProvider,
    branches: &[String],
    dry_run: bool,
) -> Result<()> {
    for branch in branches {
        let Some(issue) = issue_number_from_branch(branch) else {
            continue;
        };

        let Some(review) = review_provider.review_for_branch(branch)? else {
            // On a dry run the review was likely never created; for real the
            // submit just failed to produce one, which deserves a mention.
            if dry_run {
                println!("would link issue #{issue} in the review for {branch}");
            } else {
                println!("skipped issue link: no review found for {branch}");
            }
            continue;
        };

        if review.branch != *branch || review.state == ReviewState::Merged {
            continue;
        }

        if dry_run {
            println!("would link issue #{issue} in {}", review.id);
            continue;
        }

        let body = review_provider.review_body(&review)?;
        let updated = body_with_closes_note(&body, &format!("Closes #{issue}"));
        if updated == body {
            continue;
        }

        review_provider.update_review_body(&review, &updated)?;
        println!("linked issue #{issue} in {}", review.id);
    }

    Ok(())
}

/// Write (or, with an empty string, clear) the description block in the
/// branch's review body. Unlike the stack overview the block is sticky:
/// submits without `--desc` never touch it.
pub fn update_description_note(
    review_provider: &dyn ReviewProvider,
    branch: &str,
    description: &str,
    dry_run: bool,
) -> Result<()> {
    let verb = if description.is_empty() {
        "clear"
    } else {
        "set"
    };

    let Some(review) = review_provider.review_for_branch(branch)? else {
        if dry_run {
            println!("would {verb} the description on the review for {branch}");
        } else {
            println!("skipped description: no review found for {branch}");
        }
        return Ok(());
    };
    if review.branch != *branch {
        println!(
            "skipped description: review {} belongs to {}",
            review.id, review.branch
        );
        return Ok(());
    }

    if dry_run {
        println!("would {verb} the description in {}", review.id);
        return Ok(());
    }

    let body = review_provider.review_body(&review)?;
    let updated = if description.is_empty() {
        if !body.contains(&marker_start(DESCRIPTION_SECTION)) {
            return Ok(());
        }
        strip_sections(&body, DESCRIPTION_SECTION)
            .trim_end()
            .to_owned()
    } else {
        body_with_description_note(&body, description)
    };
    if updated == body {
        return Ok(());
    }

    review_provider.update_review_body(&review, &updated)?;
    println!(
        "{} description in {}",
        if description.is_empty() {
            "cleared"
        } else {
            "set"
        },
        review.id
    );
    Ok(())
}

/// The issue number a branch name refers to, if any. A path segment that
/// starts with the number (`123-fix-thing`, `fix/123-thing`, bare `123`) or
/// prefixes it with issue/issues (`issue-123`, `fix/issues-123-thing`)
/// counts; trailing numbers do not, to keep version-ish names from
/// closing unrelated issues.
fn issue_number_from_branch(branch: &str) -> Option<u64> {
    for segment in branch.split('/') {
        let lowered = segment.to_ascii_lowercase();
        let candidate = lowered
            .strip_prefix("issue-")
            .or_else(|| lowered.strip_prefix("issues-"))
            .unwrap_or(&lowered);

        let end = candidate
            .find(|character: char| !character.is_ascii_digit())
            .unwrap_or(candidate.len());
        let (digits, rest) = candidate.split_at(end);
        if digits.is_empty() || !(rest.is_empty() || rest.starts_with('-')) {
            continue;
        }

        if let Ok(number) = digits.parse::<u64>()
            && number > 0
        {
            return Some(number);
        }
    }

    None
}

/// Render the overview for one review: a hidden data line carrying the
/// ledger, every entry leaf-first as a status-styled bullet, a pointer on
/// the review being viewed, the trunk in backticks at the bottom, and a
/// footer crediting the tool.
fn build_stack_note(entries: &[NoteEntry], current: usize, trunk: &str) -> String {
    let mut lines = vec![data_line(entries)];
    for (index, entry) in entries.iter().enumerate().rev() {
        lines.push(render_entry(entry, index == current));
    }
    lines.push(format!("- `{trunk}`"));

    format!(
        "{}\n\n---\n\nStack managed by \
         <img src=\"{LOGO_URL}\" width=\"12\" height=\"12\" alt=\"\" /> \
         [git-stk]({TOOL_URL})",
        lines.join("\n")
    )
}

/// A status emoji as the bullet, strikethrough plus a suffix for entries
/// that have left the stack, and the pointer on the current review.
fn render_entry(entry: &NoteEntry, current: bool) -> String {
    let label = if entry.title.is_empty() {
        entry.id.clone()
    } else {
        format!("{} ({})", entry.title, entry.id)
    };
    let link = format!("[{label}]({})", entry.url);

    let mut line = match entry.state.as_str() {
        "merged" => format!("- \u{1F7E3} ~~{link}~~ (merged)"),
        "closed" => format!("- \u{1F534} ~~{link}~~ (closed)"),
        _ => format!("- \u{1F7E2} {link}"),
    };
    if current {
        line.push_str(" \u{1F448}");
    }
    line
}

/// One hidden machine-readable line so the ledger survives restyling: the
/// rendered bullets are presentation, this is the data.
fn data_line(entries: &[NoteEntry]) -> String {
    let data = Value::Array(
        entries
            .iter()
            .map(|entry| {
                json!({
                    "id": entry.id,
                    "url": entry.url,
                    "title": entry.title,
                    "state": entry.state,
                })
            })
            .collect(),
    );

    // '>' only ever appears inside JSON strings, so escaping it globally
    // keeps a title containing "-->" from terminating the comment early.
    let encoded = data.to_string().replace('>', "\\u003e");
    format!("{DATA_PREFIX}{encoded} {COMMENT_END}")
}

/// Read the ledger out of a stack section: the embedded data line when it
/// is intact, otherwise whatever the rendered bullets still reveal (the
/// hidden line may have been edited or deleted along with everything else).
fn parse_ledger(section: &str) -> Vec<NoteEntry> {
    for line in section.lines() {
        if let Some(rest) = line.trim().strip_prefix(DATA_PREFIX)
            && let Some(encoded) = rest.trim_end().strip_suffix(COMMENT_END)
            && let Some(entries) = parse_data_json(encoded.trim())
        {
            return entries;
        }
    }

    section.lines().filter_map(parse_entry_line).collect()
}

fn parse_data_json(encoded: &str) -> Option<Vec<NoteEntry>> {
    let value: Value = serde_json::from_str(encoded).ok()?;
    let mut entries = Vec::new();
    for item in value.as_array()? {
        entries.push(NoteEntry {
            id: item.get("id")?.as_str()?.to_owned(),
            url: item.get("url")?.as_str()?.to_owned(),
            title: item
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            state: item
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or("open")
                .to_owned(),
        });
    }
    Some(entries)
}

/// Best-effort recovery of one rendered bullet: `[label](url)` plus the
/// state suffix. The trunk line (backticks, no link) and the footer fall
/// through to None.
fn parse_entry_line(line: &str) -> Option<NoteEntry> {
    let rest = line.trim().strip_prefix("- ")?;
    if rest.starts_with('`') {
        return None;
    }

    let open = rest.find('[')?;
    let split = rest[open..].find("](")? + open;
    let close = rest[split + 2..].find(')')? + split + 2;
    let label = &rest[open + 1..split];
    let url = &rest[split + 2..close];
    let tail = &rest[close + 1..];

    let state = if tail.contains("(merged)") {
        "merged"
    } else if tail.contains("(closed)") {
        "closed"
    } else {
        "open"
    };

    // "Title (#12)" carries both; a bare "#12" label is just the id.
    let (title, id) = match rest[open + 1..split].rfind(" (") {
        Some(position) if label.ends_with(')') => {
            let id = &label[position + 2..label.len() - 1];
            if id.starts_with('#') || id.starts_with('!') {
                (label[..position].to_owned(), id.to_owned())
            } else {
                (label.to_owned(), String::new())
            }
        }
        _ if label.starts_with('#') || label.starts_with('!') => (String::new(), label.to_owned()),
        _ => (label.to_owned(), String::new()),
    };

    Some(NoteEntry {
        id,
        url: url.to_owned(),
        title,
        state: state.to_owned(),
    })
}

/// The content of the first well-formed marker section, if any.
fn extract_section<'body>(body: &'body str, name: &str) -> Option<&'body str> {
    let start_marker = marker_start(name);
    let end_marker = marker_end(name);
    let start = body.find(&start_marker)? + start_marker.len();
    let length = body[start..].find(&end_marker)?;
    Some(&body[start..start + length])
}

/// Replace the marker-delimited section in a review body, appending it at
/// the end. Damaged markup (orphaned or reordered markers, duplicates) is
/// stripped first, so the section self-repairs on the next update.
fn body_with_section(body: &str, name: &str, content: &str) -> String {
    let section = format!("{}\n{content}\n{}", marker_start(name), marker_end(name));
    let cleaned = strip_sections(body, name);

    if cleaned.trim().is_empty() {
        section
    } else {
        format!("{}\n\n{section}", cleaned.trim_end())
    }
}

/// Splice the closes note in, keeping it above the stack overview so the
/// closing keyword reads as part of the description rather than the footer.
fn body_with_closes_note(body: &str, note: &str) -> String {
    body_with_section_before(body, CLOSES_SECTION, note, &[STACK_SECTION])
}

/// Splice the user's description in, above every managed section so it
/// reads as the opening of the body.
fn body_with_description_note(body: &str, description: &str) -> String {
    body_with_section_before(
        body,
        DESCRIPTION_SECTION,
        description,
        &[CLOSES_SECTION, STACK_SECTION],
    )
}

/// Replace the named section, keeping it above the first of the `before`
/// sections present in the body; without one, append at the end.
fn body_with_section_before(body: &str, name: &str, content: &str, before: &[&str]) -> String {
    let section = format!("{}\n{content}\n{}", marker_start(name), marker_end(name));
    let cleaned = strip_sections(body, name);

    let position = before
        .iter()
        .filter_map(|other| cleaned.find(&marker_start(other)))
        .min();
    match position {
        Some(position) => {
            let head = cleaned[..position].trim_end();
            let tail = &cleaned[position..];
            if head.is_empty() {
                format!("{section}\n\n{tail}")
            } else {
                format!("{head}\n\n{section}\n\n{tail}")
            }
        }
        None if cleaned.trim().is_empty() => section,
        None => format!("{}\n\n{section}", cleaned.trim_end()),
    }
}

/// Remove every well-formed marker section and any orphaned markers.
fn strip_sections(body: &str, name: &str) -> String {
    let start_marker = marker_start(name);
    let end_marker = marker_end(name);
    let mut result = body.to_owned();

    while let Some(start) = result.find(&start_marker) {
        match result[start..].find(&end_marker) {
            Some(end_offset) => {
                let end = start + end_offset + end_marker.len();
                result.replace_range(start..end, "");
            }
            None => result.replace_range(start..start + start_marker.len(), ""),
        }
    }
    while let Some(start) = result.find(&end_marker) {
        result.replace_range(start..start + end_marker.len(), "");
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

    fn entry(id: &str, title: &str, url: &str, state: &str) -> NoteEntry {
        NoteEntry {
            id: id.to_owned(),
            url: url.to_owned(),
            title: title.to_owned(),
            state: state.to_owned(),
        }
    }

    #[test]
    fn build_stack_note_lists_ledger_leaf_first_with_pointer_and_trunk() {
        let entries = vec![
            entry("#12", "Bottom change", "https://example.com/12", "open"),
            entry("#13", "Top change", "https://example.com/13", "open"),
        ];

        let note = build_stack_note(&entries, 0, "main");
        let lines: Vec<&str> = note.lines().collect();
        assert!(
            lines[0].starts_with(DATA_PREFIX),
            "missing data line: {note}"
        );
        assert_eq!(
            lines[1],
            "- \u{1F7E2} [Top change (#13)](https://example.com/13)"
        );
        assert_eq!(
            lines[2],
            "- \u{1F7E2} [Bottom change (#12)](https://example.com/12) \u{1F448}"
        );
        assert_eq!(lines[3], "- `main`");
        assert!(note.ends_with(
            "Stack managed by \
             <img src=\"https://raw.githubusercontent.com/lararosekelley/git-stk/main/assets/logo.svg\" \
             width=\"12\" height=\"12\" alt=\"\" /> \
             [git-stk](https://github.com/lararosekelley/git-stk)"
        ));
    }

    #[test]
    fn build_stack_note_styles_merged_and_closed_entries() {
        let entries = vec![
            entry("#11", "Landed", "https://example.com/11", "merged"),
            entry("#12", "Abandoned", "https://example.com/12", "closed"),
            entry("#13", "Live", "https://example.com/13", "open"),
        ];

        let note = build_stack_note(&entries, 2, "main");
        assert!(note.contains("- \u{1F7E2} [Live (#13)](https://example.com/13) \u{1F448}"));
        assert!(
            note.contains("- \u{1F534} ~~[Abandoned (#12)](https://example.com/12)~~ (closed)")
        );
        assert!(note.contains("- \u{1F7E3} ~~[Landed (#11)](https://example.com/11)~~ (merged)"));
    }

    #[test]
    fn build_stack_note_falls_back_to_id_without_title() {
        let entries = vec![entry("#12", "", "https://example.com/12", "open")];
        let note = build_stack_note(&entries, 0, "main");
        assert!(note.contains("- \u{1F7E2} [#12](https://example.com/12) \u{1F448}"));
    }

    #[test]
    fn parse_ledger_round_trips_the_data_line() {
        let entries = vec![
            entry("#11", "Landed", "https://example.com/11", "merged"),
            entry("#13", "Top -> change", "https://example.com/13", "open"),
        ];

        let note = build_stack_note(&entries, 1, "main");
        assert_eq!(parse_ledger(&note), entries);
    }

    #[test]
    fn data_line_survives_a_title_containing_a_comment_terminator() {
        let entries = vec![entry(
            "#12",
            "weird --> title",
            "https://example.com/12",
            "open",
        )];
        let line = data_line(&entries);
        assert!(!line[DATA_PREFIX.len()..line.len() - COMMENT_END.len()].contains("-->"));
        assert_eq!(parse_ledger(&line), entries);
    }

    #[test]
    fn parse_ledger_recovers_entries_from_bullets_when_data_line_is_gone() {
        let entries = vec![
            entry("#11", "Landed", "https://example.com/11", "merged"),
            entry("#12", "", "https://example.com/12", "closed"),
            entry("#13", "Live", "https://example.com/13", "open"),
        ];

        let note = build_stack_note(&entries, 2, "main");
        let without_data: String = note
            .lines()
            .filter(|line| !line.trim().starts_with(DATA_PREFIX))
            .collect::<Vec<_>>()
            .join("\n");

        // Bullets render leaf-first, so recovery reverses back to
        // bottom-first ledger order.
        let mut recovered = parse_ledger(&without_data);
        recovered.reverse();
        assert_eq!(recovered, entries);
    }

    #[test]
    fn parse_ledger_falls_back_to_bullets_when_data_line_is_corrupt() {
        let section = "<!-- git-stk:data [{\"id\": -->\n\
                       - \u{1F7E3} ~~[Landed (#11)](https://example.com/11)~~ (merged)\n\
                       - `main`";
        assert_eq!(
            parse_ledger(section),
            vec![entry("#11", "Landed", "https://example.com/11", "merged")]
        );
    }

    #[test]
    fn parse_ledger_reads_the_legacy_unstyled_format() {
        let section = "- [Top change (#13)](https://example.com/13)\n\
                       - [Bottom change (#12)](https://example.com/12) \u{1F448}\n\
                       - `main`\n\n---\n\nfooter";
        assert_eq!(
            parse_ledger(section),
            vec![
                entry("#13", "Top change", "https://example.com/13", "open"),
                entry("#12", "Bottom change", "https://example.com/12", "open"),
            ]
        );
    }

    #[test]
    fn issue_number_from_branch_reads_supported_shapes() {
        assert_eq!(issue_number_from_branch("123-fix-thing"), Some(123));
        assert_eq!(issue_number_from_branch("fix/123-thing"), Some(123));
        assert_eq!(issue_number_from_branch("fix/issue-123"), Some(123));
        assert_eq!(issue_number_from_branch("feat/issues-9-cleanup"), Some(9));
        assert_eq!(issue_number_from_branch("42"), Some(42));
    }

    #[test]
    fn issue_number_from_branch_rejects_lookalikes() {
        assert_eq!(issue_number_from_branch("feature/b"), None);
        assert_eq!(issue_number_from_branch("fix-thing-123"), None);
        assert_eq!(issue_number_from_branch("v2-migration"), None);
        assert_eq!(issue_number_from_branch("2024q1-cleanup"), None);
        assert_eq!(issue_number_from_branch("0-zero"), None);
        assert_eq!(issue_number_from_branch("upgrade-issue"), None);
    }

    #[test]
    fn body_with_section_appends_to_existing_body() {
        let updated = body_with_section("Some PR description.\n", STACK_SECTION, "stack list");
        assert_eq!(
            updated,
            "Some PR description.\n\n<!-- git-stk:stack -->\nstack list\n<!-- /git-stk:stack -->"
        );
    }

    #[test]
    fn body_with_section_fills_empty_body() {
        let updated = body_with_section("", STACK_SECTION, "stack list");
        assert_eq!(
            updated,
            "<!-- git-stk:stack -->\nstack list\n<!-- /git-stk:stack -->"
        );
    }

    #[test]
    fn body_with_section_replaces_existing_section() {
        let body = "Intro.\n\n<!-- git-stk:stack -->\nold list\n<!-- /git-stk:stack -->\n\nOutro.";
        let updated = body_with_section(body, STACK_SECTION, "new list");
        assert_eq!(
            updated,
            "Intro.\n\nOutro.\n\n<!-- git-stk:stack -->\nnew list\n<!-- /git-stk:stack -->"
        );
    }

    #[test]
    fn body_with_section_is_idempotent() {
        let body = body_with_section("Description.", STACK_SECTION, "stack list");
        assert_eq!(body_with_section(&body, STACK_SECTION, "stack list"), body);
    }

    #[test]
    fn body_with_section_keeps_other_sections_intact() {
        let body = "Intro.\n\n<!-- git-stk:closes -->\nCloses #5\n<!-- /git-stk:closes -->";
        let updated = body_with_section(body, STACK_SECTION, "stack list");
        assert!(updated.contains("<!-- git-stk:closes -->\nCloses #5\n<!-- /git-stk:closes -->"));
        assert!(updated.ends_with("<!-- /git-stk:stack -->"));
    }

    #[test]
    fn body_with_section_repairs_orphaned_start_marker() {
        let body = "Intro.\n\n<!-- git-stk:stack -->\nleftover text";
        let updated = body_with_section(body, STACK_SECTION, "fresh list");
        assert_eq!(
            updated,
            "Intro.\n\nleftover text\n\n<!-- git-stk:stack -->\nfresh list\n<!-- /git-stk:stack -->"
        );
    }

    #[test]
    fn body_with_section_repairs_orphaned_end_marker() {
        let body = "Intro.\nstray\n<!-- /git-stk:stack -->\nOutro.";
        let updated = body_with_section(body, STACK_SECTION, "fresh list");
        assert!(updated.matches("<!-- git-stk:stack -->").count() == 1);
        assert!(updated.matches("<!-- /git-stk:stack -->").count() == 1);
        assert!(updated.contains("Intro.\nstray"));
        assert!(updated.ends_with("<!-- /git-stk:stack -->"));
    }

    #[test]
    fn body_with_section_repairs_reversed_and_duplicate_markers() {
        let body = "<!-- /git-stk:stack -->\nA\n<!-- git-stk:stack -->\nB\n\
                    <!-- git-stk:stack -->\nC\n<!-- /git-stk:stack -->\nD";
        let updated = body_with_section(body, STACK_SECTION, "fresh list");
        assert_eq!(updated.matches("<!-- git-stk:stack -->").count(), 1);
        assert_eq!(updated.matches("<!-- /git-stk:stack -->").count(), 1);
        assert!(updated.contains("fresh list"));
        assert!(updated.ends_with("<!-- /git-stk:stack -->"));
    }

    #[test]
    fn body_with_closes_note_appends_without_a_stack_section() {
        let updated = body_with_closes_note("Description.", "Closes #5");
        assert_eq!(
            updated,
            "Description.\n\n<!-- git-stk:closes -->\nCloses #5\n<!-- /git-stk:closes -->"
        );
    }

    #[test]
    fn body_with_closes_note_lands_above_the_stack_section() {
        let body = "Description.\n\n<!-- git-stk:stack -->\nstack list\n<!-- /git-stk:stack -->";
        let updated = body_with_closes_note(body, "Closes #5");
        assert_eq!(
            updated,
            "Description.\n\n\
             <!-- git-stk:closes -->\nCloses #5\n<!-- /git-stk:closes -->\n\n\
             <!-- git-stk:stack -->\nstack list\n<!-- /git-stk:stack -->"
        );
    }

    #[test]
    fn body_with_closes_note_replaces_a_stale_note_in_place() {
        let body = "Intro.\n\n<!-- git-stk:closes -->\nCloses #4\n<!-- /git-stk:closes -->\n\n\
                    <!-- git-stk:stack -->\nstack list\n<!-- /git-stk:stack -->";
        let updated = body_with_closes_note(body, "Closes #5");
        assert_eq!(updated.matches("<!-- git-stk:closes -->").count(), 1);
        assert!(updated.contains("Closes #5"));
        assert!(!updated.contains("Closes #4"));
        let closes = updated.find("Closes #5").expect("closes note");
        let stack = updated.find("stack list").expect("stack note");
        assert!(
            closes < stack,
            "closes note should sit above the stack note"
        );
    }

    #[test]
    fn body_with_description_note_lands_above_every_managed_section() {
        let body = "Intro.\n\n\
                    <!-- git-stk:closes -->\nCloses #5\n<!-- /git-stk:closes -->\n\n\
                    <!-- git-stk:stack -->\nstack list\n<!-- /git-stk:stack -->";
        let updated = body_with_description_note(body, "Summary.");

        let intro = updated.find("Intro.").expect("intro");
        let description = updated.find("Summary.").expect("description");
        let closes = updated.find("Closes #5").expect("closes");
        let stack = updated.find("stack list").expect("stack");
        assert!(intro < description && description < closes && closes < stack);
        assert!(
            updated
                .contains("<!-- git-stk:description -->\nSummary.\n<!-- /git-stk:description -->")
        );
    }

    #[test]
    fn body_with_description_note_replaces_in_place() {
        let body = "<!-- git-stk:description -->\nOld.\n<!-- /git-stk:description -->\n\n\
                    <!-- git-stk:stack -->\nstack list\n<!-- /git-stk:stack -->";
        let updated = body_with_description_note(body, "New.");
        assert_eq!(updated.matches("<!-- git-stk:description -->").count(), 1);
        assert!(updated.contains("New."));
        assert!(!updated.contains("Old."));
        let description = updated.find("New.").expect("description");
        let stack = updated.find("stack list").expect("stack");
        assert!(description < stack);
    }

    #[test]
    fn note_entry_round_trips_through_review() {
        let landed = entry("#11", "Landed", "https://example.com/11", "merged");
        let review = landed.to_review();
        assert_eq!(review.state, ReviewState::Merged);
        assert_eq!(NoteEntry::from_review(&review), landed);
    }

    #[test]
    fn note_entry_matches_by_id_or_url() {
        let by_id = entry("#11", "", "", "open");
        let by_url = entry("", "", "https://example.com/11", "open");
        assert!(by_id.matches(&entry("#11", "x", "y", "merged")));
        assert!(by_url.matches(&entry("#12", "", "https://example.com/11", "open")));
        assert!(!by_url.matches(&by_id));
    }
}
