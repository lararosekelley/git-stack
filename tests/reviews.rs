use std::fs;

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
    repo.git(["switch", "-c", "feature/a"]);
    repo.git(["switch", "-c", "feature/b"]);
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
    repo.git(["switch", "-c", "feature/a"]);
    repo.git(["switch", "-c", "feature/b"]);
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
        .args(["sync", "--dry-run"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "would sync feature/b -> feature/a (#12)",
        ))
        .stdout(predicates::str::contains(
            "would restack the remaining stack",
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
    repo.git(["switch", "-c", "feature/a"]);
    repo.git(["switch", "-c", "feature/b"]);
    let path = repo.fake_cli(
        "glab",
        r##"#!/usr/bin/env sh
case "$*" in
  *feature/b*)
    cat <<'JSON'
[{"iid":34,"state":"opened","target_branch":"feature/a","source_branch":"feature/b","web_url":"https://gitlab.com/lararosekelley/git-stk-mirror/-/merge_requests/34"}]
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
            "synced feature/b -> feature/a (!34)",
        ));

    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkParent"]),
        "feature/a"
    );
}

#[test]
fn sync_skips_stack_branches_without_reviews() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.stack().args(["new", "feature/b"]).assert().success();
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
printf '[]\n'
"##,
    );

    repo.stack()
        .arg("sync")
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "skipped feature/a: no github review found",
        ))
        .stdout(predicates::str::contains(
            "skipped feature/b: no github review found",
        ))
        .stdout(predicates::str::contains(
            "sync complete: 0 synced, 2 skipped",
        ));
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

#[test]
fn sync_advances_the_merge_loop_end_to_end() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.git(["config", "stk.pushOnRestack", "true"]);

    // Stack: main -> feature/a -> feature/b, with real commits.
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "a work");
    repo.stack().args(["new", "feature/b"]).assert().success();
    repo.commit_file("b.txt", "b\n", "b work");

    let bare = repo.add_bare_origin(&["main", "feature/a", "feature/b"]);

    // Simulate GitHub squash-merging feature/a: advance ORIGIN's main, then
    // rewind local main so sync has something to fetch.
    repo.git(["switch", "main"]);
    repo.git(["merge", "--squash", "feature/a"]);
    repo.git(["commit", "-m", "a work (#12)"]);
    repo.git(["push", "origin", "main"]);
    repo.git(["reset", "--hard", "HEAD~1"]);

    // Stand on the MERGED branch: sync must move us off it.
    repo.git(["switch", "feature/a"]);

    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  pr\ view\ 12*)
    printf '{"body":""}\n'
    ;;
  pr\ view\ 13*)
    printf '{"body":""}\n'
    ;;
  pr\ edit\ 12\ --body*)
    printf '%s\n' "$*" > edit-body-12.txt
    ;;
  pr\ edit\ 13\ --body*)
    printf '%s\n' "$*" > edit-body-13.txt
    ;;
  *feature/a\ --state\ merged*)
    cat <<'JSON'
[{"number":12,"state":"MERGED","baseRefName":"main","headRefName":"feature/a","url":"https://github.com/owner/repo/pull/12","title":"A work"}]
JSON
    ;;
  *feature/a*)
    printf '[]\n'
    ;;
  *feature/b*)
    cat <<'JSON'
