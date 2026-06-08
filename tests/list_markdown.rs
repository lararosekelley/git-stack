// These suites drive sh-script provider fakes, so they are Unix-only.
#![cfg(unix)]

mod common;

use common::TestRepo;

#[test]
fn list_markdown_prints_summary_and_ordered_pr_list() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.stack().args(["new", "feature/b"]).assert().success();
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  *feature/a\ --state\ merged*)
    cat <<'JSON'
[{"number":9,"state":"MERGED","baseRefName":"main","headRefName":"feature/a","url":"https://github.com/owner/repo/pull/9","title":"Bottom change"}]
JSON
    ;;
  *feature/a*)
    printf '[]\n'
    ;;
  *feature/b*)
    cat <<'JSON'
[{"number":10,"state":"OPEN","baseRefName":"feature/a","headRefName":"feature/b","url":"https://github.com/owner/repo/pull/10","title":"Top change"}]
JSON
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
    );

    // Run from the BOTTOM branch: the root walk must find the whole stack.
    repo.git(["switch", "feature/a"]);
    repo.stack()
        .args(["list", "--format", "markdown"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "2 PRs, base `main`, 1 open / 1 merged",
        ))
        .stdout(predicates::str::contains(
            "1. [Bottom change (#9)](https://github.com/owner/repo/pull/9) - merged",
        ))
        .stdout(predicates::str::contains(
            "2. [Top change (#10)](https://github.com/owner/repo/pull/10) - open",
        ));
}

#[test]
fn list_markdown_degrades_to_branch_names_without_provider() {
    let repo = TestRepo::new();
    // No remote, no provider config: lookups are impossible.
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.stack().args(["new", "feature/b"]).assert().success();

    repo.stack()
        .args(["list", "--format", "markdown"])
        .assert()
        .success()
        .stdout(predicates::str::contains("2 branches, base `main`"))
        .stdout(predicates::str::contains("1. `feature/a` (no review)"))
        .stdout(predicates::str::contains("2. `feature/b` (no review)"));
}

#[test]
fn list_markdown_reports_empty_stack() {
    let repo = TestRepo::new();

    repo.stack()
        .args(["list", "--format", "markdown"])
        .assert()
        .success()
        .stdout(predicates::str::contains("no stacked branches"));
}
