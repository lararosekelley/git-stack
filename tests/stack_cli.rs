use std::{env, fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

use tempfile::TempDir;

struct TestRepo {
    dir: TempDir,
}

impl TestRepo {
    fn new() -> Self {
        let repo = Self {
            dir: tempfile::tempdir().expect("create temp repo"),
        };
        repo.git(["init", "--initial-branch", "main"]);
        repo.git(["config", "user.email", "test@example.com"]);
        repo.git(["config", "user.name", "Test User"]);
        repo.write("README.md", "# test repo\n");
        repo.git(["add", "README.md"]);
        repo.git(["commit", "-m", "initial commit"]);
        repo
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn write(&self, path: &str, contents: &str) {
        fs::write(self.path().join(path), contents).expect("write test file");
    }

    /// Isolate a command from the developer's global and system git config so
    /// the suite stays hermetic (e.g. a global stk.pushOnSubmit=true must not
    /// change test behavior).
    fn isolate_git_config(command: &mut Command) {
        command.env("GIT_CONFIG_GLOBAL", "/dev/null");
        command.env("GIT_CONFIG_NOSYSTEM", "1");
    }

    fn git<const N: usize>(&self, args: [&str; N]) -> String {
        let mut command = Command::new("git");
        Self::isolate_git_config(&mut command);
        let output = command
            .args(args)
            .current_dir(self.path())
            .output()
            .expect("run git command");

        assert!(
            output.status.success(),
            "git failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    fn git_status<const N: usize>(&self, args: [&str; N]) -> std::process::Output {
        let mut command = Command::new("git");
        Self::isolate_git_config(&mut command);
        command
            .args(args)
            .current_dir(self.path())
            .output()
            .expect("run git command")
    }

    fn stack_output<const N: usize>(&self, args: [&str; N]) -> std::process::Output {
        let mut command = self.stack();
        command.args(args).output().expect("run git-stk command")
    }

    fn supports_update_refs(&self) -> bool {
        let output = self.git_status(["rebase", "-h"]);
        let help = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        help.contains("--update-refs")
    }

    fn commit_file(&self, path: &str, contents: &str, message: &str) {
        self.write(path, contents);
        self.git(["add", path]);
        self.git(["commit", "-m", message]);
    }

    fn stack(&self) -> assert_cmd::Command {
        let mut command = assert_cmd::Command::cargo_bin("git-stk").expect("git-stk binary");
        command.current_dir(self.path());
        command.env("GIT_EDITOR", "true");
        command.env("GIT_CONFIG_GLOBAL", "/dev/null");
        command.env("GIT_CONFIG_NOSYSTEM", "1");
        command
    }

    fn fake_cli(&self, name: &str, script: &str) -> String {
        let bin_dir = self.path().join("fake-bin");
        fs::create_dir_all(&bin_dir).expect("create fake bin dir");

        let path = bin_dir.join(name);
        fs::write(&path, script).expect("write fake cli");

        let mut permissions = fs::metadata(&path)
            .expect("fake cli metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).expect("chmod fake cli");

        format!(
            "{}:{}",
            bin_dir.display(),
            env::var("PATH").unwrap_or_default()
        )
    }
}

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
fn restack_rebases_descendants_onto_updated_parent() {
    let repo = TestRepo::new();

    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "first parent change\n", "add parent change");

    repo.stack().args(["new", "feature/b"]).assert().success();
    repo.commit_file("b.txt", "child change\n", "add child change");

    repo.git(["switch", "feature/a"]);
    repo.commit_file("a2.txt", "second parent change\n", "update parent");

    repo.stack()
        .arg("restack")
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "rebasing feature/b onto feature/a",
        ))
        .stdout(predicates::str::contains("restack complete"));

    let parent_head = repo.git(["rev-parse", "feature/a"]);
    let merge_base = repo.git(["merge-base", "feature/a", "feature/b"]);
    assert_eq!(merge_base, parent_head);
    assert_eq!(repo.git(["branch", "--show-current"]), "feature/b");
}

#[test]
fn restack_uses_update_refs_when_git_config_enables_it() {
    let repo = TestRepo::new();
    if !repo.supports_update_refs() {
        return;
    }
    repo.git(["config", "stk.updateRefs", "true"]);

    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "parent change\n", "add parent change");

    repo.stack().args(["new", "feature/b"]).assert().success();
    repo.commit_file("b.txt", "child change\n", "add child change");

    repo.git(["switch", "feature/a"]);
    repo.commit_file("a2.txt", "second parent change\n", "update parent");

    repo.stack()
        .arg("restack")
        .assert()
        .success()
        .stdout(predicates::str::contains("--update-refs"));
}

#[test]
fn restack_can_force_update_refs() {
    let repo = TestRepo::new();
    if !repo.supports_update_refs() {
        return;
    }
    repo.git(["config", "stk.updateRefs", "false"]);

    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "parent change\n", "add parent change");

    repo.stack().args(["new", "feature/b"]).assert().success();
    repo.commit_file("b.txt", "child change\n", "add child change");

    repo.git(["switch", "feature/a"]);
    repo.commit_file("a2.txt", "second parent change\n", "update parent");

    repo.stack()
        .args(["restack", "--update-refs"])
        .assert()
        .success()
        .stdout(predicates::str::contains("--update-refs"));
}

#[test]
fn restack_can_opt_out_of_update_refs() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.updateRefs", "true"]);

    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "parent change\n", "add parent change");

    repo.stack().args(["new", "feature/b"]).assert().success();
    repo.commit_file("b.txt", "child change\n", "add child change");

    repo.git(["switch", "feature/a"]);
    repo.commit_file("a2.txt", "second parent change\n", "update parent");

    let output = repo.stack_output(["restack", "--no-update-refs"]);
    assert!(
        output.status.success(),
        "restack failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("--update-refs"));
    assert!(stdout.contains("restack complete"));
}

