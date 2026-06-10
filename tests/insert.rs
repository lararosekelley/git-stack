mod common;

use common::TestRepo;
use predicates::prelude::PredicateBooleanExt;

/// A linear stack main <- feature/a <- feature/b with real commits.
fn linear_stack() -> TestRepo {
    let repo = TestRepo::new();
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "a work");
    repo.stack().args(["new", "feature/b"]).assert().success();
    repo.commit_file("b.txt", "b\n", "b work");
    repo
}

#[test]
fn new_insert_splices_above_current_and_retargets_children() {
    let repo = linear_stack();
    repo.git(["switch", "feature/a"]);

    repo.stack()
        .args(["new", "feature/mid", "--insert"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "inserted feature/mid above feature/a",
        ))
        .stdout(predicates::str::contains(
            "retargeted feature/b -> feature/mid",
        ));

    // main <- feature/a <- feature/mid <- feature/b
    assert_eq!(repo.git(["branch", "--show-current"]), "feature/mid");
    assert_eq!(
        repo.git(["config", "--get", "branch.feature/mid.stkParent"]),
        "feature/a"
    );
    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkParent"]),
        "feature/mid"
    );
    // The inserted branch shares feature/a's tip, so feature/b stays based on
    // the same commit it always was.
    assert_eq!(
        repo.git(["rev-parse", "feature/mid"]),
        repo.git(["rev-parse", "feature/a"])
    );
    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkBase"]),
        repo.git(["rev-parse", "feature/a"])
    );
}

#[test]
fn new_prepend_splices_below_current() {
    let repo = linear_stack();
    repo.git(["switch", "feature/b"]);

    repo.stack()
        .args(["new", "feature/mid", "--prepend"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "inserted feature/mid between feature/a and feature/b",
        ))
        .stdout(predicates::str::contains(
            "retargeted feature/b -> feature/mid",
        ));

    // main <- feature/a <- feature/mid <- feature/b
    assert_eq!(repo.git(["branch", "--show-current"]), "feature/mid");
    assert_eq!(
        repo.git(["config", "--get", "branch.feature/mid.stkParent"]),
        "feature/a"
    );
    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkParent"]),
        "feature/mid"
    );
    assert_eq!(
        repo.git(["rev-parse", "feature/mid"]),
        repo.git(["rev-parse", "feature/a"])
    );
}

#[test]
fn new_insert_on_a_leaf_just_extends_the_stack() {
    let repo = linear_stack(); // standing on the leaf, feature/b

    repo.stack()
        .args(["new", "feature/top", "--insert"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "inserted feature/top above feature/b",
        ))
        // No children to move.
        .stdout(predicates::str::contains("retargeted").not());

    assert_eq!(repo.git(["branch", "--show-current"]), "feature/top");
    assert_eq!(
        repo.git(["config", "--get", "branch.feature/top.stkParent"]),
        "feature/b"
    );
}

#[test]
fn new_insert_and_prepend_conflict() {
    let repo = linear_stack();

    repo.stack()
        .args(["new", "feature/mid", "--insert", "--prepend"])
        .assert()
        .failure();
}

#[test]
fn new_prepend_without_a_parent_fails() {
    let repo = linear_stack();
    repo.git(["switch", "main"]);

    repo.stack()
        .args(["new", "feature/mid", "--prepend"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "no stack parent to prepend below",
        ));
}

#[test]
fn new_prepend_refuses_a_dirty_worktree() {
    let repo = linear_stack();
    repo.git(["switch", "feature/b"]);
    repo.write("a.txt", "uncommitted\n");

    repo.stack()
        .args(["new", "feature/mid", "--prepend"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("uncommitted changes"));
}
