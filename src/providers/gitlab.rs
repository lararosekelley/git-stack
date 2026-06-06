use anyhow::Result;

use super::json::{
    first_json_item, optional_string, parse_body_field, parse_state, required_string,
};
use super::{ReviewProvider, ReviewRequest, command_output};

pub(super) struct GitLabProvider;

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

    fn merge_review(&self, review: &ReviewRequest, strategy: &str) -> Result<String> {
        let mut args = vec!["mr", "merge", review.id_value()];
        match strategy {
            "rebase" => args.push("--rebase"),
            "merge" => {}
            _ => args.push("--squash"),
        }
        command_output("glab", &args)
    }
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
        title: optional_string(&review, "title"),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{ReviewRequest, ReviewState};

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
                title: String::new(),
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
    fn parse_gitlab_review_empty_array_returns_none() {
        assert_eq!(parse_gitlab_review("[]").expect("parse review"), None);
    }
}