#[test]
fn restack_records_state_when_rebase_conflicts() {
    let repo = TestRepo::new();

    repo.commit_file("conflict.txt", "base\n", "add conflict file");
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("conflict.txt", "parent\n", "parent edits conflict file");

    repo.stack().args(["new", "feature/b"]).assert().success();
    // The child's own commit touches the conflicting file, so the conflict
    // survives base-aware restacking (a parent rewrite alone no longer
    // conflicts: only the child's own commits are replayed).
    repo.commit_file("conflict.txt", "parent\nchild\n", "child edits same file");

    repo.git(["switch", "feature/a"]);
    repo.git(["reset", "--hard", "HEAD~1"]);
    repo.commit_file(
        "conflict.txt",
        "updated parent\n",
        "update parent differently",
    );

    repo.stack()
        .arg("restack")
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "resolve conflicts, then run `git stk continue`",
        ));

    let state = fs::read_to_string(repo.path().join(".git/stack-state")).expect("read stack state");
    assert!(state.contains("branch=feature/b\n"));
    assert!(state.contains("parent=feature/a\n"));
    assert!(state.contains("updateRefs="));
    assert!(state.contains("remaining=\n"));

    let rebase_head = repo.git_status(["rev-parse", "--verify", "REBASE_HEAD"]);
    assert!(rebase_head.status.success(), "expected active rebase");

    repo.stack().arg("abort").assert().success();
    assert!(!repo.path().join(".git/stack-state").exists());
}

#[test]
fn continue_resumes_restack_after_conflict_resolution() {
    let repo = TestRepo::new();

    repo.commit_file("conflict.txt", "base\n", "add conflict file");
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("conflict.txt", "parent\n", "parent edits conflict file");

    repo.stack().args(["new", "feature/b"]).assert().success();
    // Conflict must come from the child's own commit; see
    // restack_records_state_when_rebase_conflicts.
    repo.commit_file("conflict.txt", "parent\nchild\n", "child edits same file");

    repo.git(["switch", "feature/a"]);
    repo.git(["reset", "--hard", "HEAD~1"]);
    repo.commit_file(
        "conflict.txt",
        "updated parent\n",
        "update parent differently",
    );

    repo.stack().arg("restack").assert().failure();

    repo.write("conflict.txt", "updated parent\nchild\n");
    repo.git(["add", "conflict.txt"]);

    repo.stack()
        .arg("continue")
        .assert()
        .success()
        .stdout(predicates::str::contains("restack complete"));

    assert!(!repo.path().join(".git/stack-state").exists());

    let parent_head = repo.git(["rev-parse", "feature/a"]);
    let merge_base = repo.git(["merge-base", "feature/a", "feature/b"]);
    assert_eq!(merge_base, parent_head);
    assert_eq!(repo.git(["branch", "--show-current"]), "feature/b");

    let conflict_file = fs::read_to_string(repo.path().join("conflict.txt")).expect("read file");
    assert_eq!(conflict_file, "updated parent\nchild\n");

    // continue must refresh the recorded fork point to the new parent tip
    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkBase"]),
        parent_head
    );
}

#[test]
fn provider_detects_github_https_remote() {
    let repo = TestRepo::new();
    repo.git([
        "remote",
        "add",
        "origin",
        "https://github.com/lararosekelley/git-stk.git",
    ]);

    repo.stack()
        .arg("provider")
        .assert()
        .success()
        .stdout("github (remote origin (https://github.com/lararosekelley/git-stk.git))\n");
}

#[test]
fn provider_detects_github_ssh_remote() {
    let repo = TestRepo::new();
    repo.git([
        "remote",
        "add",
        "origin",
        "git@github.com:lararosekelley/git-stk.git",
    ]);

    repo.stack()
        .arg("provider")
        .assert()
        .success()
        .stdout("github (remote origin (git@github.com:lararosekelley/git-stk.git))\n");
}

#[test]
fn provider_detects_gitlab_https_remote() {
    let repo = TestRepo::new();
    repo.git([
        "remote",
        "add",
        "origin",
        "https://gitlab.com/lararosekelley/git-stk-mirror.git",
    ]);

    repo.stack()
        .arg("provider")
        .assert()
        .success()
        .stdout("gitlab (remote origin (https://gitlab.com/lararosekelley/git-stk-mirror.git))\n");
}

#[test]
fn provider_detects_gitlab_ssh_remote() {
    let repo = TestRepo::new();
    repo.git([
        "remote",
        "add",
        "origin",
        "git@gitlab.com:lararosekelley/git-stk-mirror.git",
    ]);

    repo.stack()
        .arg("provider")
        .assert()
        .success()
        .stdout("gitlab (remote origin (git@gitlab.com:lararosekelley/git-stk-mirror.git))\n");
}

#[test]
fn provider_config_override_wins() {
    let repo = TestRepo::new();
    repo.git([
        "remote",
        "add",
        "origin",
        "https://github.com/lararosekelley/git-stk.git",
    ]);
    repo.git(["config", "stk.provider", "gitlab"]);

    repo.stack()
        .arg("provider")
        .assert()
        .success()
        .stdout("gitlab (config)\n");
}

#[test]
fn provider_rejects_invalid_config() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "bitbucket"]);

    repo.stack()
        .arg("provider")
        .assert()
        .failure()
        .stderr(predicates::str::contains("unsupported stk.provider value"));
}

#[test]
fn status_prints_local_stack_and_review_state() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.git(["switch", "-c", "feature/a"]);
    repo.git(["switch", "-c", "feature/b"]);
    repo.git(["config", "branch.feature/b.stkParent", "feature/a"]);
    repo.git(["switch", "-c", "feature/c"]);
    repo.git(["config", "branch.feature/c.stkParent", "feature/b"]);
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
cat <<'JSON'
[{"number":13,"state":"OPEN","baseRefName":"feature/a","headRefName":"feature/b","url":"https://github.com/lararosekelley/git-stk/pull/13"}]
JSON
"##,
    );

    repo.stack()
        .args(["status", "feature/b"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains("branch: feature/b"))
        .stdout(predicates::str::contains("parent: feature/a"))
        .stdout(predicates::str::contains("children: feature/c"))
        .stdout(predicates::str::contains("provider: github (config)"))
        .stdout(predicates::str::contains(
            "review: #13 open feature/b -> feature/a",
        ))
        .stdout(predicates::str::contains(
            "url: https://github.com/lararosekelley/git-stk/pull/13",
        ));
}

#[test]
fn status_prints_none_when_review_is_missing() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.git(["config", "branch.feature/b.stkParent", "feature/a"]);
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
printf '[]\n'
"##,
    );

    repo.stack()
        .args(["status", "feature/b"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains("branch: feature/b"))
        .stdout(predicates::str::contains("parent: feature/a"))
        .stdout(predicates::str::contains("children: none"))
        .stdout(predicates::str::contains("review: none"));
}

