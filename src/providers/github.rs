use anyhow::Result;

use super::json::{
    first_json_item, optional_string, parse_body_field, parse_state, required_string,
};
use super::{ReviewProvider, ReviewRequest, command_output};

pub(super) struct GitHubProvider;

impl ReviewProvider for GitHubProvider {
    fn review_for_branch(&self, branch: &str) -> Result<Option<ReviewRequest>> {
        // gh pr list only returns open pull requests by default; check merged
        // ones too so cleanup can see landed reviews.
        if let Some(review) = list_review(branch, None)? {
            return Ok(Some(review));
        }
        list_review(branch, Some("merged"))
    }

    fn review_for_branch_including_closed(&self, branch: &str) -> Result<Option<ReviewRequest>> {
        // Open and merged take precedence: a branch resubmitted after its
        // review was closed should resolve to the fresh review.
        if let Some(review) = self.review_for_branch(branch)? {
            return Ok(Some(review));
        }
        list_review(branch, Some("closed"))
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

    fn merge_review(&self, review: &ReviewRequest, strategy: &str) -> Result<String> {
        let flag = match strategy {
            "rebase" => "--rebase",
            "merge" => "--merge",
            _ => "--squash",
        };
        command_output("gh", &["pr", "merge", review.id_value(), flag])
    }
}

fn list_review(branch: &str, state: Option<&str>) -> Result<Option<ReviewRequest>> {
    let mut args = vec!["pr", "list", "--head", branch];
    if let Some(state) = state {
        args.extend(["--state", state]);
    }
    args.extend(["--json", "number,state,baseRefName,headRefName,url,title"]);

    let output = command_output("gh", &args)?;
    parse_github_review(&output)
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
        title: optional_string(&review, "title"),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{ReviewRequest, ReviewState};

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
                title: String::new(),
            }
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
    fn parse_review_empty_array_returns_none() {
        assert_eq!(parse_github_review("[]").expect("parse review"), None);
    }
}
