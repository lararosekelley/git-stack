use std::{fs, process::Command};
mod common;

use common::TestRepo;

#[test]
fn submit_creates_github_pr_when_none_exists() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.git(["switch", "-c", "feature/b"]);
    repo.git(["config", "branch.feature/b.stkParent", "feature/a"]);
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  pr\ list*)
    printf '[]\n'
    ;;
  pr\ create*)
    printf 'https://github.com/lararosekelley/git-stk/pull/12\n'
    ;;
  *)
    echo "unexpected gh args: $*" >&2
    exit 1
    ;;
esac
"##,
    );

    repo.stack()
        .arg("submit")
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains("created feature/b -> feature/a"))
        .stdout(predicates::str::contains(
            "https://github.com/lararosekelley/git-stk/pull/12",
        ))
        .stdout(predicates::str::contains(
            "submit complete: 1 created, 0 updated, 0 skipped",
        ));
}

#[test]
fn submit_dry_run_reports_create_without_calling_create() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.git(["switch", "-c", "feature/b"]);
    repo.git(["config", "branch.feature/b.stkParent", "feature/a"]);
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  pr\ list*)
    printf '[]\n'
    ;;
  *)
    echo "unexpected gh args: $*" >&2
    exit 1
    ;;
esac
"##,
    );

    repo.stack()
        .args(["submit", "--dry-run"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout("would create feature/b -> feature/a\nsubmit complete: 1 created, 0 updated, 0 skipped\n");
}

#[test]
fn submit_noops_when_github_pr_already_targets_parent() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.git(["config", "branch.feature/b.stkParent", "feature/a"]);
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  pr\ list*)
    cat <<'JSON'
[{"number":12,"state":"OPEN","baseRefName":"feature/a","headRefName":"feature/b","url":"https://github.com/lararosekelley/git-stk/pull/12"}]
JSON
    ;;
  *)
    echo "unexpected gh args: $*" >&2
    exit 1
    ;;
esac
"##,
    );

    repo.stack()
        .args(["submit", "feature/b"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout("feature/b already targets feature/a (#12)\nsubmit complete: 0 created, 0 updated, 1 skipped\n");
}

#[test]
fn submit_updates_gitlab_mr_target_when_parent_changed() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "gitlab"]);
    repo.git(["config", "branch.feature/b.stkParent", "main"]);
    let path = repo.fake_cli(
        "glab",
        r##"#!/usr/bin/env sh
case "$*" in
  mr\ list*)
    cat <<'JSON'
[{"iid":34,"state":"opened","target_branch":"feature/a","source_branch":"feature/b","web_url":"https://gitlab.com/lararosekelley/git-stk-mirror/-/merge_requests/34"}]
JSON
    ;;
  mr\ update*)
    printf 'updated mr\n'
    ;;
  *)
    echo "unexpected glab args: $*" >&2
    exit 1
    ;;
esac
"##,
    );

    repo.stack()
        .args(["submit", "feature/b"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains("updated feature/b -> main (!34)"))
        .stdout(predicates::str::contains("updated mr"))
        .stdout(predicates::str::contains(
            "submit complete: 0 created, 1 updated, 0 skipped",
        ));
}

#[test]
fn submit_dry_run_reports_update_without_calling_update() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "gitlab"]);
    repo.git(["config", "branch.feature/b.stkParent", "main"]);
    let path = repo.fake_cli(
        "glab",
        r##"#!/usr/bin/env sh
case "$*" in
  mr\ list*)
    cat <<'JSON'
[{"iid":34,"state":"opened","target_branch":"feature/a","source_branch":"feature/b","web_url":"https://gitlab.com/lararosekelley/git-stk-mirror/-/merge_requests/34"}]
JSON
    ;;
  *)
    echo "unexpected glab args: $*" >&2
    exit 1
    ;;
