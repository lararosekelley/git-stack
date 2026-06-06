mod common;

use common::TestRepo;

#[test]
fn cleanup_retargets_children_and_detaches_merged_branch() {
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
[{"number":12,"state":"MERGED","baseRefName":"main","headRefName":"feature/a","url":"https://github.com/lararosekelley/git-stk/pull/12"}]
JSON
    ;;
  *feature/a*)
    printf '[]\n'
    ;;
  *feature/b*)
    cat <<'JSON'
[{"number":13,"state":"OPEN","baseRefName":"feature/a","headRefName":"feature/b","url":"https://github.com/lararosekelley/git-stk/pull/13"}]
JSON
    ;;
  pr\ edit*)
    printf 'updated child review\n'
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
    );

    repo.stack()
        .args(["cleanup", "feature/a"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains("will retarget feature/b -> main"))
        .stdout(predicates::str::contains(
            "will update review feature/b -> main (#13)",
        ))
        .stdout(predicates::str::contains("updated child review"))
        .stdout(predicates::str::contains("will detach feature/a"))
        .stdout(predicates::str::contains(
            "skipped feature/b: review #13 is open",
        ))
        .stdout(predicates::str::contains(
            "cleanup complete: 1 cleaned, 1 skipped",
        ));

    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkParent"]),
        "main"
    );
    assert_eq!(
        repo.git_status(["config", "--get", "branch.feature/a.stkParent"])
            .status
            .code(),
        Some(1)
    );
}

#[test]
fn cleanup_dry_run_leaves_stack_metadata_unchanged() {
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
[{"number":12,"state":"MERGED","baseRefName":"main","headRefName":"feature/a","url":"https://github.com/lararosekelley/git-stk/pull/12"}]
JSON
    ;;
  *feature/a*)
    printf '[]\n'
    ;;
  *feature/b*)
    cat <<'JSON'
[{"number":13,"state":"OPEN","baseRefName":"feature/a","headRefName":"feature/b","url":"https://github.com/lararosekelley/git-stk/pull/13"}]
JSON
    ;;
  pr\ edit*)
    echo "dry-run should not edit review" >&2
    exit 1
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
    );

    repo.stack()
        .args(["cleanup", "--dry-run", "feature/a"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "would retarget feature/b -> main",
        ))
        .stdout(predicates::str::contains(
            "would update review feature/b -> main (#13)",
        ))
        .stdout(predicates::str::contains("would detach feature/a"));

    assert_eq!(
        repo.git(["config", "--get", "branch.feature/a.stkParent"]),
        "main"
    );
    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkParent"]),
        "feature/a"
    );
}

#[test]
fn cleanup_delete_branch_removes_cleaned_merged_branch() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.stack().args(["new", "feature/a"]).assert().success();
    // Real commits + a squash merge: feature/a's commits are NOT
    // ancestry-merged into main afterwards, so `git branch -d` would refuse.
    // Deletion must trust the provider-verified merge state instead.
    repo.commit_file("a.txt", "one\n", "parent change one");
    repo.commit_file("a.txt", "one\ntwo\n", "parent change two");
    repo.stack().args(["new", "feature/b"]).assert().success();
    repo.git(["switch", "main"]);
    repo.git(["merge", "--squash", "feature/a"]);
    repo.git(["commit", "-m", "parent changes (#12)"]);
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  *feature/a\ --state\ merged*)
    cat <<'JSON'
[{"number":12,"state":"MERGED","baseRefName":"main","headRefName":"feature/a","url":"https://github.com/lararosekelley/git-stk/pull/12"}]
JSON
    ;;
  *feature/a*)
    printf '[]\n'
    ;;
  *feature/b*)
    cat <<'JSON'
[{"number":13,"state":"OPEN","baseRefName":"feature/a","headRefName":"feature/b","url":"https://github.com/lararosekelley/git-stk/pull/13"}]
JSON
    ;;
  pr\ edit*)
    printf 'updated child review\n'
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
    );

    repo.stack()
        .args(["cleanup", "--delete-branch", "feature/a"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains("will delete branch feature/a"))
        .stdout(predicates::str::contains(
            "cleanup complete: 1 cleaned, 1 skipped",
        ));

    assert!(
        !repo
            .git(["branch", "--list", "feature/a"])
            .contains("feature/a")
    );
    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkParent"]),
        "main"
    );
}

#[test]
fn cleanup_delete_branch_dry_run_keeps_branch() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.git(["switch", "main"]);
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  *--state\ merged*)
    cat <<'JSON'
[{"number":12,"state":"MERGED","baseRefName":"main","headRefName":"feature/a","url":"https://github.com/lararosekelley/git-stk/pull/12"}]
JSON
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
    );

    repo.stack()
        .args(["cleanup", "--dry-run", "--delete-branch", "feature/a"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains("would delete branch feature/a"));

    assert!(
        repo.git(["branch", "--list", "feature/a"])
            .contains("feature/a")
    );
    assert_eq!(
        repo.git(["config", "--get", "branch.feature/a.stkParent"]),
        "main"
    );
}

#[test]
fn cleanup_delete_branch_refuses_current_branch() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.stack().args(["new", "feature/a"]).assert().success();
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  *--state\ merged*)
    cat <<'JSON'
[{"number":12,"state":"MERGED","baseRefName":"main","headRefName":"feature/a","url":"https://github.com/lararosekelley/git-stk/pull/12"}]
JSON
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
    );

    repo.stack()
        .args(["cleanup", "--delete-branch", "feature/a"])
        .env("PATH", path)
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "refusing to delete currently checked out branch feature/a",
        ));

    assert!(
        repo.git(["branch", "--list", "feature/a"])
            .contains("feature/a")
    );
}
