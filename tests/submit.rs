use std::{fs, process::Command};
mod common;

use common::TestRepo;
use predicates::prelude::PredicateBooleanExt;

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
    assert!(bottom.contains("<!-- git-stk:data "));
    assert!(
        bottom.contains("- \u{1F7E2} [Top change (#13)](https://github.com/owner/repo/pull/13)")
    );
    assert!(bottom.contains(
        "- \u{1F7E2} [Bottom change (#12)](https://github.com/owner/repo/pull/12) \u{1F448}"
    ));
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
    assert!(top.contains(
        "- \u{1F7E2} [Top change (#13)](https://github.com/owner/repo/pull/13) \u{1F448}"
    ));
    assert!(!top.contains("(#12)](https://github.com/owner/repo/pull/12) \u{1F448}"));
}

#[test]
fn submit_links_issue_referenced_by_branch_name() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.git(["switch", "-c", "5-fix-thing"]);
    repo.git(["config", "branch.5-fix-thing.stkParent", "main"]);
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  pr\ view\ 12*)
    printf '{"body":"Description."}\n'
    ;;
  pr\ edit\ 12\ --body*)
    printf '%s\n' "$*" > edit-body-12.txt
    ;;
  pr\ list*5-fix-thing*)
    cat <<'JSON'
[{"number":12,"state":"OPEN","baseRefName":"main","headRefName":"5-fix-thing","url":"https://github.com/owner/repo/pull/12","title":"Fix thing"}]
JSON
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
    );

    // Dry run announces the link without editing anything.
    repo.stack()
        .args(["submit", "--dry-run"])
        .env("PATH", path.clone())
        .assert()
        .success()
        .stdout(predicates::str::contains("would link issue #5 in #12"));
    assert!(!repo.path().join("edit-body-12.txt").exists());

    repo.stack()
        .arg("submit")
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains("linked issue #5 in #12"));

    let body = fs::read_to_string(repo.path().join("edit-body-12.txt")).expect("edited body");
    assert!(body.contains("Description."));
    assert!(body.contains("<!-- git-stk:closes -->\nCloses #5\n<!-- /git-stk:closes -->"));
}

#[test]
fn submit_desc_sets_replaces_and_clears_the_description_block() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.git(["switch", "-c", "feature/a"]);
    repo.git(["config", "branch.feature/a.stkParent", "main"]);

    // First pass: a body with an existing stack section; the description
    // must land above it.
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  pr\ view\ 12*)
    printf '{"body":"Intro.\\n\\n<!-- git-stk:stack -->\\nstack list\\n<!-- /git-stk:stack -->"}\n'
    ;;
  pr\ edit\ 12\ --body*)
    printf '%s\n' "$*" > edit-body-12.txt
    ;;
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
        .args(["submit", "--dry-run", "-d", "What and why."])
        .env("PATH", path.clone())
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "would set the description in #12",
        ));
    assert!(!repo.path().join("edit-body-12.txt").exists());

    repo.stack()
        .args(["submit", "-d", "What and why."])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains("set description in #12"));

    let body = fs::read_to_string(repo.path().join("edit-body-12.txt")).expect("edited body");
    assert!(
        body.contains("<!-- git-stk:description -->\nWhat and why.\n<!-- /git-stk:description -->")
    );
    let intro = body.find("Intro.").expect("intro");
    let description = body.find("What and why.").expect("description");
    let stack = body.find("stack list").expect("stack");
    assert!(intro < description && description < stack);

    // Second pass: a body that already carries a description; an empty
    // --desc clears the block and leaves the rest alone.
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  pr\ view\ 12*)
    printf '{"body":"Intro.\\n\\n<!-- git-stk:description -->\\nStale.\\n<!-- /git-stk:description -->"}\n'
    ;;
  pr\ edit\ 12\ --body*)
    printf '%s\n' "$*" > edit-body-12.txt
    ;;
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
        .args(["submit", "-d", ""])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains("cleared description in #12"));

    let body = fs::read_to_string(repo.path().join("edit-body-12.txt")).expect("edited body");
    assert!(body.contains("Intro."));
    assert!(!body.contains("git-stk:description"));
    assert!(!body.contains("Stale."));
}