#[test]
fn status_warns_when_review_base_differs_from_parent() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "gitlab"]);
    repo.git(["config", "branch.feature/b.stkParent", "feature/a"]);
    let path = repo.fake_cli(
        "glab",
        r##"#!/usr/bin/env sh
cat <<'JSON'
[{"iid":34,"state":"opened","target_branch":"main","source_branch":"feature/b","web_url":"https://gitlab.com/lararosekelley/git-stk-mirror/-/merge_requests/34"}]
JSON
"##,
    );

    repo.stack()
        .args(["status", "feature/b"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "review: !34 open feature/b -> main",
        ))
        .stdout(predicates::str::contains(
            "warning: review base is main, local parent is feature/a",
        ));
}

#[test]
fn review_reads_github_pr_for_current_branch() {
    let repo = TestRepo::new();
    repo.git([
        "remote",
        "add",
        "origin",
        "git@github.com:lararosekelley/git-stk",
    ]);
    repo.git(["switch", "-c", "feature/b"]);
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
cat <<'JSON'
[{"number":12,"state":"OPEN","baseRefName":"feature/a","headRefName":"feature/b","url":"https://github.com/lararosekelley/git-stk/pull/12"}]
JSON
"##,
    );

    repo.stack()
        .arg("review")
        .env("PATH", path)
        .assert()
        .success()
        .stdout(
            "#12 feature/b -> feature/a open https://github.com/lararosekelley/git-stk/pull/12\n",
        );
}

#[test]
fn review_reads_gitlab_mr_for_explicit_branch() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "gitlab"]);
    let path = repo.fake_cli(
        "glab",
        r##"#!/usr/bin/env sh
cat <<'JSON'
[{"iid":34,"state":"opened","target_branch":"feature/a","source_branch":"feature/b","web_url":"https://gitlab.com/lararosekelley/git-stk-mirror/-/merge_requests/34"}]
JSON
"##,
    );

    repo.stack()
        .args(["review", "feature/b"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout("!34 feature/b -> feature/a open https://gitlab.com/lararosekelley/git-stk-mirror/-/merge_requests/34\n");
}

#[test]
fn review_reports_when_no_review_exists() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
cat <<'JSON'
[]
JSON
"##,
    );

    repo.stack()
        .args(["review", "feature/b"])
        .env("PATH", path)
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "no github review found for feature/b",
        ));
}

#[test]
fn sync_sets_parent_from_github_pr_base() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.git(["switch", "-c", "feature/b"]);
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
cat <<'JSON'
[{"number":12,"state":"OPEN","baseRefName":"feature/a","headRefName":"feature/b","url":"https://github.com/lararosekelley/git-stk/pull/12"}]
JSON
"##,
    );

    repo.stack()
        .args(["sync", "feature/b"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "synced feature/b -> feature/a (#12)",
        ));

    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkParent"]),
        "feature/a"
    );
}

#[test]
fn sync_dry_run_reports_parent_without_writing_config() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.git(["switch", "-c", "feature/b"]);
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
cat <<'JSON'
[{"number":12,"state":"OPEN","baseRefName":"feature/a","headRefName":"feature/b","url":"https://github.com/lararosekelley/git-stk/pull/12"}]
JSON
"##,
    );

    repo.stack()
        .args(["sync", "--dry-run", "feature/b"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "would sync feature/b -> feature/a (#12)",
        ))
        .stdout(predicates::str::contains(
            "sync complete: 1 would be synced, 0 skipped",
        ));

    assert_eq!(
        repo.git_status(["config", "--get", "branch.feature/b.stkParent"])
            .status
            .code(),
        Some(1)
    );
}

#[test]
fn sync_sets_parent_from_gitlab_mr_target() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "gitlab"]);
    let path = repo.fake_cli(
        "glab",
        r##"#!/usr/bin/env sh
cat <<'JSON'
[{"iid":34,"state":"opened","target_branch":"feature/a","source_branch":"feature/b","web_url":"https://gitlab.com/lararosekelley/git-stk-mirror/-/merge_requests/34"}]
JSON
"##,
    );

    repo.stack()
        .args(["sync", "feature/b"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "synced feature/b -> feature/a (!34)",
        ));

    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkParent"]),
        "feature/a"
    );
}

#[test]
fn sync_all_local_branches_skips_branches_without_reviews() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.git(["switch", "-c", "feature/a"]);
    repo.git(["switch", "main"]);
    repo.git(["switch", "-c", "feature/b"]);
    repo.git(["switch", "main"]);
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  *feature/b*)
    cat <<'JSON'
[{"number":12,"state":"OPEN","baseRefName":"feature/a","headRefName":"feature/b","url":"https://github.com/lararosekelley/git-stk/pull/12"}]
JSON
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
    );

    repo.stack()
        .arg("sync")
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "synced feature/b -> feature/a (#12)",
        ))
        .stdout(predicates::str::contains("sync complete: 1 synced"));

    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkParent"]),
        "feature/a"
    );
}

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
fn cleanup_retargets_children_and_detaches_merged_branch() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.stack().args(["new", "feature/b"]).assert().success();
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  *feature/a\ --state\ merged*)
    cat <<'JSON'
[{"number":12,"state":"MERGED","baseRefName":"main","headRefName":"feature/a","url":"https://github.com/lararosekelley/git-stk/pull/12"}]
JSON
    ;;
  *feature/a*)
    printf '[]\n'
    ;;
  *feature/b*)
    cat <<'JSON'
[{"number":13,"state":"OPEN","baseRefName":"feature/a","headRefName":"feature/b","url":"https://github.com/lararosekelley/git-stk/pull/13"}]
JSON
    ;;
  pr\ edit*)
    printf 'updated child review\n'
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
    );

    repo.stack()
        .args(["cleanup", "feature/a"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains("will retarget feature/b -> main"))
        .stdout(predicates::str::contains(
            "will update review feature/b -> main (#13)",
        ))
        .stdout(predicates::str::contains("updated child review"))
        .stdout(predicates::str::contains("will detach feature/a"))
        .stdout(predicates::str::contains(
            "skipped feature/b: review #13 is open",
        ))
        .stdout(predicates::str::contains(
            "cleanup complete: 1 cleaned, 1 skipped",
        ));

    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkParent"]),
        "main"
    );
    assert_eq!(
        repo.git_status(["config", "--get", "branch.feature/a.stkParent"])
            .status
            .code(),
        Some(1)
    );
}

