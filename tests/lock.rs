mod common;

use common::TestRepo;

/// Stand in for another git-stk process holding the operation lock.
fn hold_lock(repo: &TestRepo) {
    std::fs::write(repo.path().join(".git/stk-lock"), "99999 merge\n").expect("write lock");
}

#[test]
fn mutating_command_refuses_while_locked() {
    let repo = TestRepo::new();
    hold_lock(&repo);

    repo.stack()
        .args(["new", "feature/x"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "another git stk operation is in progress",
        ))
        // The holder line is surfaced so the message is actionable.
        .stderr(predicates::str::contains("99999 merge"));
}

#[test]
fn read_only_command_ignores_the_lock() {
    let repo = TestRepo::new();
    hold_lock(&repo);

    // Navigation/read-only commands are safe to run alongside anything.
    repo.stack().args(["list"]).assert().success();
}

#[test]
fn mutating_command_releases_the_lock_when_done() {
    let repo = TestRepo::new();

    repo.stack().args(["new", "feature/x"]).assert().success();

    assert!(
        !repo.path().join(".git/stk-lock").exists(),
        "the lock file should be gone once the command finishes"
    );
}