[{"number":13,"state":"OPEN","baseRefName":"feature/a","headRefName":"feature/b","url":"https://github.com/owner/repo/pull/13","title":"B work"}]
JSON
    ;;
  pr\ edit*)
    printf 'updated review\n'
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
        .stdout(predicates::str::contains("feature/a: review #12 is merged"))
        .stdout(predicates::str::contains("updated stack note in #12"))
        .stdout(predicates::str::contains("updated stack note in #13"))
        .stdout(predicates::str::contains(
            "next up: feature/b -> #13 https://github.com/owner/repo/pull/13",
        ));

    // The overview was refreshed mid-loop: the merged entry is restyled in
    // the surviving review (and in its own), not dropped.
    let survivor = fs::read_to_string(repo.path().join("edit-body-13.txt")).expect("survivor");
    assert!(
        survivor.contains(
            "- \u{1F7E2} [B work (#13)](https://github.com/owner/repo/pull/13) \u{1F448}"
        )
    );
    assert!(survivor.contains(
        "- \u{1F7E3} ~~[A work (#12)](https://github.com/owner/repo/pull/12)~~ (merged)"
    ));
    let merged_body = fs::read_to_string(repo.path().join("edit-body-12.txt")).expect("merged");
    assert!(merged_body.contains(
        "- \u{1F7E3} ~~[A work (#12)](https://github.com/owner/repo/pull/12)~~ (merged) \u{1F448}"
    ));

    // Local main was fetched forward to the squash commit.
    assert_eq!(
        repo.git(["rev-parse", "main"]),
        repo.remote_sha(&bare, "main")
    );
    // The merged branch is gone; we were moved to the survivor.
    assert_eq!(repo.git(["branch", "--show-current"]), "feature/b");
    assert_eq!(
        repo.git_status(["branch", "--list", "feature/a"])
            .stdout
            .len(),
        0
    );
    // feature/b was retargeted, restacked onto fetched main, and pushed.
    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkParent"]),
        "main"
    );
    assert_eq!(
        repo.git(["merge-base", "main", "feature/b"]),
        repo.git(["rev-parse", "main"])
    );
    assert_eq!(
        repo.remote_sha(&bare, "feature/b"),
        repo.git(["rev-parse", "feature/b"])
    );
}

#[test]
fn sync_styles_closed_reviews_in_the_stack_overview() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.stack().args(["new", "feature/b"]).assert().success();

    // feature/b's review was closed on the platform: invisible to the sync
    // classification, but the overview must show it red rather than drop it.
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  pr\ view\ 12*)
    printf '{"body":""}\n'
    ;;
  pr\ view\ 13*)
    printf '{"body":""}\n'
    ;;
  pr\ edit\ 12\ --body*)
    printf '%s\n' "$*" > edit-body-12.txt
    ;;
  pr\ edit\ 13\ --body*)
    printf '%s\n' "$*" > edit-body-13.txt
    ;;
  *feature/b\ --state\ closed*)
    cat <<'JSON'
[{"number":13,"state":"CLOSED","baseRefName":"feature/a","headRefName":"feature/b","url":"https://github.com/owner/repo/pull/13","title":"B work"}]
JSON
    ;;
  *feature/b*)
    printf '[]\n'
    ;;
  *feature/a*)
    cat <<'JSON'
[{"number":12,"state":"OPEN","baseRefName":"main","headRefName":"feature/a","url":"https://github.com/owner/repo/pull/12","title":"A work"}]
JSON
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
    );

    repo.git(["switch", "feature/a"]);
    repo.stack()
        .arg("sync")
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "skipped feature/b: no github review found",
        ))
        .stdout(predicates::str::contains("updated stack note in #12"))
        .stdout(predicates::str::contains("updated stack note in #13"));

    let bottom = fs::read_to_string(repo.path().join("edit-body-12.txt")).expect("bottom body");
    assert!(bottom.contains(
        "- \u{1F534} ~~[B work (#13)](https://github.com/owner/repo/pull/13)~~ (closed)"
    ));
    assert!(
        bottom.contains(
            "- \u{1F7E2} [A work (#12)](https://github.com/owner/repo/pull/12) \u{1F448}"
        )
    );
}

#[test]
fn sync_reports_stack_complete_when_everything_merged() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);

    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "a work");

    let _bare = repo.add_bare_origin(&["main", "feature/a"]);
    repo.git(["switch", "main"]);
    repo.git(["merge", "--squash", "feature/a"]);
    repo.git(["commit", "-m", "a work (#12)"]);
    repo.git(["push", "origin", "main"]);
    repo.git(["switch", "feature/a"]);

    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  *--state\ merged*)
    cat <<'JSON'
[{"number":12,"state":"MERGED","baseRefName":"main","headRefName":"feature/a","url":"https://github.com/owner/repo/pull/12","title":"A work"}]
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
            "stack complete: everything merged into main",
        ));

    assert_eq!(repo.git(["branch", "--show-current"]), "main");
}
