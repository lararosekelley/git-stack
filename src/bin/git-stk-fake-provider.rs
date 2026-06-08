//! A cross-platform stand-in for `gh`/`glab` in the test suite, replacing
//! the Unix-only `sh` fakes so the provider suites can run on Windows too.
//!
//! Behavior is driven by a JSON spec (path in `STK_FAKE_SPEC`): an ordered
//! list of rules matched against the space-joined arguments, first match
//! wins - mirroring an `sh` `case "$*"`. A rule prints stdout/stderr, exits
//! with a code, and may append the arguments to a file (for invocation
//! assertions). Built only under the `test-fakes` feature, so it never
//! ships.

use std::io::Write;

use serde_json::Value;

fn main() -> std::process::ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>().join(" ");

    let spec_path = match std::env::var("STK_FAKE_SPEC") {
        Ok(path) => path,
        Err(_) => {
            eprintln!("fake-provider: STK_FAKE_SPEC is not set");
            return std::process::ExitCode::FAILURE;
        }
    };
    let spec = std::fs::read_to_string(&spec_path).expect("read fake spec");
    let spec: Value = serde_json::from_str(&spec).expect("parse fake spec");
    let rules = spec["rules"].as_array().expect("spec rules array");

    for rule in rules {
        let needle = rule["contains"].as_str().unwrap_or("");
        if !args.contains(needle) {
            continue;
        }

        if let Some(file) = rule["record"].as_str() {
            // Append, like the shell fakes' `> file` (one invocation each).
            let mut handle = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(file)
                .expect("open record file");
            writeln!(handle, "{args}").expect("record args");
        }
        if let Some(out) = rule["stdout"].as_str()
            && !out.is_empty()
        {
            print!("{out}");
            if !out.ends_with('\n') {
                println!();
            }
        }
        if let Some(err) = rule["stderr"].as_str() {
            eprintln!("{err}");
        }
        let code = rule["exit"].as_i64().unwrap_or(0) as u8;
        return std::process::ExitCode::from(code);
    }

    // No rule matched: surface it loudly so a missing case is obvious rather
    // than silently returning nothing.
    eprintln!("fake-provider: no rule matched args: {args}");
    std::process::ExitCode::FAILURE
}