#[test]
fn cleanup_dry_run_leaves_stack_metadata_unchanged() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.stack().args(["new", "feature/b"]).assert().success();
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  *feature/a\ --state\ merged*)
    cat <<'JSON'
[{"number":12,"state":"MERGED","baseRefName":"main","headRefName":"feature/a","url":"https://github.com/lararosekelley/git-stk/pull/12"}]
JSON
    ;;
  *feature/a*)
    printf '[]\n'
    ;;
  *feature/b*)
    cat <<'JSON'
[{"number":13,"state":"OPEN","baseRefName":"feature/a","headRefName":"feature/b","url":"https://github.com/lararosekelley/git-stk/pull/13"}]
JSON
    ;;
  pr\ edit*)
    echo "dry-run should not edit review" >&2
    exit 1
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
    );

    repo.stack()
        .args(["cleanup", "--dry-run", "feature/a"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "would retarget feature/b -> main",
        ))
        .stdout(predicates::str::contains(
            "would update review feature/b -> main (#13)",
        ))
        .stdout(predicates::str::contains("would detach feature/a"));

    assert_eq!(
        repo.git(["config", "--get", "branch.feature/a.stkParent"]),
        "main"
    );
    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkParent"]),
        "feature/a"
    );
}

#[test]
fn cleanup_delete_branch_removes_cleaned_merged_branch() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.stack().args(["new", "feature/a"]).assert().success();
    // Real commits + a squash merge: feature/a's commits are NOT
    // ancestry-merged into main afterwards, so `git branch -d` would refuse.
    // Deletion must trust the provider-verified merge state instead.
    repo.commit_file("a.txt", "one\n", "parent change one");
    repo.commit_file("a.txt", "one\ntwo\n", "parent change two");
    repo.stack().args(["new", "feature/b"]).assert().success();
    repo.git(["switch", "main"]);
    repo.git(["merge", "--squash", "feature/a"]);
    repo.git(["commit", "-m", "parent changes (#12)"]);
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  *feature/a\ --state\ merged*)
    cat <<'JSON'
[{"number":12,"state":"MERGED","baseRefName":"main","headRefName":"feature/a","url":"https://github.com/lararosekelley/git-stk/pull/12"}]
JSON
    ;;
  *feature/a*)
    printf '[]\n'
    ;;
  *feature/b*)
    cat <<'JSON'
[{"number":13,"state":"OPEN","baseRefName":"feature/a","headRefName":"feature/b","url":"https://github.com/lararosekelley/git-stk/pull/13"}]
JSON
    ;;
  pr\ edit*)
    printf 'updated child review\n'
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
    );

    repo.stack()
        .args(["cleanup", "--delete-branch", "feature/a"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains("will delete branch feature/a"))
        .stdout(predicates::str::contains(
            "cleanup complete: 1 cleaned, 1 skipped",
        ));

    assert!(
        !repo
            .git(["branch", "--list", "feature/a"])
            .contains("feature/a")
    );
    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkParent"]),
        "main"
    );
}

#[test]
fn cleanup_delete_branch_dry_run_keeps_branch() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.git(["switch", "main"]);
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  *--state\ merged*)
    cat <<'JSON'
[{"number":12,"state":"MERGED","baseRefName":"main","headRefName":"feature/a","url":"https://github.com/lararosekelley/git-stk/pull/12"}]
JSON
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
    );

    repo.stack()
        .args(["cleanup", "--dry-run", "--delete-branch", "feature/a"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains("would delete branch feature/a"));

    assert!(
        repo.git(["branch", "--list", "feature/a"])
            .contains("feature/a")
    );
    assert_eq!(
        repo.git(["config", "--get", "branch.feature/a.stkParent"]),
        "main"
    );
}

#[test]
fn cleanup_delete_branch_refuses_current_branch() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);
    repo.stack().args(["new", "feature/a"]).assert().success();
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  *--state\ merged*)
    cat <<'JSON'
[{"number":12,"state":"MERGED","baseRefName":"main","headRefName":"feature/a","url":"https://github.com/lararosekelley/git-stk/pull/12"}]
JSON
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
    );

    repo.stack()
        .args(["cleanup", "--delete-branch", "feature/a"])
        .env("PATH", path)
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "refusing to delete currently checked out branch feature/a",
        ));

    assert!(
        repo.git(["branch", "--list", "feature/a"])
            .contains("feature/a")
    );
}

#[test]
fn upgrade_head_cancels_when_not_confirmed() {
    let repo = TestRepo::new();
    let path = repo.fake_cli(
        "cargo",
        r##"#!/usr/bin/env sh
touch cargo-ran.txt
"##,
    );

    repo.stack()
        .args(["upgrade", "--head"])
        .env("PATH", path)
        .write_stdin("n\n")
        .assert()
        .success()
        .stdout(predicates::str::contains("upgrade cancelled"));

    assert!(!repo.path().join("cargo-ran.txt").exists());
}

#[test]
fn upgrade_head_warns_and_runs_cargo_install_when_confirmed() {
    let repo = TestRepo::new();
    repo.fake_cli(
        "cargo",
        r##"#!/usr/bin/env sh
printf '%s ' "$@" > cargo-args.txt
"##,
    );
    // Stub the freshly installed binary so the post-upgrade asset refresh
    // never reaches a real git-stk install.
    let path = repo.fake_cli(
        "git-stk",
        r##"#!/usr/bin/env sh
exit 0
"##,
    );

    repo.stack()
        .args(["upgrade", "--head"])
        .env("PATH", path)
        .write_stdin("y\n")
        .assert()
        .success()
        .stdout(predicates::str::contains("pre-release"))
        .stdout(predicates::str::contains(
            "to return to the latest release, run: git stk upgrade --force",
        ));

    let recorded =
        fs::read_to_string(repo.path().join("cargo-args.txt")).expect("cargo args recorded");
    assert_eq!(
        recorded.trim(),
        "install --git https://github.com/lararosekelley/git-stk --locked git-stk"
    );
}

