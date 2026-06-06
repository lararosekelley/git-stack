# TODO

> Thoughts on what to do next

---

## Bugs

- [x] Completions not working after running `source <(git stk completions bash)` in shell - the `_git_stk`
      shim called the clap completer without the positional args `complete -F` normally passes, so its command
      dispatch never matched. Fixed; the shim now passes command/cur/prev
- [x] `git stk -h` works but `git stk --help` shows "No manual entry for git-stk" - git intercepts
      `<cmd> --help` and runs `man git-<cmd>`. Fixed via `git stk setup`, which installs the man page to
      `~/.local/share/man/man1` and wires shell completions; `upgrade` re-renders assets via the new binary
      (`setup --refresh`) after each upgrade
- [ ] Completions don't include flags: `git stk submit --<TAB>` completes nothing. DIAGNOSED - it is a
      clap_complete bug with dashed binary names, not our shim (a bash harness shows the direct
      `git-stk submit --<TAB>` form fails too). The generated script is internally inconsistent: the
      dispatch loop builds `cmd="git__stk__subcmd__submit"` but the option `case` labels are generated as
      `git__subcmd__stk__subcmd__submit` (the `-` in `git-stk` expands differently in the two generators),
      so no subcommand-depth case ever matches and `opts` stays empty. Top-level subcommand completion only
      works because the shallow `git__stk` label is consistent. Fix options, in order: (1) bump
      clap_complete and check changelog for a dashed-name fix, (2) post-process the script in
      `completions::print` (rewrite `git__subcmd__stk` -> `git__stk`) with a harness test asserting
      `submit --<TAB>` yields `--dry-run --stack`, (3) report upstream either way - this affects every
      `git-*` subcommand binary
- [x] `git stk bump` next-step instructions don't work as printed: `git tag vX.Y.Z` creates a LIGHTWEIGHT
      tag, and `git push --follow-tags` only pushes ANNOTATED tags (confirmed: `git cat-file -t v0.3.0` ->
      `commit`, not `tag`). Fixed: printed steps now use `git tag -a vX.Y.Z -m vX.Y.Z`
- [x] `git-stk --version` errors - clap's version flag was never enabled. Fixed with `#[command(version)]`

## Handle more cases / types of merges

- [x] Squash-merge detection - solved by tracking fork points instead of detecting squashes: each branch
      records `branch.<name>.stackBase`, restack rebases with `--onto` so only a branch's own commits replay,
      and cleanup records the child's fork point off the merged parent before retargeting. Also makes amended
      or rebase-merged parents restack cleanly
- [ ] Parent PR closed without merging: decide what `cleanup`/`status` should say (currently the merged-only
      fallback lookup means closed-unmerged reviews report as "no review found")
- [ ] Parent branch deleted remotely after merge: use PR metadata to discover the old base when retargeting

## More helpful PR management tools

- [ ] Graphite is really nice in how it comments on PRs directly to show the stack and where this PR sits in it -
      we should do that with either a comment that we can edit when stack is re-submitted/otherwise updated, or
      managing the end of the PR description
- [x] Simpler version first: "Depends on #123" style links in PR bodies on `submit --stack` - maintained in a
      marker-delimited section (`<!-- git-stk:stack -->`) so resubmits update in place; the full Graphite-style
      stack visualization above can reuse those markers later
- [ ] `git stk list --markdown`: print the stack in a copy-paste format for sharing with reviewers in
      Slack/etc. Brief summary at top (e.g. "5 PRs, base main, 3 open / 2 merged"), then an ordered
      bottom-to-top list of PRs/MRs with title, link, and state per entry. Needs provider review lookups,
      so it should degrade gracefully (plain branch names) when no reviews exist or `gh`/`glab` is missing

## Stack ergonomics

