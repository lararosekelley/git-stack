// These suites drive sh-script provider fakes, so they are Unix-only.
#![cfg(unix)]

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
        .args(["merge", "--dry-run", "--auto"])
        .env("PATH", path.clone())
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "would merge A work (#12) into main (squash, auto)",
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
fn merge_all_lands_the_whole_stack() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);

    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "a work");
    repo.stack().args(["new", "feature/b"]).assert().success();
    repo.commit_file("b.txt", "b\n", "b work");
    let _bare = repo.add_bare_origin(&["main", "feature/a", "feature/b"]);

    // Stateful fake: each `pr merge` flips its PR to merged, and the
    // retarget from the first sync moves #13's base to main.
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
base13() { cat base-13 2>/dev/null || echo feature/a; }
case "$*" in
  pr\ merge\ 12*)
    printf '%s\n' "$*" > merge-args-12.txt
    ;;
  pr\ merge\ 13*)
    printf '%s\n' "$*" > merge-args-13.txt
    ;;
  pr\ edit\ 13\ --base\ *)
    printf '%s' "$5" > base-13
    ;;
  *feature/a\ --state\ merged*)
    if [ -f merge-args-12.txt ]; then
      printf '[{"number":12,"state":"MERGED","baseRefName":"main","headRefName":"feature/a","url":"https://example.com/12","title":"A work"}]\n'
    else
      printf '[]\n'
    fi
    ;;
  *feature/a*)
    if [ -f merge-args-12.txt ]; then
      printf '[]\n'
    else
      printf '[{"number":12,"state":"OPEN","baseRefName":"main","headRefName":"feature/a","url":"https://example.com/12","title":"A work"}]\n'
    fi
    ;;
  *feature/b\ --state\ merged*)
    if [ -f merge-args-13.txt ]; then
      printf '[{"number":13,"state":"MERGED","baseRefName":"%s","headRefName":"feature/b","url":"https://example.com/13","title":"B work"}]\n' "$(base13)"
    else
      printf '[]\n'
    fi
    ;;
  *feature/b*)
    if [ -f merge-args-13.txt ]; then
      printf '[]\n'
    else
      printf '[{"number":13,"state":"OPEN","baseRefName":"%s","headRefName":"feature/b","url":"https://example.com/13","title":"B work"}]\n' "$(base13)"
    fi
    ;;
  pr\ edit*)
    printf 'edited\n'
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
    );

    repo.stack()
        .args(["merge", "--all", "-y"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains("merged A work (#12)"))
        .stdout(predicates::str::contains("merged B work (#13)"))
        .stdout(predicates::str::contains(
            "stack complete: everything merged into main",
        ))
        .stdout(predicates::str::contains(
            "merge complete: 2 of 2 reviews merged",
        ));

    let first = fs::read_to_string(repo.path().join("merge-args-12.txt")).expect("merge 12");
    assert_eq!(first.trim(), "pr merge 12 --squash");
    let second = fs::read_to_string(repo.path().join("merge-args-13.txt")).expect("merge 13");
    assert_eq!(second.trim(), "pr merge 13 --squash");

    assert_eq!(repo.git(["branch", "--show-current"]), "main");
    assert_eq!(
        repo.git_status(["branch", "--list", "feature/a", "feature/b"])
            .stdout
            .len(),
        0
    );
}

#[test]
fn merge_all_dry_run_lists_each_review() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);

    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.stack().args(["new", "feature/b"]).assert().success();

    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  pr\ merge*)
    touch merged.txt
    ;;
  *feature/a*)
    cat <<'JSON'
[{"number":12,"state":"OPEN","baseRefName":"main","headRefName":"feature/a","url":"https://example.com/12","title":"A work"}]
JSON
    ;;
  *feature/b*)
    cat <<'JSON'
[{"number":13,"state":"OPEN","baseRefName":"feature/a","headRefName":"feature/b","url":"https://example.com/13","title":"B work"}]
JSON
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
    );

    repo.stack()
        .args(["merge", "--all", "--dry-run"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "would merge A work (#12) into main (squash)",
        ))
        .stdout(predicates::str::contains(
            "would merge B work (#13) into feature/a (squash)",
        ))
        .stdout(predicates::str::contains("would sync after each merge"));

    assert!(!repo.path().join("merged.txt").exists());
}

#[test]
fn merge_all_stops_when_a_merge_only_schedules() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);

    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "a work");
    repo.stack().args(["new", "feature/b"]).assert().success();
    repo.commit_file("b.txt", "b\n", "b work");

    // The first merge only schedules (the PR stays open), so the loop must
    // stop without touching the rest of the stack.
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  pr\ merge\ 12*)
    printf '%s\n' "$*" > merge-args-12.txt
    ;;
  pr\ merge*)
    touch unexpected-merge.txt
    ;;
  *feature/a*)
    cat <<'JSON'
[{"number":12,"state":"OPEN","baseRefName":"main","headRefName":"feature/a","url":"https://example.com/12","title":"A work"}]
JSON
    ;;
  *feature/b*)
    cat <<'JSON'
[{"number":13,"state":"OPEN","baseRefName":"feature/a","headRefName":"feature/b","url":"https://example.com/13","title":"B work"}]
JSON
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
    );

    repo.stack()
        .args(["merge", "--all", "-y"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "merge scheduled for A work (#12); rerun `git stk sync` once checks pass",
        ))
        .stdout(predicates::str::contains(
            "merge complete: 0 of 2 reviews merged",
        ));

    assert!(!repo.path().join("unexpected-merge.txt").exists());
    assert_eq!(repo.git(["branch", "--show-current"]), "feature/b");
}

