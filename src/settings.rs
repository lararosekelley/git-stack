//! Every stk-owned git config key and its resolution logic, in one place.

use anyhow::Result;

use crate::cli::PushMode;
use crate::git;

pub const PROVIDER_KEY: &str = "stk.provider";
pub const REMOTE_KEY: &str = "stk.remote";
pub const UPDATE_REFS_KEY: &str = "stk.updateRefs";
pub const PUSH_ON_RESTACK_KEY: &str = "stk.pushOnRestack";
pub const PUSH_ON_SUBMIT_KEY: &str = "stk.pushOnSubmit";
pub const SUBMIT_STACK_KEY: &str = "stk.submitStack";
pub const DEFAULT_REMOTE: &str = "origin";

/// Every `[stk]` setting the tool reads, with its default behavior. Shown by
/// `git stk config`.
pub const SETTINGS: &[(&str, &str)] = &[
    (PROVIDER_KEY, "auto-detect from the remote URL"),
    (REMOTE_KEY, DEFAULT_REMOTE),
    (UPDATE_REFS_KEY, "false"),
    (PUSH_ON_RESTACK_KEY, "false"),
    (PUSH_ON_SUBMIT_KEY, "false"),
    (SUBMIT_STACK_KEY, "false"),
];

/// The remote used for provider detection, trunk discovery, and pushes.
pub fn remote() -> Result<String> {
    Ok(git::config_get(REMOTE_KEY)?.unwrap_or_else(|| DEFAULT_REMOTE.to_owned()))
}

/// A boolean setting's value, defaulting to false when unset.
pub fn bool_setting(key: &str) -> Result<bool> {
    Ok(git::config_get_bool(key)?.unwrap_or(false))
}

/// Resolve a `--push`/`--no-push` flag pair against its config-key default.
pub fn push_enabled(mode: PushMode, key: &str) -> Result<bool> {
    match mode {
        PushMode::Config => Ok(git::config_get_bool(key)?.unwrap_or(false)),
        PushMode::Enabled => Ok(true),
        PushMode::Disabled => Ok(false),
    }
}
