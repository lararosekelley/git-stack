//! Shared integration-test harness.
#![allow(dead_code)]

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{env, fs, path::Path, process::Command};

use tempfile::TempDir;

pub struct TestRepo {
    dir: TempDir,
}

impl TestRepo {
    pub fn new() -> Self {
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

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn write(&self, path: &str, contents: &str) {
        fs::write(self.path().join(path), contents).expect("write test file");
    }

    /// Isolate a command from the developer's global and system git config so
    /// the suite stays hermetic (e.g. a global stk.pushOnSubmit=true must not
    /// change test behavior).
    pub fn isolate_git_config(command: &mut Command) {
        command.env("GIT_CONFIG_GLOBAL", nul_device());
        command.env("GIT_CONFIG_NOSYSTEM", "1");
    }

    pub fn git<const N: usize>(&self, args: [&str; N]) -> String {
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

    pub fn git_status<const N: usize>(&self, args: [&str; N]) -> std::process::Output {
        let mut command = Command::new("git");
        Self::isolate_git_config(&mut command);
        command
            .args(args)
            .current_dir(self.path())
            .output()
            .expect("run git command")
    }

    pub fn stack_output<const N: usize>(&self, args: [&str; N]) -> std::process::Output {
        let mut command = self.stack();
        command.args(args).output().expect("run git-stk command")
    }

    pub fn supports_update_refs(&self) -> bool {
        let output = self.git_status(["rebase", "-h"]);
        let help = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        // Match the name: git may render it as --[no-]update-refs.
        help.contains("update-refs")
    }

    pub fn commit_file(&self, path: &str, contents: &str, message: &str) {
        self.write(path, contents);
        self.git(["add", path]);
        self.git(["commit", "-m", message]);
    }

    pub fn stack(&self) -> assert_cmd::Command {
        let mut command = assert_cmd::Command::cargo_bin("git-stk").expect("git-stk binary");
        command.current_dir(self.path());
        command.env("GIT_EDITOR", "true");
        command.env("GIT_CONFIG_GLOBAL", nul_device());
        command.env("GIT_CONFIG_NOSYSTEM", "1");
        // Hermetic color: ambient terminal settings must not restyle output.
        command.env_remove("CLICOLOR");
        command.env_remove("CLICOLOR_FORCE");
        command.env_remove("NO_COLOR");
        command
    }

    /// Unix-only: the fakes are sh scripts (the portable fake is future
    /// work).
    #[cfg(unix)]
    pub fn fake_cli(&self, name: &str, script: &str) -> String {
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

impl TestRepo {
    /// Create a bare repo, add it as origin, and push the given branches.
    pub fn add_bare_origin(&self, branches: &[&str]) -> TempDir {
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

    pub fn remote_sha(&self, bare: &TempDir, branch: &str) -> String {
        let output = Command::new("git")
            .args(["rev-parse", branch])
            .current_dir(bare.path())
            .output()
            .expect("rev-parse remote branch");
        assert!(output.status.success(), "remote branch {branch} missing");
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }
}

impl TestRepo {
    /// Run the bash completion harness: source the registration script, set
    /// up COMP_WORDS for `git stk <words...><TAB>`, invoke the _git_stk shim,
    /// and return COMPREPLY entries.
    /// Unix-only: completion assertions run through a bash harness.
    #[cfg(unix)]
    pub fn complete_git_stk(&self, words: &[&str]) -> String {
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

/// Git's "no config file" sink, per platform.
pub fn nul_device() -> &'static str {
    if cfg!(windows) { "NUL" } else { "/dev/null" }
}
