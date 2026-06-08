mod common;

use common::TestRepo;
use predicates::prelude::PredicateBooleanExt;

#[test]
fn demo_provider_runs_the_full_merge_loop_offline() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "demo"]);

    repo.stack()
        .args(["new", "feature/login"])
        .assert()
        .success();
    repo.commit_file("login.txt", "login form\n", "add login form");
    repo.stack()
        .args(["new", "feature/avatar"])
        .assert()
        .success();
    repo.commit_file("avatar.txt", "avatars\n", "add avatars");

    // Submit opens demo reviews and writes the stack overview into them.
    repo.stack()
        .args(["submit", "--stack"])
        .assert()
        .success()
        .stdout(predicates::str::contains("created feature/login -> main"))
        .stdout(predicates::str::contains(
            "created feature/avatar -> feature/login",
        ))
        .stdout(predicates::str::contains("updated stack note in #1"))
        .stdout(predicates::str::contains("updated stack note in #2"));

    repo.stack()
        .args(["status", "feature/login"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "review: #1 open feature/login -> main",
        ))
        .stdout(predicates::str::contains("url: demo://review/1"));

    // The whole landing, no network: real squashes onto main, real cleanup.
    repo.stack()
        .args(["merge", "--all", "-y"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "squashed feature/login into main",
        ))
        .stdout(predicates::str::contains("merged add login form (#1)"))
        .stdout(predicates::str::contains(
            "squashed feature/avatar into main",
        ))
        .stdout(predicates::str::contains("merged add avatars (#2)"))
        .stdout(predicates::str::contains(
            "stack complete: everything merged into main",
        ))
        .stdout(predicates::str::contains(
            "merge complete: 2 of 2 reviews merged",
        ));

    // The squashed work is genuinely on main.
    assert_eq!(repo.git(["branch", "--show-current"]), "main");
    assert_eq!(repo.git(["show", "main:login.txt"]), "login form");
    assert_eq!(repo.git(["show", "main:avatar.txt"]), "avatars");
    assert_eq!(
        repo.git_status(["branch", "--list", "feature/login", "feature/avatar"])
            .stdout
            .len(),
        0
    );
}

#[test]
fn demo_provider_is_never_auto_detected() {
    let repo = TestRepo::new();
    repo.git(["remote", "add", "origin", "git@github.com:owner/repo.git"]);

    repo.stack()
        .arg("provider")
        .assert()
        .success()
        .stdout(predicates::str::contains("github"))
        .stdout(predicates::str::contains("demo").not());
}

#[test]
fn list_plain_format_uses_plain_text_and_bare_urls() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "demo"]);
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "add feature a");
    repo.stack().args(["submit"]).assert().success();

    repo.stack()
        .args(["list", "--format", "plain"])
        .assert()
        .success()
        // Unquoted base; bare URL on its own line for chat apps to auto-link.
        .stdout(predicates::str::contains("1 PR, base main"))
        .stdout(predicates::str::contains("1. add feature a (#1) - open"))
        .stdout(predicates::str::contains("   demo://review/1"))
        // No markdown link syntax.
        .stdout(predicates::str::contains("](").not());
}

#[test]
fn undo_restores_branch_tips_after_a_restack() {
    let repo = TestRepo::new();

    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "a work");
    repo.stack().args(["new", "feature/b"]).assert().success();
    repo.commit_file("b.txt", "b\n", "b work");
    let before = repo.git(["rev-parse", "feature/b"]);

    // Move the parent so the restack actually rewrites feature/b.
    repo.git(["switch", "feature/a"]);
    repo.commit_file("a2.txt", "more\n", "a moves on");
    repo.stack().arg("restack").assert().success();
    repo.git(["switch", "feature/b"]);
    assert_ne!(repo.git(["rev-parse", "feature/b"]), before);

    repo.stack()
        .arg("undo")
        .assert()
        .success()
        .stdout(predicates::str::contains("undid restack"))
        .stdout(predicates::str::contains(
            "pushes and merged reviews are not reverted",
        ));

    assert_eq!(repo.git(["rev-parse", "feature/b"]), before);
    // One-shot: a second undo has nothing to restore.
    repo.stack()
        .arg("undo")
        .assert()
        .failure()
        .stderr(predicates::str::contains("nothing to undo"));
}

#[test]
fn undo_recreates_a_branch_cleanup_deleted() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "demo"]);
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "a work");
    repo.stack().args(["submit"]).assert().success();
    let sha = repo.git(["rev-parse", "feature/a"]);

    // Merge lands feature/a on main and cleanup deletes it.
    repo.stack().args(["merge", "-y"]).assert().success();
    assert_eq!(
        repo.git_status(["rev-parse", "--verify", "feature/a"])
            .status
            .code(),
        Some(128),
        "feature/a should be gone after merge"
    );

    repo.stack().arg("undo").assert().success();
    // The deleted branch is back at its pre-merge tip.
    assert_eq!(repo.git(["rev-parse", "feature/a"]), sha);
}