- [ ] Tab completion for branch-name arguments (`down <TAB>`, `adopt <TAB>`, `cleanup <TAB>`, ...) - huge DX
      win. clap_complete static scripts can't complete dynamic values; options: clap_complete's dynamic
      completion (`CompleteEnv`, completer queries the binary at completion time - also fixes the static
      script's dashed-name bug for free), or post-process the bash script to call git's `__git_complete_refs`
      for branch positions. Ideally stack-aware: `down` completes only children, `cleanup` only stack branches
- [ ] `status`/`list` should hint at what's next: e.g. "feature/b is 2 commits behind its parent - run
      `git stk restack`", "review #5 base is stale - run `git stk submit`", "parent review merged - run
      `git stk cleanup feature/a`". The data is already fetched; the hints make the tool teach its own loop
- [ ] Silence git noise when it isn't actionable: rebase progress ("Rebasing (1/1)"), `branch -d` upstream
      warnings, etc. currently pass through raw because we use `Stdio::inherit` for status() calls. Capture
      stderr and surface it only on failure (or behind `--verbose`); keep our own one-line summaries as the
      primary output
- [ ] `top` / `bottom` navigation commands
- [ ] Better multi-child UX on `down` (interactive pick instead of erroring)
- [ ] `repair` command: rebuild/verify local metadata from provider state in one shot
- [ ] Ancestry-inference fallback for parent discovery (priority chain from the original design:
      local config -> PR base -> commit ancestry -> ask the user; only the first two exist today)
- [ ] Handle branch renames (metadata under `branch.<old>.stackParent` goes stale)
- [ ] `git stk guide` command to provide an example to try the tool out?

## More git automation

- [ ] Should we handle pushing so it's not a manual step or doesn't have to be? Instead of
      `git push -u <list of all branches in stack>`, perhaps submit could be configured via
      `config.stk.pushOnSubmit` perhaps?
- [x] Same question for `restack`: offer `--push` / config to force-with-lease push rebased branches.
      Done: `restack --push` / `--no-push` with `stack.pushOnRestack` config fallback force-pushes (with
      lease) every rebased branch, including after a conflicted restack finishes via `continue` (the state
      file now carries the full branch list). Without it, restack prints the exact push command - the
      stale-PR-diff trap from the first merge cycle is now impossible to hit silently
- [x] `cleanup --delete-branch` uses `git branch -d`, which can REFUSE after a squash merge (commits are not
      ancestry-merged; it only worked for us because the branch matched its un-pruned upstream). Fixed: uses
      `-D` now, justified by provider-verified merge state - strictly better information than git's ancestry
      heuristic. Regression test does a real squash merge and proves `-d` would refuse
- [ ] Should `--delete-branch` be the default (with `--keep-branch` to opt out)? Cases for keeping: wanting
      to inspect the old commits post-squash, reusing the branch name, or distrust while the tool is young.
      Revisit once `-D` semantics above are in

## Automate completion setup

- [ ] Could installer include optional step (y/n) prompt the user asking if we can add the completion sourcing
      script to their shell?
- [ ] Regardless, we should always print what the user should do after install/upgrade to get shell completions
      for their detected shell
- [ ] Can completion docs and future automations include a guard to make sure `git stk` is valid or completion
      files exist before sourcing completions?

## Providers

- [ ] Self-hosted GitLab support (`stack.gitlab.host` or similar config; detection only matches gitlab.com today)
- [ ] Low-noise "new version available" hint (check at most once/day on a common command, cache the result)

## Clearer docs / what needs to be cared about or is referenced in .gitconfig that the user would manage

- [ ] If we have our own config section, it'd be `[stk]` I figure. NOTE: code currently uses `stack.provider` /
      `stack.remote` and `branch.<name>.stackParent` - renaming to `stk.*` needs a migration/fallback read path
- [ ] Call out any "normal" git config settings we care about (the rebasing --update-refs may be a good example?)
- [ ] Document the install receipt (`~/.config/git-stk/`) and how `upgrade` uses it; `--head` leaves the receipt
      version stale by design

## More release automation

- [ ] Can `cargo publish` happen in our release.yml action automatically at the end? (cargo-dist supports
      `publish-jobs = ["cargo"]` + a `CARGO_REGISTRY_TOKEN` repo secret)
- [ ] Consider musl Linux targets instead of/alongside gnu for portability on older-glibc distros
- [ ] Homebrew tap via cargo-dist once there are non-cargo users; Debian/WinGet/AUR only if usage justifies it
- [ ] Verify `cargo binstall git-stk` finds release artifacts (should work for free with cargo-dist naming)

---

## Done

- [x] Chore - re-organize other docs/temp docs into here (chat exports, old README draft, and RELEASING.md
      deleted; everything actionable from them lives above)
- [x] `git stk upgrade` (+ `--head`, `--force`) via axoupdater; first real upgrade 0.1.1 -> 0.2.0 worked
- [x] `completions` subcommand + bash `_git_stk` shim so `git stk <TAB>` completes
- [x] Installer one-liner at `larakelley.com/sh/git-stk` (stable wrapper served by the website)
- [x] cleanup bug: merged PRs invisible to `gh pr list` default state (fixed with merged-state fallback)
- [x] Linear history rewrite; mirror workflow force-pushes main
- [x] `just bump` version helper (dev-tools feature-gated binary)