#[test]
fn submit_stack_desc_targets_only_the_current_branch() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.stack().args(["new", "feature/b"]).assert().success();
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  pr\ view\ 12*)
    printf '{"body":""}\n'
    ;;
  pr\ view\ 13*)
    printf '{"body":""}\n'
    ;;
  pr\ edit\ 12\ --body*)
    printf '%s\n=====\n' "$*" >> edit-body-12.log
    ;;
  pr\ edit\ 13\ --body*)
    printf '%s\n=====\n' "$*" >> edit-body-13.log
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

    // Standing on the leaf: the description belongs to its review alone.
    repo.stack()
        .args(["submit", "--stack", "-d", "Top summary."])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains("set description in #13"));

    let top = fs::read_to_string(repo.path().join("edit-body-13.log")).expect("top edits");
    assert!(
        top.contains("<!-- git-stk:description -->\nTop summary.\n<!-- /git-stk:description -->")
    );
    let bottom = fs::read_to_string(repo.path().join("edit-body-12.log")).expect("bottom edits");
    assert!(!bottom.contains("git-stk:description"));
}

#[test]
fn submit_stack_preserves_merged_ledger_entries() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.stack().args(["new", "feature/b"]).assert().success();

    // The bottom PR's body carries a ledger that remembers #11, a review
    // whose branch merged and was deleted long ago. The top PR has never
    // seen it.
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  pr\ view\ 11*)
    printf '{"body":"Old description."}\n'
    ;;
  pr\ view\ 12*)
    cat <<'JSON'
{"body":"Intro.\n\n<!-- git-stk:stack -->\n<!-- git-stk:data [{\"id\":\"#11\",\"url\":\"https://github.com/owner/repo/pull/11\",\"title\":\"Landed\",\"state\":\"merged\"}] -->\n- stale bullets\n<!-- /git-stk:stack -->"}
JSON
    ;;
  pr\ view\ 13*)
    printf '{"body":""}\n'
    ;;
  pr\ edit\ 11\ --body*)
    printf '%s\n' "$*" > edit-body-11.txt
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
        .stdout(predicates::str::contains("updated stack note in #13"))
        .stdout(predicates::str::contains("updated stack note in #11"));

    // The merged entry survives, restyled, below the live stack.
    let bottom = fs::read_to_string(repo.path().join("edit-body-12.txt")).expect("bottom body");
    assert!(bottom.contains("Intro."));
    assert!(bottom.contains(
        "- \u{1F7E3} ~~[Landed (#11)](https://github.com/owner/repo/pull/11)~~ (merged)"
    ));
    let top_at = bottom.find("(#13)").expect("top entry");
    let bottom_at = bottom.find("(#12)").expect("bottom entry");
    let landed_at = bottom.find("(#11)").expect("merged entry");
    assert!(
        top_at < bottom_at && bottom_at < landed_at,
        "leaf-first order"
    );

    // History propagates to bodies that never carried it.
    let top = fs::read_to_string(repo.path().join("edit-body-13.txt")).expect("top body");
    assert!(top.contains("~~[Landed (#11)](https://github.com/owner/repo/pull/11)~~ (merged)"));

    // The merged review's own body gets the refreshed ledger, pointing at
    // itself.
    let landed = fs::read_to_string(repo.path().join("edit-body-11.txt")).expect("merged body");
    assert!(landed.contains("Old description."));
    assert!(landed.contains(
        "- \u{1F7E3} ~~[Landed (#11)](https://github.com/owner/repo/pull/11)~~ (merged) \u{1F448}"
    ));
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
    assert!(top.contains("- \u{1F7E2} [Top change (#13)]"));
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
        "- \u{1F7E2} [Top change (!35)](https://gitlab.com/owner/repo/-/merge_requests/35) \u{1F448}"
    ));
    assert!(recorded.contains(
        "- \u{1F7E2} [Bottom change (!34)](https://gitlab.com/owner/repo/-/merge_requests/34)"
    ));
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

