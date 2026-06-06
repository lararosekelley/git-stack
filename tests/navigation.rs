mod common;

use common::TestRepo;

#[test]
fn new_records_parent_and_supports_navigation() {
    let repo = TestRepo::new();

    repo.stack()
        .args(["new", "feature/a"])
        .assert()
        .success()
        .stdout("created feature/a with parent main\n");

    assert_eq!(repo.git(["branch", "--show-current"]), "feature/a");
    assert_eq!(
        repo.git(["config", "--get", "branch.feature/a.stkParent"]),
        "main"
    );

    repo.stack()
        .arg("parent")
        .assert()
        .success()
        .stdout("main\n");

    repo.stack()
        .args(["children", "main"])
        .assert()
        .success()
        .stdout("feature/a\n");

    repo.stack().arg("down").assert().success();
    assert_eq!(repo.git(["branch", "--show-current"]), "main");

    repo.stack().arg("up").assert().success();
    assert_eq!(repo.git(["branch", "--show-current"]), "feature/a");
}

#[test]
fn adopt_list_and_detach_manage_existing_branches() {
    let repo = TestRepo::new();

    repo.git(["switch", "-c", "feature/a"]);
    repo.git(["switch", "main"]);
    repo.git(["switch", "-c", "feature/b"]);
    repo.git(["switch", "main"]);

    repo.stack()
        .args(["adopt", "feature/a", "--parent", "main"])
        .assert()
        .success()
        .stdout("attached feature/a to main\n");

    repo.stack()
        .args(["adopt", "feature/b", "--parent", "feature/a"])
        .assert()
        .success()
        .stdout("attached feature/b to feature/a\n");

    repo.stack()
        .arg("list")
        .assert()
        .success()
        .stdout("    feature/b\n  feature/a\nmain (trunk) *\n");

    repo.stack()
        .args(["detach", "feature/b"])
        .assert()
        .success()
        .stdout("detached feature/b\n");

    repo.stack()
        .args(["children", "feature/a"])
        .assert()
        .success()
        .stdout("");
}

#[test]
fn up_requires_branch_when_multiple_children_exist() {
    let repo = TestRepo::new();

    repo.git(["switch", "-c", "feature/a"]);
    repo.git(["switch", "main"]);
    repo.git(["switch", "-c", "feature/b"]);
    repo.git(["switch", "main"]);

    repo.stack()
        .args(["adopt", "feature/a", "--parent", "main"])
        .assert()
        .success();
    repo.stack()
        .args(["adopt", "feature/b", "--parent", "main"])
        .assert()
        .success();

    repo.stack()
        .arg("up")
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "choose one with `git stk up <branch>`",
        ));

    repo.stack().args(["up", "feature/b"]).assert().success();
    assert_eq!(repo.git(["branch", "--show-current"]), "feature/b");
}

#[test]
fn new_and_restack_record_stack_base() {
    let repo = TestRepo::new();

    let main_tip = repo.git(["rev-parse", "main"]);
    repo.stack().args(["new", "feature/a"]).assert().success();
    assert_eq!(
        repo.git(["config", "--get", "branch.feature/a.stkBase"]),
        main_tip
    );

    repo.commit_file("a.txt", "a\n", "feature work");
    repo.git(["switch", "main"]);
    repo.commit_file("main.txt", "main\n", "main moves on");

    repo.git(["switch", "feature/a"]);
    repo.stack().arg("restack").assert().success();

    let new_main_tip = repo.git(["rev-parse", "main"]);
    assert_eq!(
        repo.git(["config", "--get", "branch.feature/a.stkBase"]),
        new_main_tip
    );
}

#[test]
fn detach_clears_stack_base() {
    let repo = TestRepo::new();

    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.stack().args(["detach"]).assert().success();

    assert_eq!(
        repo.git_status(["config", "--get", "branch.feature/a.stkBase"])
            .status
            .code(),
        Some(1)
    );
}

#[test]
fn list_prints_leaf_first_without_trunk_label_for_fragments() {
    let repo = TestRepo::new();

    // A stack fragment not rooted at the trunk gets no (trunk) label.
    repo.git(["switch", "-c", "feature/x"]);
    repo.git(["switch", "-c", "feature/y"]);
    repo.stack()
        .args(["adopt", "feature/y", "--parent", "feature/x"])
        .assert()
        .success();

    repo.stack()
        .arg("list")
        .assert()
        .success()
        .stdout("  feature/y *\nfeature/x\n");
}
