use std::fs;

use common::TestRepo;

mod common;

#[test]
fn merge_merges_bottom_review_then_syncs() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.git(["config", "stk.pushOnRestack", "true"]);

    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "a work");
    repo.stack().args(["new", "feature/b"]).assert().success();
    repo.commit_file("b.txt", "b\n", "b work");
    let bare = repo.add_bare_origin(&["main", "feature/a", "feature/b"]);

    // Stateful fake: after `pr merge 12` runs, feature/a reports as merged.
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  pr\ merge\ 12*)
    printf '%s\n' "$*" > merge-args.txt
    ;;
  *feature/a\ --state\ merged*)
    if [ -f merge-args.txt ]; then
      cat <<'JSON'
[{"number":12,"state":"MERGED","baseRefName":"main","headRefName":"feature/a","url":"https://github.com/owner/repo/pull/12","title":"A work"}]
JSON
    else
      printf '[]\n'
    fi
    ;;
  *feature/a*)
    if [ -f merge-args.txt ]; then
      printf '[]\n'
    else
      cat <<'JSON'
[{"number":12,"state":"OPEN","baseRefName":"main","headRefName":"feature/a","url":"https://github.com/owner/repo/pull/12","title":"A work"}]
JSON
    fi
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

    // Run from the leaf with -y: position-independent and unprompted.
    repo.stack()
        .args(["merge", "-y"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains("merged A work (#12)"))
        .stdout(predicates::str::contains("next up: feature/b -> #13"));

    // The provider was asked to squash-merge (the default strategy).
    let recorded = fs::read_to_string(repo.path().join("merge-args.txt")).expect("merge args");
    assert_eq!(recorded.trim(), "pr merge 12 --squash");

    // The sync swept up afterwards: branch gone, child retargeted and pushed.
    assert_eq!(
        repo.git_status(["branch", "--list", "feature/a"])
            .stdout
            .len(),
        0
    );
    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkParent"]),
        "main"
    );
    assert_eq!(
        repo.remote_sha(&bare, "feature/b"),
        repo.git(["rev-parse", "feature/b"])
    );
}

#[test]
fn merge_respects_strategy_config() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.git(["config", "stk.mergeStrategy", "rebase"]);

    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "a work");

    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  pr\ merge\ 12*)
    printf '%s\n' "$*" > merge-args.txt
    ;;
  *feature/a*)
    if [ -f merge-args.txt ]; then
      printf '[]\n'
    else
      cat <<'JSON'
[{"number":12,"state":"OPEN","baseRefName":"main","headRefName":"feature/a","url":"https://github.com/owner/repo/pull/12","title":"A work"}]
JSON
    fi
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
    );

    repo.stack()
        .args(["merge", "-y"])
        .env("PATH", path)
        .assert()
        .success();

    let recorded = fs::read_to_string(repo.path().join("merge-args.txt")).expect("merge args");
    assert_eq!(recorded.trim(), "pr merge 12 --rebase");
}

#[test]
fn merge_dry_run_and_decline_merge_nothing() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);

    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "a work");

    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  pr\ merge*)
    touch merged.txt
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

    repo.stack()
        .args(["merge", "--dry-run"])
        .env("PATH", path.clone())
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "would merge A work (#12) into main (squash)",
        ));
    assert!(!repo.path().join("merged.txt").exists());

    repo.stack()
        .args(["merge"])
        .env("PATH", path)
        .write_stdin("n\n")
        .assert()
        .success()
        .stdout(predicates::str::contains("merge cancelled"));
    assert!(!repo.path().join("merged.txt").exists());
}

#[test]
fn merge_requires_an_open_review_at_the_bottom() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);

    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "a work");

    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
printf '[]\n'
"##,
    );

    repo.stack()
        .args(["merge", "-y"])
        .env("PATH", path)
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "no github review found for feature/a; submit the stack first",
        ));
}