#[test]
fn upgrade_head_yes_skips_confirmation_prompt() {
    let repo = TestRepo::new();
    repo.fake_cli(
        "cargo",
        r##"#!/usr/bin/env sh
printf '%s ' "$@" > cargo-args.txt
"##,
    );
    let path = repo.fake_cli(
        "git-stk",
        r##"#!/usr/bin/env sh
exit 0
"##,
    );

    repo.stack()
        .args(["upgrade", "--head", "--yes"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains("installed git-stk from HEAD"));

    assert!(repo.path().join("cargo-args.txt").exists());
}

#[test]
fn upgrade_head_reports_cargo_install_failure() {
    let repo = TestRepo::new();
    let path = repo.fake_cli(
        "cargo",
        r##"#!/usr/bin/env sh
exit 1
"##,
    );

    repo.stack()
        .args(["upgrade", "--head", "--yes"])
        .env("PATH", path)
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "cargo install exited with status",
        ));
}

#[test]
fn upgrade_yes_requires_head() {
    let repo = TestRepo::new();

    repo.stack().args(["upgrade", "--yes"]).assert().failure();
}

#[test]
fn upgrade_head_conflicts_with_force() {
    let repo = TestRepo::new();

    repo.stack()
        .args(["upgrade", "--head", "--force"])
        .assert()
        .failure();
}

#[test]
fn upgrade_without_receipt_suggests_cargo_install() {
    let repo = TestRepo::new();
    let empty = repo.path().join("no-receipt");
    fs::create_dir_all(&empty).expect("create empty receipt dir");

    repo.stack()
        .args(["upgrade"])
        .env("AXOUPDATER_CONFIG_PATH", &empty)
        .assert()
        .failure()
        .stderr(predicates::str::contains("cargo install git-stk --locked"));
}

#[test]
fn completions_bash_includes_git_subcommand_shim() {
    let repo = TestRepo::new();

    repo.stack()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "-F _clap_complete_git_stk git-stk",
        ))
        .stdout(predicates::str::contains("_git_stk() {"));
}

#[test]
fn completions_zsh_emits_compdef_and_git_shim() {
    let repo = TestRepo::new();

    repo.stack()
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(predicates::str::contains("#compdef git-stk"))
        .stdout(predicates::str::contains("function _git-stk() {"));
}

#[test]
fn completions_rejects_unknown_shell() {
    let repo = TestRepo::new();

    repo.stack()
        .args(["completions", "tcsh"])
        .assert()
        .failure();
}

#[test]
fn completions_bash_shim_completes_git_stk_subcommands() {
    let repo = TestRepo::new();

    let output = repo.stack_output(["completions", "bash"]).stdout;
    let script_path = repo.path().join("completions.bash");
    fs::write(&script_path, output).expect("write completions script");

    // Simulate `git stk re<TAB>` the way git's completion would: set up the
    // completion environment and invoke the _git_stk shim directly.
    let harness = format!(
        r#"source "{}"
COMP_WORDS=(git stk re)
COMP_CWORD=2
_git_stk
printf '%s\n' "${{COMPREPLY[@]}}"
"#,
        script_path.display()
    );
    let result = Command::new("bash")
        .args(["-c", &harness])
        .output()
        .expect("run bash completion harness");

    assert!(result.status.success());
    let completions = String::from_utf8_lossy(&result.stdout);
    assert!(
        completions.contains("restack") && completions.contains("review"),
        "expected restack/review completions for `git stk re`, got: {completions}"
    );
}

#[test]
fn setup_installs_man_page_and_wires_bashrc() {
    let repo = TestRepo::new();
    let home = repo.path().join("home");
    fs::create_dir_all(&home).expect("create fake home");

    repo.stack()
        .args(["setup"])
        .env("HOME", &home)
        .env_remove("XDG_DATA_HOME")
        .env("SHELL", "/bin/bash")
        .write_stdin("y\n")
        .assert()
        .success()
        .stdout(predicates::str::contains("installed man page"))
        .stdout(predicates::str::contains("added bash completion setup"));

    assert!(home.join(".local/share/man/man1/git-stk.1").exists());
    let rc = fs::read_to_string(home.join(".bashrc")).expect("read bashrc");
    assert!(rc.contains("source <(git stk completions bash)"));
}

#[test]
fn setup_is_idempotent_for_completions() {
    let repo = TestRepo::new();
    let home = repo.path().join("home");
    fs::create_dir_all(&home).expect("create fake home");

    for _ in 0..2 {
        repo.stack()
            .args(["setup", "--yes"])
            .env("HOME", &home)
            .env("SHELL", "/bin/zsh")
            .assert()
            .success();
    }

    let rc = fs::read_to_string(home.join(".zshrc")).expect("read zshrc");
    assert_eq!(rc.matches("git stk completions zsh").count(), 1);
}

#[test]
fn setup_declining_prompt_skips_rc_but_installs_man_page() {
    let repo = TestRepo::new();
    let home = repo.path().join("home");
    fs::create_dir_all(&home).expect("create fake home");

    repo.stack()
        .args(["setup"])
        .env("HOME", &home)
        .env("SHELL", "/bin/bash")
        .write_stdin("n\n")
        .assert()
        .success()
        .stdout(predicates::str::contains("skipped completion setup"))
        .stdout(predicates::str::contains(
            "source <(git stk completions bash)",
        ));

    assert!(home.join(".local/share/man/man1/git-stk.1").exists());
    assert!(!home.join(".bashrc").exists());
}

#[test]
fn setup_unknown_shell_prints_manual_hint() {
    let repo = TestRepo::new();
    let home = repo.path().join("home");
    fs::create_dir_all(&home).expect("create fake home");

    repo.stack()
        .args(["setup", "--yes"])
        .env("HOME", &home)
        .env("SHELL", "/bin/tcsh")
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "could not detect a supported shell",
        ));

    assert!(!home.join(".bashrc").exists());
}

#[test]
fn setup_respects_xdg_data_home_for_man_page() {
    let repo = TestRepo::new();
    let home = repo.path().join("home");
    let data = repo.path().join("xdg-data");
    fs::create_dir_all(&home).expect("create fake home");

    repo.stack()
        .args(["setup", "--yes"])
        .env("HOME", &home)
        .env("XDG_DATA_HOME", &data)
        .env("SHELL", "/bin/bash")
        .assert()
        .success();

    assert!(data.join("man/man1/git-stk.1").exists());
    assert!(!home.join(".local/share/man/man1/git-stk.1").exists());
}

