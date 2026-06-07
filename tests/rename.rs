mod common;

use common::TestRepo;

#[test]
fn rename_current_branch_retargets_children() {
    let repo = TestRepo::new();
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.stack().args(["new", "feature/b"]).assert().success();
    repo.git(["switch", "feature/a"]);

    repo.stack()
        .args(["rename", "feature/a2"])
        .assert()
        .success()
        .stdout(predicates::str::contains("renamed feature/a -> feature/a2"))
        .stdout(predicates::str::contains(
            "retargeted feature/b -> feature/a2",
        ));

    assert_eq!(repo.git(["branch", "--show-current"]), "feature/a2");
    // The branch's own metadata moved with the rename; the child follows.
    assert_eq!(
        repo.git(["config", "--get", "branch.feature/a2.stkParent"]),
        "main"
    );
    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkParent"]),
        "feature/a2"
    );
}

#[test]
fn rename_named_branch_with_two_names() {
    let repo = TestRepo::new();
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.stack().args(["new", "feature/b"]).assert().success();
    repo.git(["switch", "main"]);

    repo.stack()
        .args(["rename", "feature/a", "feature/a2"])
        .assert()
        .success()
        .stdout(predicates::str::contains("renamed feature/a -> feature/a2"));

    assert_eq!(repo.git(["branch", "--show-current"]), "main");
    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkParent"]),
        "feature/a2"
    );
}

#[test]
fn rename_retargets_every_direct_child() {
    let repo = TestRepo::new();
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.stack().args(["new", "feature/b"]).assert().success();
    repo.git(["switch", "feature/a"]);
    repo.stack().args(["new", "feature/c"]).assert().success();

    repo.stack()
        .args(["rename", "feature/a", "feature/a2"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "retargeted feature/b -> feature/a2",
        ))
        .stdout(predicates::str::contains(
            "retargeted feature/c -> feature/a2",
        ));

    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkParent"]),
        "feature/a2"
    );
    assert_eq!(
        repo.git(["config", "--get", "branch.feature/c.stkParent"]),
        "feature/a2"
    );
}

#[test]
fn rename_warns_when_a_review_heads_the_old_name() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.stack().args(["new", "feature/a"]).assert().success();
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  *feature/a*)
    cat <<'JSON'
[{"number":12,"state":"OPEN","baseRefName":"main","headRefName":"feature/a","url":"https://github.com/owner/repo/pull/12"}]
JSON
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
    );

    repo.stack()
        .args(["rename", "feature/a2"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "warning: review #12 still heads feature/a; \
             submitting feature/a2 will open a new review",
        ));
}

#[test]
fn rename_dry_run_previews_without_renaming() {
    let repo = TestRepo::new();
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.stack().args(["new", "feature/b"]).assert().success();

    repo.stack()
        .args(["rename", "--dry-run", "feature/a", "feature/a2"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "would rename feature/a -> feature/a2",
        ))
        .stdout(predicates::str::contains(
            "would retarget feature/b -> feature/a2",
        ));

    assert!(
        repo.git(["branch", "--list", "feature/a"])
            .contains("feature/a")
    );
    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkParent"]),
        "feature/a"
    );
}

#[test]
fn rename_fails_when_target_exists_and_changes_nothing() {
    let repo = TestRepo::new();
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.stack().args(["new", "feature/b"]).assert().success();

    repo.stack()
        .args(["rename", "feature/a", "main"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("failed to rename feature/a"));

    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkParent"]),
        "feature/a"
    );
}
