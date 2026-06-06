//! One module per CLI command. Every command is a clap `Args` struct that
//! implements [`Run`], so each has the same shape: parse, then `run()`.

use anyhow::Result;

/// The interface every command implements.
pub trait Run {
    fn run(self) -> Result<()>;
}

pub mod adopt;
pub mod children;
pub mod cleanup;
pub mod completions;
pub mod config;
pub mod detach;
pub mod down;
pub mod list;
pub mod merge;
pub mod new;
pub mod parent;
pub mod provider;
pub mod repair;
pub mod restack;
pub mod review;
pub mod setup;
pub mod status;
pub mod submit;
pub mod sync;
pub mod up;
pub mod upgrade;
