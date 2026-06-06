//! Development-only helper that bumps the crate version.
//!
//! This binary is gated behind the `dev-tools` feature so it never ships in
//! release artifacts or the published crate. It updates the version in
//! `Cargo.toml` and refreshes `Cargo.lock`, then prints the suggested commit
//! and tag commands. It intentionally does **not** commit or tag: the release
//! flow expects the user to do that manually.

use std::env;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let part = args.get(1).map(String::as_str).unwrap_or("");
    let dry_run = args.iter().any(|arg| arg == "--dry-run" || arg == "-n");

    if !matches!(part, "major" | "minor" | "patch") {
        eprintln!(
            "Usage: cargo run --features dev-tools --bin git-stk-bump -- <major|minor|patch> [--dry-run|-n]"
        );
        std::process::exit(1);
    }

    let cargo_path = Path::new("Cargo.toml");
    let cargo_text = std::fs::read_to_string(cargo_path).context("failed to read Cargo.toml")?;

    let current = extract_cargo_version(&cargo_text)
        .context("unable to find [package] version in Cargo.toml")?;
    let next = bump_version(&current, part)?;

    if dry_run {
        println!("Dry run: would bump version from {current} to {next}");
        println!("Dry run: would refresh Cargo.lock");
        print_next_steps(&next);
        return Ok(());
    }

    let updated = replace_cargo_version(&cargo_text, &next)
        .context("unable to replace [package] version in Cargo.toml")?;
    std::fs::write(cargo_path, updated).context("failed to write Cargo.toml")?;

    refresh_lockfile().context("failed to refresh Cargo.lock")?;

    println!("Bumped version from {current} to {next}");
    print_next_steps(&next);

    Ok(())
}

/// Update the `git-stk` entry in `Cargo.lock` to match `Cargo.toml`.
fn refresh_lockfile() -> Result<()> {
    let status = Command::new(env!("CARGO"))
        .args(["update", "--package", "git-stk", "--offline"])
        .status()
        .context("failed to run cargo update")?;
    if !status.success() {
        bail!("cargo update exited with status {status}");
    }
    Ok(())
}

fn print_next_steps(version: &str) {
    println!();
    print!("{}", next_steps(version));
}

/// The tag must be annotated (`-a`): `git push --follow-tags` only pushes
/// annotated tags, so a lightweight tag would silently stay local.
fn next_steps(version: &str) -> String {
    format!(
        "Next steps (run manually):\n  \
         git add Cargo.toml Cargo.lock\n  \
         git commit -m \"chore(release): bump version to {version}\"\n  \
         git tag -a v{version} -m \"v{version}\"\n  \
         git push --follow-tags\n"
    )
}

/// Extract the `version` field from the `[package]` table.
fn extract_cargo_version(contents: &str) -> Option<String> {
    let mut in_package = false;
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            continue;
        }
        if in_package && trimmed.starts_with("version = ") {
            let rest = trimmed.trim_start_matches("version = ").trim();
            let rest = rest.strip_prefix('"')?;
            let closing = rest.find('"')?;
            return Some(rest[..closing].to_string());
        }
    }
    None
}

/// Replace the `version` field in the `[package]` table without touching any
/// other table's `version` (e.g. dependency versions).
fn replace_cargo_version(contents: &str, new_version: &str) -> Option<String> {
    let mut replaced = false;
    let mut updated = String::new();
    let mut in_package = false;
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            updated.push_str(line);
            updated.push('\n');
            continue;
        }
        if in_package && !replaced && trimmed.starts_with("version = ") {
            let indent = &line[..line.len() - line.trim_start().len()];
            updated.push_str(indent);
            updated.push_str(&format!("version = \"{new_version}\""));
            updated.push('\n');
            replaced = true;
            continue;
        }
        updated.push_str(line);
        updated.push('\n');
    }
    replaced.then_some(updated)
}

fn bump_version(version: &str, part: &str) -> Result<String> {
    let mut parts = version.split('.');
    let mut next = || -> Result<u64> {
        parts
            .next()
            .context("version is missing a component")?
            .parse::<u64>()
            .context("version component is not a number")
    };
    let major = next()?;
    let minor = next()?;
    let patch = next()?;

    let (major, minor, patch) = match part {
        "major" => (major + 1, 0, 0),
        "minor" => (major, minor + 1, 0),
        _ => (major, minor, patch + 1),
    };

    Ok(format!("{major}.{minor}.{patch}"))
}

#[cfg(test)]
mod tests {
    use super::{bump_version, extract_cargo_version, replace_cargo_version};

    const CARGO: &str = r#"[package]
name = "git-stk"
version = "0.1.1"
edition = "2024"

[dependencies]
anyhow = "1.0.100"
clap = { version = "4.5.53", features = ["derive"] }
"#;

    #[test]
    fn extracts_package_version() {
        assert_eq!(extract_cargo_version(CARGO), Some("0.1.1".to_string()));
    }

    #[test]
    fn replaces_only_package_version() {
        let updated = replace_cargo_version(CARGO, "0.2.0").unwrap();
        assert!(updated.contains("version = \"0.2.0\""));
        // Dependency versions untouched.
        assert!(updated.contains("anyhow = \"1.0.100\""));
        assert!(updated.contains("clap = { version = \"4.5.53\""));
        // Exactly one package version line.
        assert_eq!(updated.matches("version = \"0.2.0\"").count(), 1);
    }

    #[test]
    fn bumps_each_part() {
        assert_eq!(bump_version("1.2.3", "major").unwrap(), "2.0.0");
        assert_eq!(bump_version("1.2.3", "minor").unwrap(), "1.3.0");
        assert_eq!(bump_version("1.2.3", "patch").unwrap(), "1.2.4");
    }

    #[test]
    fn next_steps_use_an_annotated_tag() {
        let steps = super::next_steps("0.3.1");
        // --follow-tags only pushes annotated tags; a plain `git tag` would
        // silently leave the tag local.
        assert!(steps.contains("git tag -a v0.3.1 -m \"v0.3.1\""));
        assert!(steps.contains("git push --follow-tags"));
    }
}