#[test]
fn setup_refresh_installs_man_page_without_touching_rc() {
    let repo = TestRepo::new();
    let home = repo.path().join("home");
    fs::create_dir_all(&home).expect("create fake home");

    repo.stack()
        .args(["setup", "--refresh"])
        .env("HOME", &home)
        .env_remove("XDG_DATA_HOME")
        .env("SHELL", "/bin/bash")
        .assert()
        .success()
        .stdout(predicates::str::contains("installed man page"));

    assert!(home.join(".local/share/man/man1/git-stk.1").exists());
    assert!(!home.join(".bashrc").exists());
}

#[test]
fn upgrade_head_refreshes_assets_with_new_binary() {
    let repo = TestRepo::new();
    repo.fake_cli(
        "cargo",
        r##"#!/usr/bin/env sh
exit 0
"##,
    );
    // Fake the freshly installed binary: upgrade must invoke it (not itself)
    // so refreshed assets match the new version.
    let path = repo.fake_cli(
        "git-stk",
        r##"#!/usr/bin/env sh
printf '%s ' "$@" > stk-args.txt
"##,
    );

    repo.stack()
        .args(["upgrade", "--head", "--yes"])
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains("installed git-stk from HEAD"));

    let recorded =
        fs::read_to_string(repo.path().join("stk-args.txt")).expect("fake git-stk args recorded");
    assert_eq!(recorded.trim(), "setup --refresh");
}

#[test]
fn upgrade_head_warns_when_asset_refresh_fails() {
    let repo = TestRepo::new();
    let path = repo.fake_cli(
        "cargo",
        r##"#!/usr/bin/env sh
exit 0
"##,
    );
    repo.fake_cli(
        "git-stk",
        r##"#!/usr/bin/env sh
exit 1
"##,
    );

    repo.stack()
        .args(["upgrade", "--head", "--yes"])
        .env("PATH", path)
        .assert()
        .success()
        .stderr(predicates::str::contains(
            "failed to refresh generated assets",
        ));
}

#[test]
fn restack_after_squash_merge_replays_only_child_commits() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);

    // Parent stack: two commits on feature/a so git's patch-id auto-skip
    // cannot save a naive rebase after the squash merge.
    repo.commit_file("shared.txt", "base\n", "add shared file");
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("shared.txt", "base\none\n", "parent change one");
    repo.commit_file("shared.txt", "base\none\ntwo\n", "parent change two");

    repo.stack().args(["new", "feature/b"]).assert().success();
    repo.commit_file("shared.txt", "base\none\ntwo\nchild\n", "child change");

    // Simulate GitHub squash-merging feature/a into main.
    repo.git(["switch", "main"]);
    repo.git(["merge", "--squash", "feature/a"]);
    repo.git(["commit", "-m", "parent changes (#1)"]);

    // cleanup: feature/a merged -> retarget feature/b to main, record fork point.
    let old_parent_tip = repo.git(["rev-parse", "feature/a"]);
    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  *feature/a\ --state\ merged*)
    cat <<'JSON'
[{"number":1,"state":"MERGED","baseRefName":"main","headRefName":"feature/a","url":"https://github.com/lararosekelley/git-stk/pull/1"}]
JSON
    ;;
  *feature/a*)
    printf '[]\n'
    ;;
  *feature/b*)
    cat <<'JSON'
[{"number":2,"state":"OPEN","baseRefName":"feature/a","headRefName":"feature/b","url":"https://github.com/lararosekelley/git-stk/pull/2"}]
JSON
    ;;
  pr\ edit*)
    printf 'updated child review\n'
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
    );

    repo.stack()
        .args(["cleanup", "feature/a"])
        .env("PATH", path)
        .assert()
        .success();

    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkParent"]),
        "main"
    );
    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkBase"]),
        old_parent_tip
    );

    // Without the recorded fork point this rebase replays the parent's
    // commits onto the squashed main and conflicts. With it, only the
    // child's own commit replays, cleanly.
    repo.git(["switch", "feature/b"]);
    repo.stack()
        .arg("restack")
        .assert()
        .success()
        .stdout(predicates::str::contains("restack complete"));

    let own_commits = repo.git(["rev-list", "--count", "main..feature/b"]);
    assert_eq!(own_commits, "1");
    let shared = fs::read_to_string(repo.path().join("shared.txt")).expect("read shared file");
    assert_eq!(shared, "base\none\ntwo\nchild\n");
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
fn restack_falls_back_to_plain_rebase_when_base_is_invalid() {
    let repo = TestRepo::new();

    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "feature work");
    repo.git([
        "config",
        "branch.feature/a.stkBase",
        "0000000000000000000000000000000000000000",
    ]);

    repo.git(["switch", "main"]);
    repo.commit_file("main.txt", "main\n", "main moves on");
    repo.git(["switch", "feature/a"]);

    repo.stack()
        .arg("restack")
        .assert()
        .success()
        .stdout(predicates::str::contains("restack complete"));

    let merge_base = repo.git(["merge-base", "main", "feature/a"]);
    assert_eq!(merge_base, repo.git(["rev-parse", "main"]));
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
        bottom.contains("Stack managed by [git-stk](https://github.com/lararosekelley/git-stk)")
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
fn version_flag_prints_name_and_version() {
    let repo = TestRepo::new();

    repo.stack()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicates::str::contains(concat!(
            "git-stk ",
            env!("CARGO_PKG_VERSION")
        )));
}

impl TestRepo {
    /// Create a bare repo, add it as origin, and push the given branches.
    fn add_bare_origin(&self, branches: &[&str]) -> TempDir {
        let bare = tempfile::tempdir().expect("create bare remote");
        Command::new("git")
            .args(["init", "--bare", "--initial-branch", "main"])
            .arg(bare.path())
            .output()
            .expect("init bare remote");

        self.git(["remote", "add", "origin", bare.path().to_str().unwrap()]);
        for branch in branches {
            self.git(["push", "-u", "origin", branch]);
        }
        bare
    }

