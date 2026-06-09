mod common;

use common::{FakeProvider, TestRepo};

/// A two-branch stack (feature/a <- feature/b) with real commits.
fn two_branch_stack() -> TestRepo {
    let repo = TestRepo::new();
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "a work");
    repo.stack().args(["new", "feature/b"]).assert().success();
    repo.commit_file("b.txt", "b\n", "b work");
    repo
}

#[test]
fn run_executes_on_each_branch_bottom_up_and_restores_original() {
    let repo = two_branch_stack();
    repo.git(["switch", "feature/a"]);
    // The fake stands in for an arbitrary command: it logs every invocation
    // and succeeds.
    let fake = FakeProvider::new()
        .commands(&["probe"])
        .log_all("runs.log")
        .fallback("")
        .install(&repo);

    let output = repo
        .stack_faked(&fake)
        .args(["run", "--", "probe"])
        .output()
        .expect("run");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Branch headers print bottom-up.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let a = stdout.find("feature/a").expect("feature/a header");
    let b = stdout.find("feature/b").expect("feature/b header");
    assert!(a < b, "bottom-up order: feature/a before feature/b");

    // The command ran once per branch.
    let log = std::fs::read_to_string(repo.path().join("runs.log")).expect("runs log");
    assert_eq!(log.lines().count(), 2);

    // We are returned to where we started.
    assert_eq!(repo.git(["branch", "--show-current"]), "feature/a");
}

#[test]
fn run_reports_failures_and_exits_nonzero() {
    let repo = two_branch_stack();
    let fake = FakeProvider::new()
        .commands(&["probe"])
        .log_all("runs.log")
        .fallback_fail("boom")
        .install(&repo);

    repo.stack_faked(&fake)
        .args(["run", "--", "probe"])
        .assert()
        .failure()
        .stdout(predicates::str::contains("FAIL"));

    // Without --fail-fast every branch is still attempted.
    let log = std::fs::read_to_string(repo.path().join("runs.log")).expect("runs log");
    assert_eq!(log.lines().count(), 2);
}

#[test]
fn run_fail_fast_stops_at_the_first_failure() {
    let repo = two_branch_stack();
    let fake = FakeProvider::new()
        .commands(&["probe"])
        .log_all("runs.log")
        .fallback_fail("boom")
        .install(&repo);

    repo.stack_faked(&fake)
        .args(["run", "--fail-fast", "--", "probe"])
        .assert()
        .failure();

    let log = std::fs::read_to_string(repo.path().join("runs.log")).expect("runs log");
    assert_eq!(
        log.lines().count(),
        1,
        "stopped after the first failing branch"
    );
}

#[test]
fn run_refuses_a_dirty_worktree() {
    let repo = two_branch_stack();
    // a.txt is tracked (committed on feature/a); dirty it.
    repo.write("a.txt", "uncommitted\n");
    let fake = FakeProvider::new()
        .commands(&["probe"])
        .fallback("")
        .install(&repo);

    repo.stack_faked(&fake)
        .args(["run", "--", "probe"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("uncommitted changes"));
}

#[test]
fn run_without_a_stack_reports_nothing_to_do() {
    let repo = TestRepo::new();
    let fake = FakeProvider::new()
        .commands(&["probe"])
        .fallback("")
        .install(&repo);

    repo.stack_faked(&fake)
        .args(["run", "--", "probe"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("no stacked branches to run on"));
}