esac
"##,
    );

    repo.stack()
        .args(["submit", "--dry-run", "feature/b"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout("would update feature/b -> main (!34)\nsubmit complete: 0 created, 1 updated, 0 skipped\n");
}

#[test]
fn submit_requires_stack_parent() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);

    repo.stack()
        .args(["submit", "feature/b"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("feature/b has no stack parent"));
}

#[test]
fn submit_stack_creates_reviews_parent_first() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.stack().args(["new", "feature/b"]).assert().success();

    let log_path = repo.path().join("submit.log");
    let path = repo.fake_cli(
        "gh",
        &format!(
            r##"#!/usr/bin/env sh
printf '%s\n' "$*" >> '{log}'
case "$*" in
  pr\ list*)
    printf '[]\n'
    ;;
  pr\ create*)
    printf 'created url\n'
    ;;
  *)
    echo "unexpected gh args: $*" >&2
    exit 1
    ;;
esac
"##,
            log = log_path.display()
        ),
    );

    repo.git(["switch", "feature/a"]);
    repo.stack()
        .args(["submit", "--stack"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains("created feature/a -> main"))
        .stdout(predicates::str::contains("created feature/b -> feature/a"))
        .stdout(predicates::str::contains(
            "submit complete: 2 created, 0 updated, 0 skipped",
        ));

    let log = fs::read_to_string(log_path).expect("read submit log");
    let create_a = log
        .find("pr create --head feature/a --base main --fill")
        .expect("feature/a create call");
    let create_b = log
        .find("pr create --head feature/b --base feature/a --fill")
        .expect("feature/b create call");
    assert!(create_a < create_b, "parent should submit before child");
}

#[test]
fn submit_stack_validates_parents_before_provider_calls() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.git(["switch", "-c", "feature/a"]);
    repo.git(["switch", "-c", "feature/b"]);
    repo.git(["config", "branch.feature/b.stkParent", "feature/a"]);
    repo.git(["switch", "feature/a"]);
    let log_path = repo.path().join("submit.log");
    let path = repo.fake_cli(
        "gh",
        &format!(
            r##"#!/usr/bin/env sh
printf '%s\n' "$*" >> '{log}'
printf '[]\n'
"##,
            log = log_path.display()
        ),
    );

    repo.stack()
        .args(["submit", "--stack"])
        .env("PATH", path)
        .assert()
        .failure()
        .stderr(predicates::str::contains("feature/a has no stack parent"));

    assert!(
        !log_path.exists(),
        "provider should not be called after validation failure"
    );
}

#[test]
fn submit_stack_writes_stack_overview_into_review_bodies() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.stack().args(["new", "feature/b"]).assert().success();
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  pr\ view\ 12*)
    printf '{"body":"Parent PR description."}\n'
    ;;
  pr\ view\ 13*)
    printf '{"body":"Child PR description."}\n'
    ;;
  pr\ edit\ 12\ --body*)
    printf '%s\n' "$*" > edit-body-12.txt
    ;;
  pr\ edit\ 13\ --body*)
    printf '%s\n' "$*" > edit-body-13.txt
    ;;
  pr\ list*feature/a*)
    cat <<'JSON'
[{"number":12,"state":"OPEN","baseRefName":"main","headRefName":"feature/a","url":"https://github.com/owner/repo/pull/12","title":"Bottom change"}]
JSON
    ;;
  pr\ list*feature/b*)
    cat <<'JSON'
