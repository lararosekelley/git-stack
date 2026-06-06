use std::fs;
mod common;

use common::TestRepo;

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
