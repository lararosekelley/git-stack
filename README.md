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

Upgrade an installer-managed copy with:

```sh
git stk upgrade
```

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
git stk restack [--update-refs | --no-update-refs]
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