[{"number":13,"state":"OPEN","baseRefName":"feature/a","headRefName":"feature/b","url":"https://github.com/owner/repo/pull/13","title":"Top change"}]
JSON
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
    );

    repo.git(["switch", "feature/a"]);
    repo.stack()
        .args(["submit", "--stack"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains("updated stack note in #12"))
        .stdout(predicates::str::contains("updated stack note in #13"));

    // The bottom PR's body: full list leaf-first, pointer on itself,
    // trunk in backticks, footer link.
    let bottom = fs::read_to_string(repo.path().join("edit-body-12.txt")).expect("bottom body");
    assert!(bottom.contains("Parent PR description."));
    assert!(bottom.contains("- [Top change (#13)](https://github.com/owner/repo/pull/13)"));
    assert!(
        bottom.contains("- [Bottom change (#12)](https://github.com/owner/repo/pull/12) \u{1F448}")
    );
    assert!(bottom.contains("- `main`"));
    assert!(
        bottom.contains(
            "Stack managed by \
             <img src=\"https://raw.githubusercontent.com/lararosekelley/git-stk/main/assets/logo.svg\" \
             width=\"12\" height=\"12\" alt=\"\" /> \
             [git-stk](https://github.com/lararosekelley/git-stk)"
        )
    );

    // The top PR points at itself instead.
    let top = fs::read_to_string(repo.path().join("edit-body-13.txt")).expect("top body");
    assert!(top.contains("- [Top change (#13)](https://github.com/owner/repo/pull/13) \u{1F448}"));
    assert!(!top.contains("(#12)](https://github.com/owner/repo/pull/12) \u{1F448}"));
}

#[test]
fn submit_stack_repairs_mangled_note_markup() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.stack().args(["new", "feature/b"]).assert().success();
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  pr\ view\ 12*)
    printf '{"body":"Intro."}\n'
    ;;
  pr\ view\ 13*)
    printf '{"body":"Intro.\\n\\n<!-- git-stk:stack -->\\nuser deleted the end marker"}\n'
    ;;
  pr\ edit\ 12\ --body*)
    printf '%s\n' "$*" > edit-body-12.txt
    ;;
  pr\ edit\ 13\ --body*)
    printf '%s\n' "$*" > edit-body-13.txt
    ;;
  pr\ list*feature/a*)
    cat <<'JSON'
[{"number":12,"state":"OPEN","baseRefName":"main","headRefName":"feature/a","url":"https://github.com/owner/repo/pull/12","title":"Bottom change"}]
JSON
    ;;
  pr\ list*feature/b*)
    cat <<'JSON'
[{"number":13,"state":"OPEN","baseRefName":"feature/a","headRefName":"feature/b","url":"https://github.com/owner/repo/pull/13","title":"Top change"}]
JSON
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
    );

    repo.git(["switch", "feature/a"]);
    repo.stack()
        .args(["submit", "--stack"])
        .env("PATH", path)
        .assert()
        .success();

    let top = fs::read_to_string(repo.path().join("edit-body-13.txt")).expect("top body");
    assert_eq!(top.matches("<!-- git-stk:stack -->").count(), 1);
    assert_eq!(top.matches("<!-- /git-stk:stack -->").count(), 1);
    assert!(top.contains("Intro."));
    assert!(top.contains("user deleted the end marker"));
    assert!(top.contains("- [Top change (#13)]"));
}

#[test]
fn submit_stack_writes_overview_into_gitlab_descriptions() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "gitlab"]);
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.stack().args(["new", "feature/b"]).assert().success();
    let path = repo.fake_cli(
        "glab",
        r##"#!/usr/bin/env sh
case "$*" in
  mr\ view\ 34*)
    printf '{"description":""}\n'
    ;;
  mr\ view\ 35*)
    printf '{"description":""}\n'
    ;;
  mr\ update\ 35\ --description*)
    printf '%s\n' "$*" > update-description-args.txt
    ;;
  mr\ update\ 34\ --description*)
    printf 'updated 34\n'
    ;;
  mr\ list*feature/a*)
    cat <<'JSON'
[{"iid":34,"state":"opened","target_branch":"main","source_branch":"feature/a","web_url":"https://gitlab.com/owner/repo/-/merge_requests/34","title":"Bottom change"}]
JSON
    ;;
  mr\ list*feature/b*)
    cat <<'JSON'