    fn remote_sha(&self, bare: &TempDir, branch: &str) -> String {
        let output = Command::new("git")
            .args(["rev-parse", branch])
            .current_dir(bare.path())
            .output()
            .expect("rev-parse remote branch");
        assert!(output.status.success(), "remote branch {branch} missing");
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }
}

#[test]
fn restack_push_flag_pushes_rewritten_branches() {
    let repo = TestRepo::new();

    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "parent change");
    repo.stack().args(["new", "feature/b"]).assert().success();
    repo.commit_file("b.txt", "b\n", "child change");

    let bare = repo.add_bare_origin(&["main", "feature/a", "feature/b"]);

    repo.git(["switch", "feature/a"]);
    repo.commit_file("a2.txt", "a2\n", "parent moves");

    repo.stack()
        .args(["restack", "--push"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "pushed feature/a feature/b to origin",
        ));

    assert_eq!(
        repo.remote_sha(&bare, "feature/b"),
        repo.git(["rev-parse", "feature/b"])
    );
}

#[test]
fn restack_prints_push_hint_when_not_pushing() {
    let repo = TestRepo::new();

    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "parent change");
    repo.stack().args(["new", "feature/b"]).assert().success();
    repo.commit_file("b.txt", "b\n", "child change");

    let bare = repo.add_bare_origin(&["main", "feature/a", "feature/b"]);
    let stale = repo.remote_sha(&bare, "feature/b");

    repo.git(["switch", "feature/a"]);
    repo.commit_file("a2.txt", "a2\n", "parent moves");

    repo.stack()
        .arg("restack")
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "git push --force-with-lease origin feature/a feature/b",
        ));

    // No push happened.
    assert_eq!(repo.remote_sha(&bare, "feature/b"), stale);
}

#[test]
fn restack_push_respects_config_and_no_push_overrides_it() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.pushOnRestack", "true"]);

    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "parent change");
    repo.stack().args(["new", "feature/b"]).assert().success();
    repo.commit_file("b.txt", "b\n", "child change");

    let bare = repo.add_bare_origin(&["main", "feature/a", "feature/b"]);

    repo.git(["switch", "feature/a"]);
    repo.commit_file("a2.txt", "a2\n", "parent moves");

    // Config enables the push.
    repo.stack().arg("restack").assert().success();
    assert_eq!(
        repo.remote_sha(&bare, "feature/b"),
        repo.git(["rev-parse", "feature/b"])
    );

    // --no-push overrides the config.
    repo.git(["switch", "feature/a"]);
    repo.commit_file("a3.txt", "a3\n", "parent moves again");
    let before = repo.remote_sha(&bare, "feature/b");

    repo.stack()
        .args(["restack", "--no-push"])
        .assert()
        .success()
        .stdout(predicates::str::contains("remote branches may be stale"));
    assert_eq!(repo.remote_sha(&bare, "feature/b"), before);
}

#[test]
fn continue_after_conflict_pushes_all_restacked_branches() {
    let repo = TestRepo::new();

    repo.commit_file("conflict.txt", "base\n", "add conflict file");
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("conflict.txt", "parent\n", "parent edits conflict file");
    repo.stack().args(["new", "feature/b"]).assert().success();
    repo.commit_file("conflict.txt", "parent\nchild\n", "child edits same file");

    let bare = repo.add_bare_origin(&["main", "feature/a", "feature/b"]);

    repo.git(["switch", "feature/a"]);
    repo.git(["reset", "--hard", "HEAD~1"]);
    repo.commit_file(
        "conflict.txt",
        "updated parent\n",
        "update parent differently",
    );
    repo.git(["push", "--force-with-lease", "origin", "feature/a"]);

    repo.stack().args(["restack", "--push"]).assert().failure();

    repo.write("conflict.txt", "updated parent\nchild\n");
    repo.git(["add", "conflict.txt"]);

    repo.stack()
        .arg("continue")
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "pushed feature/a feature/b to origin",
        ));

    assert_eq!(
        repo.remote_sha(&bare, "feature/b"),
        repo.git(["rev-parse", "feature/b"])
    );
}

impl TestRepo {
    /// Run the bash completion harness: source the registration script, set
    /// up COMP_WORDS for `git stk <words...><TAB>`, invoke the _git_stk shim,
    /// and return COMPREPLY entries.
    fn complete_git_stk(&self, words: &[&str]) -> String {
        let output = self.stack_output(["completions", "bash"]).stdout;
        let script_path = self.path().join("completions.bash");
        fs::write(&script_path, output).expect("write completions script");

        let comp_words = words
            .iter()
            .map(|word| format!("\"{word}\""))
            .collect::<Vec<_>>()
            .join(" ");
        let harness = format!(
            r#"source "{}"
COMP_WORDS=(git stk {comp_words})
COMP_CWORD={}
_git_stk
printf '%s\n' "${{COMPREPLY[@]}}"
"#,
            script_path.display(),
            words.len() + 1,
        );

        let mut command = Command::new("bash");
        Self::isolate_git_config(&mut command);
        let result = command
            .args(["-c", &harness])
            .current_dir(self.path())
            .output()
            .expect("run bash completion harness");
        assert!(
            result.status.success(),
            "harness failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8_lossy(&result.stdout).into_owned()
    }
}

#[test]
fn completions_complete_flags_for_subcommands() {
    let repo = TestRepo::new();

    let completions = repo.complete_git_stk(&["submit", "--"]);
    assert!(
        completions.contains("--dry-run") && completions.contains("--stack"),
        "expected submit flags, got: {completions}"
    );
}

#[test]
fn completions_complete_only_children_for_up() {
    let repo = TestRepo::new();

    repo.stack()
        .args(["new", "feature/alpha"])
        .assert()
        .success();
    repo.stack()
        .args(["new", "feature/beta"])
        .assert()
        .success();
    repo.git(["switch", "feature/alpha"]);

    let completions = repo.complete_git_stk(&["up", ""]);
    assert!(
        completions.contains("feature/beta"),
        "expected child branch, got: {completions}"
    );
    assert!(
        !completions.contains("main"),
        "up must not offer non-children, got: {completions}"
    );
}

#[test]
fn completions_complete_branch_names_with_prefix() {
    let repo = TestRepo::new();

    repo.stack()
        .args(["new", "feature/alpha"])
        .assert()
        .success();
    repo.git(["switch", "main"]);

    let completions = repo.complete_git_stk(&["status", "feat"]);
    assert!(
        completions.contains("feature/alpha"),
        "expected branch completion, got: {completions}"
    );
    assert!(
        !completions.contains("main"),
        "prefix must filter branches, got: {completions}"
    );
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
fn config_shows_defaults_and_branch_metadata() {
    let repo = TestRepo::new();

    repo.stack()
        .arg("config")
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "stk.provider (default: auto-detect from the remote URL)",
        ))
        .stdout(predicates::str::contains("stk.remote (default: origin)"))
        .stdout(predicates::str::contains("stk.updateRefs (default: false)"))
        .stdout(predicates::str::contains(
            "no branch metadata (no stacked branches)",
        ));

