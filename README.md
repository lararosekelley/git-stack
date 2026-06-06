# git-stack

> Git-native stacked branch workflow helper with GitHub and GitLab review integration.

---

`git-stack` keeps stacks as ordinary Git branches. Stack parent metadata is stored locally in `.gitconfig` as
`branch.<name>.stackParent`, and GitHub PR bases or GitLab MR target branches can be used to reconstruct that metadata.

## Status

This project is experimental. The current implementation focuses on local stacked branch workflows plus
provider-backed review lookup, sync, submit, and cleanup. It does not replace Git's branch model or
attempt automatic conflict resolution.

## Install For Development

```sh
just install
just check
cargo install --path .
```

After installation, Git can use the binary as a sub-command:

```sh
git stack list
```

## Commands

Local stack metadata:

```sh
git stack new <branch>
git stack parent [branch]
git stack children [branch]
git stack list
git stack adopt <branch> --parent <parent>
git stack detach [branch]
```

Navigation and re-stacking:

```sh
git stack up
git stack down [branch]
git stack restack [--update-refs | --no-update-refs]
git stack continue
git stack abort
```

Provider-backed workflows:

```sh
git stack provider
git stack status [branch]
git stack review [branch]
git stack sync [branch] [--dry-run]
git stack submit [branch] [--dry-run]
git stack submit --stack [--dry-run]
git stack cleanup [branch] [--dry-run] [--delete-branch]
```

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
for one run. If a rebase conflicts, `git-stack` records state in `.git/stack-state`; resolve conflicts and run
`git stack continue`, or run `git stack abort`.

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
