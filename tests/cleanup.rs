mod common;

use common::TestRepo;
use predicates::prelude::PredicateBooleanExt;

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
fn cleanup_recovers_base_when_merged_parent_branch_is_gone() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "a work");
    repo.stack().args(["new", "feature/b"]).assert().success();
    repo.commit_file("b.txt", "b\n", "b work");
    repo.git(["switch", "main"]);
    // The merged parent was deleted out-of-band: feature/b now points at a
    // branch that no longer exists, and only the review remembers its base.
    repo.git(["branch", "-D", "feature/a"]);
    repo.git(["config", "branch.feature/b.stkParent", "feature/a"]);

    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  *feature/a\ --state\ merged*)
    cat <<'JSON'
[{"number":12,"state":"MERGED","baseRefName":"main","headRefName":"feature/a","url":"https://github.com/owner/repo/pull/12"}]
JSON
    ;;
  *feature/a*)
    printf '[]\n'
    ;;
  pr\ edit\ 13\ --base*)
    printf '%s\n' "$*" > edit-base-13.txt
    ;;
  *feature/b*)
    cat <<'JSON'
[{"number":13,"state":"OPEN","baseRefName":"feature/a","headRefName":"feature/b","url":"https://github.com/owner/repo/pull/13"}]
JSON
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
    );

    // Dry run announces the retarget without writing anything.
    repo.stack()
        .args(["cleanup", "--dry-run", "feature/b"])
        .env("PATH", path.clone())
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "would retarget feature/b -> main",
        ));
    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkParent"]),
        "feature/a"
    );

    repo.stack()
        .args(["cleanup", "feature/b"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "feature/b: parent feature/a is gone, but review #12 merged into main",
        ))
        .stdout(predicates::str::contains("will retarget feature/b -> main"))
        .stdout(predicates::str::contains(
            "will update review feature/b -> main (#13)",
        ))
        .stdout(predicates::str::contains(
            "cleanup complete: 0 cleaned, 1 skipped, 1 retargeted",
        ));

    let recorded =
        std::fs::read_to_string(repo.path().join("edit-base-13.txt")).expect("edit base args");
    assert_eq!(recorded.trim(), "pr edit 13 --base main");
    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkParent"]),
        "main"
    );
}

#[test]
fn cleanup_leaves_a_gone_parent_alone_without_a_merged_review() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.git(["switch", "-c", "feature/b"]);
    repo.git(["config", "branch.feature/b.stkParent", "feature/a"]);

    // No review for the missing parent: recovery must defer to repair.
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
printf '[]\n'
"##,
    );

    repo.stack()
        .args(["cleanup", "feature/b"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains("retarget").not());

    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkParent"]),
        "feature/a"
    );
}

#[test]
fn cleanup_skips_closed_reviews_with_their_state() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.git(["switch", "main"]);
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  *--state\ closed*)
    cat <<'JSON'
[{"number":12,"state":"CLOSED","baseRefName":"main","headRefName":"feature/a","url":"https://github.com/lararosekelley/git-stk/pull/12"}]
JSON
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
        .stdout(predicates::str::contains(
            "skipped feature/a: review #12 is closed",
        ))
        .stdout(predicates::str::contains(
            "cleanup complete: 0 cleaned, 1 skipped",
        ));

    // Closed work is not in the trunk; the branch must survive.
    assert!(
        repo.git(["branch", "--list", "feature/a"])
            .contains("feature/a")
    );
}

#[test]
fn cleanup_deletes_cleaned_merged_branch_by_default() {
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
        .args(["cleanup", "feature/a"])
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
fn cleanup_dry_run_keeps_branch() {
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
        .args(["cleanup", "--dry-run", "feature/a"])
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
fn cleanup_keeps_the_checked_out_branch() {
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
        .args(["cleanup", "feature/a"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "kept feature/a: cannot delete the checked out branch",
        ))
        .stdout(predicates::str::contains(
            "cleanup complete: 1 cleaned, 0 skipped",
        ));

    assert!(
        repo.git(["branch", "--list", "feature/a"])
            .contains("feature/a")
    );
}

#[test]
fn cleanup_keep_branch_keeps_cleaned_merged_branch() {
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
        .args(["cleanup", "--keep-branch", "feature/a"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains("will detach feature/a"))
        .stdout(predicates::str::contains("delete").not());

    assert!(
        repo.git(["branch", "--list", "feature/a"])
            .contains("feature/a")
    );
    assert_eq!(
        repo.git_status(["config", "--get", "branch.feature/a.stkParent"])
            .status
            .code(),
        Some(1)
    );
}
