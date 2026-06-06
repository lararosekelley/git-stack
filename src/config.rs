use anyhow::Result;

use crate::git;

/// Every `[stk]` setting the tool reads, with its default behavior.
const SETTINGS: &[(&str, &str)] = &[
    ("stk.provider", "auto-detect from the remote URL"),
    ("stk.remote", "origin"),
    ("stk.updateRefs", "false"),
    ("stk.pushOnRestack", "false"),
    ("stk.pushOnSubmit", "false"),
];

/// Print every stk-owned git config value: the `[stk]` settings (with
/// defaults for unset keys) and the per-branch stack metadata.
pub fn print_config() -> Result<()> {
    for (key, default) in SETTINGS {
        match git::config_get(key)? {
            Some(value) => println!("{key} = {value}"),
            None => println!("{key} (default: {default})"),
        }
    }

    let metadata = git::config_get_regexp(r"^branch\..*\.stk(parent|base)$")?;
    if metadata.is_empty() {
        println!();
        println!("no branch metadata (no stacked branches)");
        return Ok(());
    }

    println!();
    println!("branch metadata:");
    for (key, value) in metadata {
        println!("  {key} = {value}");
    }
    Ok(())
}
