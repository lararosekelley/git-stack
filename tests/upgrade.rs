// These suites drive sh-script provider fakes, so they are Unix-only.
#![cfg(unix)]

use std::fs;
mod common;

use common::TestRepo;

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