#[test]
fn undo_refuses_with_a_dirty_worktree() {
    let repo = TestRepo::new();
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "a work");
    repo.git(["switch", "feature/a"]);
    repo.commit_file("a2.txt", "more\n", "a moves on");
    repo.git(["switch", "main"]); // make feature/a the stack, restack a no-op-free case
    repo.git(["switch", "feature/a"]);
    repo.stack().arg("restack").assert().success();

    // Dirty the tree, then undo must refuse rather than reset over it.
    repo.write("uncommitted.txt", "work in progress\n");
    repo.git(["add", "uncommitted.txt"]);
    repo.stack()
        .arg("undo")
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "worktree has uncommitted changes",
        ));
}

#[test]
fn errors_are_prefixed_and_colored() {
    let repo = TestRepo::new();

    // A failing command: no stack to merge. Captured (non-tty) stderr is
    // plain, but carries the `error:` prefix and exits nonzero.
    repo.stack()
        .arg("merge")
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "error: no stacked branches to merge",
        ))
        .stderr(predicates::str::contains("\u{1b}[").not());

    // Forced color paints the prefix red.
    repo.stack()
        .arg("merge")
        .env("CLICOLOR_FORCE", "1")
        .assert()
        .failure()
        .stderr(predicates::str::contains("\u{1b}["))
        .stderr(predicates::str::contains("error:"));
}

#[test]
fn view_reports_no_review_without_one() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "demo"]);
    repo.stack().args(["new", "feature/a"]).assert().success();

    repo.stack()
        .args(["view", "feature/a"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "no demo review found for feature/a; submit it first with `git stk submit`",
        ));
}

#[test]
fn view_opens_the_current_branch_review() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "demo"]);
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "a work");
    repo.stack().args(["submit"]).assert().success();

    // The demo has no browser, but the command resolves the review, prints
    // the opening line, and the provider's graceful note.
    repo.stack()
        .args(["view"])
        .assert()
        .success()
        .stdout(predicates::str::contains("opening #1"))
        .stdout(predicates::str::contains("demo reviews have no web page"));
}

#[test]
fn guide_requires_a_terminal() {
    let repo = TestRepo::new();

    repo.stack()
        .arg("guide")
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "the guide is interactive; run it from a terminal",
        ));

    // A named tour still needs the terminal.
    repo.stack()
        .args(["guide", "conflicts"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "the guide is interactive; run it from a terminal",
        ));
}

// The two scripted tours run interactively, so their exact recipes are
// proven here instead: same commands, same files, same expected outcomes.

#[test]
fn guide_conflicts_recipe_conflicts_and_continues() {
    let repo = TestRepo::new();

    repo.stack()
        .args(["new", "feature/payment"])
        .assert()
        .success();
    repo.commit_file("notes.txt", "use stripe\n", "choose payment provider");
    repo.stack()
        .args(["new", "feature/receipts"])
        .assert()
        .success();
    repo.commit_file("notes.txt", "use stripe with receipts\n", "email receipts");
    repo.git(["switch", "feature/payment"]);
    repo.commit_file("notes.txt", "use paypal\n", "switch to paypal");

    repo.stack()
        .arg("restack")
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "resolve conflicts, then run `git stk continue`",
        ));

    repo.write("notes.txt", "use paypal with receipts\n");
    repo.git(["add", "notes.txt"]);
    repo.stack()
        .arg("continue")
        .assert()
        .success()
        .stdout(predicates::str::contains("restack complete"));

    assert_eq!(
        repo.git(["show", "feature/receipts:notes.txt"]),
        "use paypal with receipts"
    );
}

#[test]
fn guide_repair_recipe_recovers_from_the_demo_review() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "demo"]);

    repo.stack().args(["new", "feature/api"]).assert().success();
    repo.commit_file("api.txt", "endpoints\n", "add api");
    repo.stack().args(["new", "feature/ui"]).assert().success();
    repo.commit_file("ui.txt", "buttons\n", "add ui");
    repo.stack().args(["submit", "--stack"]).assert().success();

    repo.git(["config", "--unset", "branch.feature/ui.stkParent"]);

    repo.stack()
        .arg("repair")
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "feature/ui: set parent feature/api (from demo review #2)",
        ));
    assert_eq!(
        repo.git(["config", "--get", "branch.feature/ui.stkParent"]),
        "feature/api"
    );
}

#[test]
fn guide_rejects_unknown_topics() {
    let repo = TestRepo::new();

    repo.stack()
        .args(["guide", "bogus"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("invalid value"))
        .stderr(predicates::str::contains("intro"));
}
