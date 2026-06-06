mod common;

use common::TestRepo;

#[test]
fn status_prints_local_stack_and_review_state() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.git(["switch", "-c", "feature/a"]);
    repo.git(["switch", "-c", "feature/b"]);
    repo.git(["config", "branch.feature/b.stkParent", "feature/a"]);
    repo.git(["switch", "-c", "feature/c"]);
    repo.git(["config", "branch.feature/c.stkParent", "feature/b"]);
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
cat <<'JSON'
[{"number":13,"state":"OPEN","baseRefName":"feature/a","headRefName":"feature/b","url":"https://github.com/lararosekelley/git-stk/pull/13"}]
JSON
"##,
    );

    repo.stack()
        .args(["status", "feature/b"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains("branch: feature/b"))
        .stdout(predicates::str::contains("parent: feature/a"))
        .stdout(predicates::str::contains("children: feature/c"))
        .stdout(predicates::str::contains("provider: github (config)"))
        .stdout(predicates::str::contains(
            "review: #13 open feature/b -> feature/a",
        ))
        .stdout(predicates::str::contains(
            "url: https://github.com/lararosekelley/git-stk/pull/13",
        ));
}

#[test]
fn status_prints_none_when_review_is_missing() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.git(["config", "branch.feature/b.stkParent", "feature/a"]);
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
printf '[]\n'
"##,
    );

    repo.stack()
        .args(["status", "feature/b"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains("branch: feature/b"))
        .stdout(predicates::str::contains("parent: feature/a"))
        .stdout(predicates::str::contains("children: none"))
        .stdout(predicates::str::contains("review: none"));
}

#[test]
fn status_warns_when_review_base_differs_from_parent() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "gitlab"]);
    repo.git(["config", "branch.feature/b.stkParent", "feature/a"]);
    let path = repo.fake_cli(
        "glab",
        r##"#!/usr/bin/env sh
cat <<'JSON'
[{"iid":34,"state":"opened","target_branch":"main","source_branch":"feature/b","web_url":"https://gitlab.com/lararosekelley/git-stk-mirror/-/merge_requests/34"}]
JSON
"##,
    );

    repo.stack()
        .args(["status", "feature/b"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "review: !34 open feature/b -> main",
        ))
        .stdout(predicates::str::contains(
            "warning: review base is main, local parent is feature/a",
        ));
}

#[test]
fn review_reads_github_pr_for_current_branch() {
    let repo = TestRepo::new();
    repo.git([
        "remote",
        "add",
        "origin",
        "git@github.com:lararosekelley/git-stk",
    ]);
    repo.git(["switch", "-c", "feature/b"]);
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
cat <<'JSON'
[{"number":12,"state":"OPEN","baseRefName":"feature/a","headRefName":"feature/b","url":"https://github.com/lararosekelley/git-stk/pull/12"}]
JSON
"##,
    );

    repo.stack()
        .arg("review")
        .env("PATH", path)
        .assert()
        .success()
        .stdout(
            "#12 feature/b -> feature/a open https://github.com/lararosekelley/git-stk/pull/12\n",
        );
}

#[test]
fn review_reads_gitlab_mr_for_explicit_branch() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "gitlab"]);
    let path = repo.fake_cli(
        "glab",
        r##"#!/usr/bin/env sh
cat <<'JSON'
[{"iid":34,"state":"opened","target_branch":"feature/a","source_branch":"feature/b","web_url":"https://gitlab.com/lararosekelley/git-stk-mirror/-/merge_requests/34"}]
JSON
"##,
    );

    repo.stack()
        .args(["review", "feature/b"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout("!34 feature/b -> feature/a open https://gitlab.com/lararosekelley/git-stk-mirror/-/merge_requests/34\n");
}

#[test]
fn review_reports_when_no_review_exists() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
cat <<'JSON'
[]
JSON
"##,
    );

    repo.stack()
        .args(["review", "feature/b"])
        .env("PATH", path)
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "no github review found for feature/b",
        ));
}

#[test]
fn sync_sets_parent_from_github_pr_base() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.git(["switch", "-c", "feature/b"]);
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
cat <<'JSON'
[{"number":12,"state":"OPEN","baseRefName":"feature/a","headRefName":"feature/b","url":"https://github.com/lararosekelley/git-stk/pull/12"}]
JSON
"##,
    );

    repo.stack()
        .args(["sync", "feature/b"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "synced feature/b -> feature/a (#12)",
        ));

    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkParent"]),
        "feature/a"
    );
}

#[test]
fn sync_dry_run_reports_parent_without_writing_config() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.git(["switch", "-c", "feature/b"]);
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
cat <<'JSON'
[{"number":12,"state":"OPEN","baseRefName":"feature/a","headRefName":"feature/b","url":"https://github.com/lararosekelley/git-stk/pull/12"}]
JSON
"##,
    );

    repo.stack()
        .args(["sync", "--dry-run", "feature/b"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "would sync feature/b -> feature/a (#12)",
        ))
        .stdout(predicates::str::contains(
            "sync complete: 1 would be synced, 0 skipped",
        ));

    assert_eq!(
        repo.git_status(["config", "--get", "branch.feature/b.stkParent"])
            .status
            .code(),
        Some(1)
    );
}

#[test]
fn sync_sets_parent_from_gitlab_mr_target() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "gitlab"]);
    let path = repo.fake_cli(
        "glab",
        r##"#!/usr/bin/env sh
cat <<'JSON'
[{"iid":34,"state":"opened","target_branch":"feature/a","source_branch":"feature/b","web_url":"https://gitlab.com/lararosekelley/git-stk-mirror/-/merge_requests/34"}]
JSON
"##,
    );

    repo.stack()
        .args(["sync", "feature/b"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "synced feature/b -> feature/a (!34)",
        ));

    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkParent"]),
        "feature/a"
    );
}

#[test]
fn sync_all_local_branches_skips_branches_without_reviews() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.git(["switch", "-c", "feature/a"]);
    repo.git(["switch", "main"]);
    repo.git(["switch", "-c", "feature/b"]);
    repo.git(["switch", "main"]);
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  *feature/b*)
    cat <<'JSON'
[{"number":12,"state":"OPEN","baseRefName":"feature/a","headRefName":"feature/b","url":"https://github.com/lararosekelley/git-stk/pull/12"}]
JSON
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
    );

    repo.stack()
        .arg("sync")
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "synced feature/b -> feature/a (#12)",
        ))
        .stdout(predicates::str::contains("sync complete: 1 synced"));

    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkParent"]),
        "feature/a"
    );
}

#[test]
fn config_shows_defaults_and_branch_metadata() {
    let repo = TestRepo::new();

    repo.stack()
        .arg("config")
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "stk.provider (default: auto-detect from the remote URL)",
        ))
        .stdout(predicates::str::contains("stk.remote (default: origin)"))
        .stdout(predicates::str::contains("stk.updateRefs (default: false)"))
        .stdout(predicates::str::contains(
            "no branch metadata (no stacked branches)",
        ));

    repo.git(["config", "stk.pushOnRestack", "true"]);
    repo.stack().args(["new", "feature/a"]).assert().success();

    repo.stack()
        .arg("config")
        .assert()
        .success()
        .stdout(predicates::str::contains("stk.pushOnRestack = true"))
        .stdout(predicates::str::contains(
            "branch.feature/a.stkparent = main",
        ))
        .stdout(predicates::str::contains("branch.feature/a.stkbase = "));
}
