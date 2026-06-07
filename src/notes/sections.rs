//! Marker-delimited sections in review bodies: `<!-- git-stk:NAME -->`
//! blocks that can be spliced, replaced in place, and self-repaired when
//! the markup is hand-edited away.

pub(crate) fn marker_start(name: &str) -> String {
    format!("<!-- git-stk:{name} -->")
}

pub(crate) fn marker_end(name: &str) -> String {
    format!("<!-- /git-stk:{name} -->")
}

/// The content of the first well-formed marker section, if any.
pub(crate) fn extract_section<'body>(body: &'body str, name: &str) -> Option<&'body str> {
    let start_marker = marker_start(name);
    let end_marker = marker_end(name);
    let start = body.find(&start_marker)? + start_marker.len();
    let length = body[start..].find(&end_marker)?;
    Some(&body[start..start + length])
}

/// Replace the marker-delimited section in a review body, appending it at
/// the end. Damaged markup (orphaned or reordered markers, duplicates) is
/// stripped first, so the section self-repairs on the next update.
pub(crate) fn body_with_section(body: &str, name: &str, content: &str) -> String {
    let section = format!("{}\n{content}\n{}", marker_start(name), marker_end(name));
    let cleaned = strip_sections(body, name);

    if cleaned.trim().is_empty() {
        section
    } else {
        format!("{}\n\n{section}", cleaned.trim_end())
    }
}

/// Replace the named section, keeping it above the first of the `before`
/// sections present in the body; without one, append at the end.
pub(crate) fn body_with_section_before(
    body: &str,
    name: &str,
    content: &str,
    before: &[&str],
) -> String {
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
pub(crate) fn strip_sections(body: &str, name: &str) -> String {
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

    #[test]
    fn body_with_section_appends_to_existing_body() {
        let updated = body_with_section("Some PR description.\n", "stack", "stack list");
        assert_eq!(
            updated,
            "Some PR description.\n\n<!-- git-stk:stack -->\nstack list\n<!-- /git-stk:stack -->"
        );
    }

    #[test]
    fn body_with_section_fills_empty_body() {
        let updated = body_with_section("", "stack", "stack list");
        assert_eq!(
            updated,
            "<!-- git-stk:stack -->\nstack list\n<!-- /git-stk:stack -->"
        );
    }

    #[test]
    fn body_with_section_replaces_existing_section() {
        let body = "Intro.\n\n<!-- git-stk:stack -->\nold list\n<!-- /git-stk:stack -->\n\nOutro.";
        let updated = body_with_section(body, "stack", "new list");
        assert_eq!(
            updated,
            "Intro.\n\nOutro.\n\n<!-- git-stk:stack -->\nnew list\n<!-- /git-stk:stack -->"
        );
    }

    #[test]
    fn body_with_section_is_idempotent() {
        let body = body_with_section("Description.", "stack", "stack list");
        assert_eq!(body_with_section(&body, "stack", "stack list"), body);
    }

    #[test]
    fn body_with_section_keeps_other_sections_intact() {
        let body = "Intro.\n\n<!-- git-stk:closes -->\nCloses #5\n<!-- /git-stk:closes -->";
        let updated = body_with_section(body, "stack", "stack list");
        assert!(updated.contains("<!-- git-stk:closes -->\nCloses #5\n<!-- /git-stk:closes -->"));
        assert!(updated.ends_with("<!-- /git-stk:stack -->"));
    }

    #[test]
    fn body_with_section_repairs_orphaned_start_marker() {
        let body = "Intro.\n\n<!-- git-stk:stack -->\nleftover text";
        let updated = body_with_section(body, "stack", "fresh list");
        assert_eq!(
            updated,
            "Intro.\n\nleftover text\n\n<!-- git-stk:stack -->\nfresh list\n<!-- /git-stk:stack -->"
        );
    }

    #[test]
    fn body_with_section_repairs_orphaned_end_marker() {
        let body = "Intro.\nstray\n<!-- /git-stk:stack -->\nOutro.";
        let updated = body_with_section(body, "stack", "fresh list");
        assert!(updated.matches("<!-- git-stk:stack -->").count() == 1);
        assert!(updated.matches("<!-- /git-stk:stack -->").count() == 1);
        assert!(updated.contains("Intro.\nstray"));
        assert!(updated.ends_with("<!-- /git-stk:stack -->"));
    }

    #[test]
    fn body_with_section_repairs_reversed_and_duplicate_markers() {
        let body = "<!-- /git-stk:stack -->\nA\n<!-- git-stk:stack -->\nB\n\
                    <!-- git-stk:stack -->\nC\n<!-- /git-stk:stack -->\nD";
        let updated = body_with_section(body, "stack", "fresh list");
        assert_eq!(updated.matches("<!-- git-stk:stack -->").count(), 1);
        assert_eq!(updated.matches("<!-- /git-stk:stack -->").count(), 1);
        assert!(updated.contains("fresh list"));
        assert!(updated.ends_with("<!-- /git-stk:stack -->"));
    }
}