#[test]
fn merge_all_wait_gates_each_merge_on_checks() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);

    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "a work");

    // Checks are green, so the wait clears and the merge follows.
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  pr\ checks\ 12)
    printf '%s\n' "$*" > checks-args.txt
    exit 0
    ;;
  pr\ merge\ 12*)
    printf '%s\n' "$*" > merge-args.txt
    ;;
  *feature/a\ --state\ merged*)
    if [ -f merge-args.txt ]; then
      printf '[{"number":12,"state":"MERGED","baseRefName":"main","headRefName":"feature/a","url":"https://example.com/12","title":"A work"}]\n'
    else
      printf '[]\n'
    fi
    ;;
  *feature/a*)
    if [ -f merge-args.txt ]; then
      printf '[]\n'
    else
      printf '[{"number":12,"state":"OPEN","baseRefName":"main","headRefName":"feature/a","url":"https://example.com/12","title":"A work"}]\n'
    fi
    ;;
  pr\ edit*)
    printf 'edited\n'
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
    );

    repo.stack()
        .args(["merge", "--all", "--wait", "-y"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains("waiting for checks on #12"))
        .stdout(predicates::str::contains("merged A work (#12)"))
        .stdout(predicates::str::contains(
            "merge complete: 1 of 1 review merged",
        ));

    // The gate ran `gh pr checks` (no `--watch` in the poll model).
    let checks = fs::read_to_string(repo.path().join("checks-args.txt")).expect("checks args");
    assert_eq!(checks.trim(), "pr checks 12");
}

#[test]
fn merge_all_wait_stops_when_checks_fail() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    // The config default turns waiting on without the flag.
    repo.git(["config", "stk.mergeWait", "true"]);

    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "a work");

    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  pr\ checks\ 12)
    echo 'X  lint  failing' >&2
    exit 1
    ;;
  pr\ merge*)
    touch merged.txt
    ;;
  *feature/a*)
    cat <<'JSON'
[{"number":12,"state":"OPEN","baseRefName":"main","headRefName":"feature/a","url":"https://example.com/12","title":"A work"}]
JSON
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
    );

    repo.stack()
        .args(["merge", "--all", "-y"])
        .env("PATH", path)
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "checks failed for #12; fix them and rerun `git stk merge --all`",
        ));
    assert!(!repo.path().join("merged.txt").exists());
}

#[test]
fn merge_all_no_wait_overrides_the_config() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.git(["config", "stk.mergeWait", "true"]);

    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "a work");

    // No `pr checks` handler: a checks call would fall through and fail
    // the wait, so success proves it never ran.
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  pr\ checks*)
    echo "checks should not run" >&2
    exit 1
    ;;
  pr\ merge\ 12*)
    printf '%s\n' "$*" > merge-args.txt
    ;;
  *feature/a\ --state\ merged*)
    if [ -f merge-args.txt ]; then
      printf '[{"number":12,"state":"MERGED","baseRefName":"main","headRefName":"feature/a","url":"https://example.com/12","title":"A work"}]\n'
    else
      printf '[]\n'
    fi
    ;;
  *feature/a*)
    if [ -f merge-args.txt ]; then
      printf '[]\n'
    else
      printf '[{"number":12,"state":"OPEN","baseRefName":"main","headRefName":"feature/a","url":"https://example.com/12","title":"A work"}]\n'
    fi
    ;;
  pr\ edit*)
    printf 'edited\n'
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
    );

    repo.stack()
        .args(["merge", "--all", "--no-wait", "-y"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains("merged A work (#12)"));
}

#[test]
fn merge_all_conflicts_with_auto() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);

    repo.stack()
        .args(["merge", "--all", "--auto"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("cannot be used with"));
}

#[test]
fn merge_auto_schedules_and_skips_the_sync() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);

    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "a work");

    // The PR stays open after `pr merge --auto`: scheduled, not merged.
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  pr\ merge\ 12*)
    printf '%s\n' "$*" > merge-args.txt
    ;;
  *feature/a\ --state\ merged*)
    printf '[]\n'
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
        .args(["merge", "-y", "--auto"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "merge scheduled for A work (#12); rerun `git stk sync` once checks pass",
        ));

    let recorded = fs::read_to_string(repo.path().join("merge-args.txt")).expect("merge args");
    assert_eq!(recorded.trim(), "pr merge 12 --squash --auto");
    // No sync ran: the branch survives untouched.
    assert_eq!(repo.git(["branch", "--show-current"]), "feature/a");
}

#[test]
fn merge_hints_when_required_checks_block_it() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);

    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "a work");

    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  pr\ merge\ 12*)
    echo 'GraphQL: Required status check "ci" is expected. (mergePullRequest)' >&2
    exit 1
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
        .args(["merge", "-y"])
        .env("PATH", path)
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "hint: required checks may not be green yet - rerun `git stk merge` \
             when they pass, or schedule with `git stk merge --auto`",
        ));
}

#[test]
fn merge_reports_a_scheduled_gitlab_auto_merge() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "gitlab"]);

    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "a work");

    // glab exits 0 after scheduling the merge; the MR stays open.
    let path = repo.fake_cli(
        "glab",
        r##"#!/usr/bin/env sh
case "$*" in
  mr\ merge\ 34*)
    printf 'merge scheduled to run when pipeline succeeds\n'
    ;;
  *feature/a*)
    cat <<'JSON'
[{"iid":34,"state":"opened","target_branch":"main","source_branch":"feature/a","web_url":"https://gitlab.com/owner/repo/-/merge_requests/34","title":"A work"}]
JSON
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
        .success()
        .stdout(predicates::str::contains(
            "merge scheduled for A work (!34); rerun `git stk sync` once checks pass",
        ));

    assert_eq!(repo.git(["branch", "--show-current"]), "feature/a");
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
