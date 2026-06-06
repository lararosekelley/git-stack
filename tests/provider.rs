mod common;

use common::TestRepo;

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
