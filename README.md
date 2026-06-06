# git-stk

> Git-native stacked branch workflow helper with GitHub and GitLab review integration.

---

`git-stk` keeps stacks as ordinary Git branches. Stack parent metadata is stored locally in `.gitconfig` as
`branch.<name>.stackParent`, and GitHub PR bases or GitLab MR target branches can be used to reconstruct that metadata.

## Status

This project is experimental. The current implementation focuses on local stacked branch workflows plus
provider-backed review lookup, sync, submit, and cleanup. It does not replace Git's branch model or
attempt automatic conflict resolution.

## Install

```sh
curl https://larakelley.com/sh/git-stk | bash
```

Installers are also attached to [GitHub Releases](https://github.com/lararosekelley/git-stk/releases), or install
from [crates.io](https://crates.io/crates/git-stk) with `cargo install git-stk --locked`.

Then install the man page and wire up shell completions (idempotent; prompts before touching your shell rc):

```sh
git stk setup
```

Upgrade an installer-managed copy with:

```sh
git stk upgrade
```

## Shell Completions

`git stk setup` configures these automatically. The installed binary prints its own completions, so they stay
in sync across upgrades:

```sh
# bash: add to ~/.bashrc
source <(git stk completions bash)

# zsh: write to a directory on your fpath
git stk completions zsh > "${fpath[1]}/_git-stk"
```

Elvish, fish, and PowerShell are also supported. The bash output includes a `_git_stk` wrapper so git's own
completion can complete `git stk <TAB>` in addition to `git-stk <TAB>`.

## Install For Development

```sh
just install
just check
cargo install --path .
```

After installation, Git can use the binary as a sub-command:

```sh
git stk list
```

## Commands

Local stack metadata:

```sh
git stk new <branch>
git stk parent [branch]
git stk children [branch]
git stk list
git stk adopt <branch> --parent <parent>
git stk detach [branch]
```

Navigation and re-stacking:

```sh
git stk up
git stk down [branch]
git stk restack [--update-refs | --no-update-refs] [--push | --no-push]
git stk continue
git stk abort
```

Provider-backed workflows:

```sh
git stk provider
git stk status [branch]
git stk review [branch]
git stk sync [branch] [--dry-run]
git stk submit [branch] [--dry-run]
git stk submit --stack [--dry-run]
git stk cleanup [branch] [--dry-run] [--delete-branch]
```

Upgrading:

```sh
git stk upgrade              # upgrade to the latest release
git stk upgrade --force      # reinstall the latest release even if up to date
git stk upgrade --head [-y]  # build and install the latest unreleased commit
```

`upgrade` uses the install receipt written by the shell installer; copies installed with `cargo install` should
upgrade through cargo instead. `--head` requires a Rust tool-chain, prompts before installing a pre-release build,
and `git stk upgrade --force` returns you to the latest release afterwards.

## Providers

Provider detection uses `stack.provider` first, then `stack.remote`, then `origin`:

```sh
git config stack.provider github  # or gitlab
git config stack.remote origin
```

GitHub support shells out to `gh`. GitLab support shells out to `glab`. Authenticate those CLIs before using provider
commands.

## Re-stacking

`restack` follows Git's `rebase.updateRefs` config by default. Use `--update-refs` or `--no-update-refs` to override that
for one run. If a rebase conflicts, `git-stk` records state in `.git/stack-state`; resolve conflicts and run
`git stk continue`, or run `git stk abort`.

`git-stk` records each branch's fork point in `.gitconfig` as `branch.<name>.stackBase` and rebases with
`--onto`, so only a branch's own commits are replayed. This makes restacking safe after a parent is
squash-merged, rebase-merged, or amended. A missing or stale fork point falls back to a plain rebase.

After a restack, every rebased branch's remote counterpart is stale. Pass `--push` (or set
`git config stack.pushOnRestack true`) to force-push (with lease) all rebased branches automatically,
including after a conflicted restack finishes via `git stk continue`. Without it, `restack` prints the
exact push command instead. `--no-push` overrides the config for one run; `stack.remote` picks the remote
(default `origin`).

## Generated Assets

Shell completions and a `man` page can be generated with:

```sh
just generate-assets
```

Generated files are written under `target/generated`.

## Project Tasks

```sh
just build
just test
just lint
just check
```

## License

Copyright (c) 2026 [Lara Kelley](https://larakelley.com). MIT License. See [LICENSE](./LICENSE).