#[test]
fn submit_downstack_stops_at_the_current_branch() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.stack().args(["new", "feature/b"]).assert().success();
    repo.stack().args(["new", "feature/c"]).assert().success();
    repo.git(["switch", "feature/b"]);
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
printf '[]\n'
"##,
    );

    // Standing mid-stack: the WIP leaf above stays unsubmitted.
    repo.stack()
        .args(["submit", "--downstack", "--dry-run"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains("would create feature/a -> main"))
        .stdout(predicates::str::contains(
            "would create feature/b -> feature/a",
        ))
        .stdout(predicates::str::contains("feature/c").not());

    // The scopes are mutually exclusive.
    repo.stack()
        .args(["submit", "--downstack", "--stack"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("cannot be used with"));
}

#[test]
fn submit_draft_flag_and_config_control_creation() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.git(["switch", "-c", "feature/b"]);
    repo.git(["config", "branch.feature/b.stkParent", "main"]);
    let log_path = repo.path().join("submit.log");
    let path = repo.fake_cli(
        "gh",
        &format!(
            r##"#!/usr/bin/env sh
printf '%s\n' "$*" >> '{log}'
case "$*" in
  pr\ create*)
    printf 'created url\n'
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
            log = log_path.display()
        ),
    );

    // --draft passes through to creation.
    repo.stack()
        .args(["submit", "--draft"])
        .env("PATH", path.clone())
        .assert()
        .success();
    let log = fs::read_to_string(&log_path).expect("submit log");
    assert!(log.contains("pr create --head feature/b --base main --fill --draft"));

    // The config makes drafts the default; --no-draft overrides it.
    fs::remove_file(&log_path).expect("reset log");
    repo.git(["config", "stk.submitDraft", "true"]);
    repo.stack()
        .arg("submit")
        .env("PATH", path.clone())
        .assert()
        .success();
    let log = fs::read_to_string(&log_path).expect("submit log");
    assert!(log.contains("--fill --draft"));

    fs::remove_file(&log_path).expect("reset log");
    repo.stack()
        .args(["submit", "--no-draft"])
        .env("PATH", path)
        .assert()
        .success();
    let log = fs::read_to_string(&log_path).expect("submit log");
    assert!(log.contains("pr create --head feature/b --base main --fill\n"));
}

#[test]
fn submit_ready_marks_draft_reviews_ready() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "demo"]);
    repo.git(["config", "stk.submitDraft", "true"]);

    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "a work");
    repo.stack().args(["new", "feature/b"]).assert().success();
    repo.commit_file("b.txt", "b\n", "b work");

    // Drafted by config, then flipped ready in one stack-wide pass.
    repo.stack().args(["submit", "--stack"]).assert().success();
    repo.stack()
        .args(["submit", "--stack", "--ready"])
        .assert()
        .success()
        .stdout(predicates::str::contains("marked #1 ready"))
        .stdout(predicates::str::contains("marked #2 ready"));

    // Already ready: nothing left to mark.
    repo.stack()
        .args(["submit", "--stack", "--ready"])
        .assert()
        .success()
        .stdout(predicates::str::contains("marked").not());
}

#[test]
fn bare_submit_uses_submit_stack_config() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.git(["config", "stk.submitStack", "true"]);
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.stack().args(["new", "feature/b"]).assert().success();
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
printf '[]\n'
"##,
    );

    // Bare submit from the leaf: config turns on whole-stack mode.
    repo.stack()
        .args(["submit", "--dry-run"])
        .env("PATH", path.clone())
        .assert()
        .success()
        .stdout(predicates::str::contains("would create feature/a -> main"))
        .stdout(predicates::str::contains(
            "would create feature/b -> feature/a",
        ));

    // --no-stack overrides the config back to single-branch.
    repo.stack()
        .args(["submit", "--dry-run", "--no-stack"])
        .env("PATH", path.clone())
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "would create feature/b -> feature/a",
        ))
        .stdout(predicates::str::contains("feature/a -> main").not());

    // An explicit branch also means single-branch, config or not.
    repo.stack()
        .args(["submit", "feature/a", "--dry-run"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains("would create feature/a -> main"))
        .stdout(predicates::str::contains("feature/b").not());
}