    repo.git(["config", "stk.pushOnRestack", "true"]);
    repo.stack().args(["new", "feature/a"]).assert().success();

    repo.stack()
        .arg("config")
        .assert()
        .success()
        .stdout(predicates::str::contains("stk.pushOnRestack = true"))
        .stdout(predicates::str::contains(
            "branch.feature/a.stkparent = main",
        ))
        .stdout(predicates::str::contains("branch.feature/a.stkbase = "));
}

#[test]
fn restack_ignores_rebase_update_refs_git_config() {
    let repo = TestRepo::new();
    if !repo.supports_update_refs() {
        return;
    }
    // Git's own config must no longer influence restack; only stk.updateRefs.
    repo.git(["config", "rebase.updateRefs", "true"]);

    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "parent change");
    repo.stack().args(["new", "feature/b"]).assert().success();
    repo.commit_file("b.txt", "b\n", "child change");
    repo.git(["switch", "feature/a"]);
    repo.commit_file("a2.txt", "a2\n", "parent moves");

    let output = repo.stack_output(["restack"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("--update-refs"),
        "rebase.updateRefs must not enable --update-refs: {stdout}"
    );
}

#[test]
fn repair_reconstructs_wiped_stack_from_ancestry() {
    let repo = TestRepo::new();

    // Build a 3-branch stack with real commits, then wipe all metadata -
    // the config-blown-away incident.
    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "a work");
    repo.stack().args(["new", "feature/b"]).assert().success();
    repo.commit_file("b.txt", "b\n", "b work");
    repo.stack().args(["new", "feature/c"]).assert().success();
    repo.commit_file("c.txt", "c\n", "c work");

    for branch in ["feature/a", "feature/b", "feature/c"] {
        repo.git(["config", "--unset", &format!("branch.{branch}.stkParent")]);
        repo.git(["config", "--unset", &format!("branch.{branch}.stkBase")]);
    }

    repo.stack()
        .arg("repair")
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "feature/a: set parent main (from ancestry)",
        ))
        .stdout(predicates::str::contains(
            "feature/b: set parent feature/a (from ancestry)",
        ))
        .stdout(predicates::str::contains(
            "feature/c: set parent feature/b (from ancestry)",
        ))
        .stdout(predicates::str::contains(
            "repair complete: 3 repaired, 0 verified, 0 unresolved",
        ));

    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkParent"]),
        "feature/a"
    );
    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkBase"]),
        repo.git(["rev-parse", "feature/a"])
    );
    // Trunk must never be assigned a parent.
    assert_eq!(
        repo.git_status(["config", "--get", "branch.main.stkParent"])
            .status
            .code(),
        Some(1)
    );
}

#[test]
fn repair_prefers_provider_review_base_over_ancestry() {
    let repo = TestRepo::new();
    repo.git(["config", "stk.provider", "github"]);

    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "a work");
    repo.stack().args(["new", "feature/b"]).assert().success();
    repo.commit_file("b.txt", "b\n", "b work");
    repo.git(["config", "--unset", "branch.feature/b.stkParent"]);
    repo.git(["config", "--unset", "branch.feature/b.stkBase"]);

    let path = repo.fake_cli(
        "gh",
        r##"#!/usr/bin/env sh
case "$*" in
  *feature/b*)
    cat <<'JSON'
[{"number":7,"state":"OPEN","baseRefName":"feature/a","headRefName":"feature/b","url":"https://github.com/owner/repo/pull/7"}]
JSON
    ;;
  *)
    printf '[]\n'
    ;;
esac
"##,
    );

    repo.stack()
        .arg("repair")
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "feature/b: set parent feature/a (from github review #7)",
        ));

    assert_eq!(
        repo.git(["config", "--get", "branch.feature/b.stkParent"]),
        "feature/a"
    );
}

#[test]
fn repair_re_records_stale_fork_point() {
    let repo = TestRepo::new();

    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "a work");
    repo.git([
        "config",
        "branch.feature/a.stkBase",
        "0000000000000000000000000000000000000000",
    ]);

    repo.stack()
        .arg("repair")
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "feature/a: re-recorded fork point from main",
        ))
        .stdout(predicates::str::contains("1 repaired, 0 verified"));

    assert_eq!(
        repo.git(["config", "--get", "branch.feature/a.stkBase"]),
        repo.git(["rev-parse", "main"])
    );
}

#[test]
fn repair_dry_run_changes_nothing() {
    let repo = TestRepo::new();

    repo.stack().args(["new", "feature/a"]).assert().success();
    repo.commit_file("a.txt", "a\n", "a work");
    repo.git(["config", "--unset", "branch.feature/a.stkParent"]);
    repo.git(["config", "--unset", "branch.feature/a.stkBase"]);

    repo.stack()
        .args(["repair", "--dry-run"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "feature/a: would set parent main (from ancestry)",
        ));

    assert_eq!(
        repo.git_status(["config", "--get", "branch.feature/a.stkParent"])
            .status
            .code(),
        Some(1)
    );
}

#[test]
fn repair_reports_unrepairable_branches() {
    let repo = TestRepo::new();

    // A branch with no commits of its own and equal tip to main: direction
    // is ambiguous, so repair must not guess.
    repo.git(["switch", "-c", "feature/empty"]);
    repo.git(["switch", "main"]);

    repo.stack()
        .arg("repair")
        .assert()
        .success()
        .stdout(predicates::str::contains("feature/empty: no parent found"))
        .stdout(predicates::str::contains(
            "0 repaired, 0 verified, 1 unresolved",
        ));
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