[{"iid":35,"state":"opened","target_branch":"feature/a","source_branch":"feature/b","web_url":"https://gitlab.com/owner/repo/-/merge_requests/35","title":"Top change"}]
JSON
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
    );

    repo.git(["switch", "feature/a"]);
    repo.stack()
        .args(["submit", "--stack"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains("updated stack note in !35"));

    let recorded = fs::read_to_string(repo.path().join("update-description-args.txt"))
        .expect("update description args");
    assert!(recorded.contains(
        "- [Top change (!35)](https://gitlab.com/owner/repo/-/merge_requests/35) \u{1F448}"
    ));
    assert!(
        recorded
            .contains("- [Bottom change (!34)](https://gitlab.com/owner/repo/-/merge_requests/34)")
    );
    assert!(recorded.contains("- `main`"));
}

#[test]
fn submit_stack_push_pushes_branches_before_provider_calls() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "parent change");
    repo.stack().args(["new", "feature/b"]).assert().success();
    repo.commit_file("b.txt", "b\n", "child change");

    // Bare origin with no branches: submit --push must create them remotely.
    let bare = repo.add_bare_origin(&[]);
    let gh_path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  pr\ create*)
    printf 'created review\n'
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
    );

    repo.git(["switch", "feature/a"]);
    let assert = repo
        .stack()
        .args(["submit", "--stack", "--push"])
        .env("PATH", gh_path)
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "pushed feature/a feature/b to origin",
        ));

    // Remote branches exist and match local.
    assert_eq!(
        repo.remote_sha(&bare, "feature/a"),
        repo.git(["rev-parse", "feature/a"])
    );
    assert_eq!(
        repo.remote_sha(&bare, "feature/b"),
        repo.git(["rev-parse", "feature/b"])
    );

    // Push output precedes review creation output.
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let push_at = stdout.find("pushed feature/a").expect("push line");
    let create_at = stdout.find("created feature/a").expect("create line");
    assert!(
        push_at < create_at,
        "push must happen before submit:\n{stdout}"
    );

    // Upstream tracking was set.
    assert_eq!(
        repo.git(["config", "--get", "branch.feature/a.remote"]),
        "origin"
    );
}

#[test]
fn submit_push_respects_config_and_no_push_overrides_it() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.git(["config", "stk.pushOnSubmit", "true"]);
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "parent change");

    let bare = repo.add_bare_origin(&[]);
    let gh_path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  pr\ create*) printf 'created review\n' ;;
  *) printf '[]\n' ;;
esac
"##,
    );

    // Config enables the push.
    repo.stack()
        .args(["submit"])
        .env("PATH", gh_path.clone())
        .assert()
        .success()
        .stdout(predicates::str::contains("pushed feature/a to origin"));
    assert_eq!(
        repo.remote_sha(&bare, "feature/a"),
        repo.git(["rev-parse", "feature/a"])
    );

    // --no-push overrides the config.
    repo.commit_file("a2.txt", "a2\n", "more work");
    let stale = repo.remote_sha(&bare, "feature/a");
    repo.stack()
        .args(["submit", "--no-push"])
        .env("PATH", gh_path)
        .assert()
        .success();
    assert_eq!(repo.remote_sha(&bare, "feature/a"), stale);
}

#[test]
fn submit_push_dry_run_does_not_push() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "parent change");

    let bare = repo.add_bare_origin(&[]);
    let gh_path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
printf '[]\n'
"##,
    );

    repo.stack()
        .args(["submit", "--push", "--dry-run"])
        .env("PATH", gh_path)
        .assert()
        .success()
        .stdout(predicates::str::contains("would push feature/a to origin"));

    let remote = Command::new("git")
        .args(["rev-parse", "feature/a"])
        .current_dir(bare.path())
        .output()
        .expect("check remote");
    assert!(!remote.status.success(), "dry run must not push");
}

#[test]
fn submit_stack_covers_whole_stack_from_the_leaf() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.stack().args(["new", "feature/b"]).assert().success();
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
printf '[]\n'
"##,
    );

    // Standing on the LEAF: position must not matter.
    repo.stack()
        .args(["submit", "--stack", "--dry-run"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains("would create feature/a -> main"))
        .stdout(predicates::str::contains(
            "would create feature/b -> feature/a",
        ));
}
