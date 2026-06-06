# TODO

> Thoughts on what to do next

---

## Bugs

- [ ] Completions not working after running `source <(git stk completions bash)` in shell - needs diagnosis
      (shim function vs git's completion loading order? does `git-stk <TAB>` work while `git stk <TAB>` doesn't?
      is bash-completion / git's completion script loaded before ours?)
- [ ] `git stk -h` works but `git stk --help` shows "No manual entry for git-stk" - git intercepts
      `<cmd> --help` and runs `man git-<cmd>`, and our man page is generated but never installed anywhere.
      Fix ideas: ship `git-stk.1` in release archives + install/print-path step, and/or a
      `git stk manpage` command mirroring the completions approach

## Handle more cases / types of merges

- [ ] Squash-merge detection is important for repos without merge commits - crucial for solid `cleanup` command.
      Children carry commits that are upstream by patch but not by SHA; needs patch-id/`git cherry` detection
      when retargeting/rebasing after a parent lands. (GitHub is now squash/rebase-only for this repo, so we
      will hit this ourselves.)
- [ ] Parent PR closed without merging: decide what `cleanup`/`status` should say (currently the merged-only
      fallback lookup means closed-unmerged reviews report as "no review found")
- [ ] Parent branch deleted remotely after merge: use PR metadata to discover the old base when retargeting

## More helpful PR management tools

- [ ] Graphite is really nice in how it comments on PRs directly to show the stack and where this PR sits in it -
      we should do that with either a comment that we can edit when stack is re-submitted/otherwise updated, or
      managing the end of the PR description
- [ ] Simpler version first: "Depends on #123" style links in PR bodies on `submit --stack`

## Stack ergonomics

- [ ] `top` / `bottom` navigation commands
- [ ] Better multi-child UX on `down` (interactive pick instead of erroring)
- [ ] `repair` command: rebuild/verify local metadata from provider state in one shot
- [ ] Ancestry-inference fallback for parent discovery (priority chain from the original design:
      local config -> PR base -> commit ancestry -> ask the user; only the first two exist today)
- [ ] Handle branch renames (metadata under `branch.<old>.stackParent` goes stale)

## More git automation

- [ ] Should we handle pushing so it's not a manual step or doesn't have to be? Instead of
      `git push -u <list of all branches in stack>`, perhaps submit could be configured via
      `config.stk.pushOnSubmit` perhaps?
- [ ] Same question for `restack`: offer `--push` / config to force-with-lease push rebased branches

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
