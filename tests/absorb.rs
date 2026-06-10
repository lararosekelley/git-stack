mod common;

use common::TestRepo;

/// main <- feature/a (adds foo.txt) <- feature/b (adds bar.txt). Each line is
/// owned by a distinct commit, so blame attribution is unambiguous.
fn stack() -> TestRepo {
    let repo = TestRepo::new();
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("foo.txt", "alpha\n", "add foo");
    repo.stack().args(["new", "feature/b"]).assert().success();
    repo.commit_file("bar.txt", "beta\n", "add bar");
    repo
}

#[test]
fn absorb_dry_run_routes_hunks_to_owning_commits() {
    let repo = stack();
    let head = repo.git(["rev-parse", "HEAD"]);

    // Edit a line each commit introduced, two branches down and one down.
    repo.write("foo.txt", "alpha fixed\n");
    repo.write("bar.txt", "beta fixed\n");
    repo.git(["add", "foo.txt", "bar.txt"]);

    repo.stack()
        .args(["absorb", "--dry-run"])
        .assert()
        .success()
        .stdout(predicates::str::contains("foo.txt:1 -> feature/a"))
        .stdout(predicates::str::contains("add foo"))
        .stdout(predicates::str::contains("bar.txt:1 -> feature/b"))
        .stdout(predicates::str::contains("add bar"));

    // --dry-run rewrites nothing.
    assert_eq!(repo.git(["rev-parse", "HEAD"]), head);
}

#[test]
fn absorb_dry_run_leaves_trunk_owned_and_added_lines_unabsorbed() {
    let repo = stack();

    // README is the trunk's; a fresh line belongs to no commit.
    repo.write("README.md", "# changed\n");
    repo.write("foo.txt", "alpha\nbrand new\n");
    repo.git(["add", "README.md", "foo.txt"]);

    repo.stack()
        .args(["absorb", "--dry-run"])
        .assert()
        .success()
        .stdout(predicates::str::contains("unabsorbed (left in place)"))
        .stdout(predicates::str::contains(
            "README.md:1 owned by a commit outside the stack",
        ))
        .stdout(predicates::str::contains(
            "foo.txt:1 added lines - no commit to attribute",
        ));
}

#[test]
fn absorb_without_staged_changes_reports_nothing() {
    let repo = stack();

    repo.stack()
        .args(["absorb", "--dry-run"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("no staged changes to absorb"));
}

#[test]
fn absorb_include_unstaged_attributes_without_staging() {
    let repo = stack();
    // Edit but do not `git add`.
    repo.write("foo.txt", "alpha fixed\n");

    // Staged-only sees nothing.
    repo.stack()
        .args(["absorb", "--dry-run"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("no staged changes"));

    // --include-unstaged picks the edit up.
    repo.stack()
        .args(["absorb", "--dry-run", "--include-unstaged"])
        .assert()
        .success()
        .stdout(predicates::str::contains("foo.txt:1 -> feature/a"));
}

#[test]
fn absorb_respects_include_unstaged_config() {
    let repo = stack();
    repo.git(["config", "stk.absorbIncludeUnstaged", "true"]);
    repo.write("foo.txt", "alpha fixed\n");

    repo.stack()
        .args(["absorb", "--dry-run"])
        .assert()
        .success()
        .stdout(predicates::str::contains("foo.txt:1 -> feature/a"));
}
