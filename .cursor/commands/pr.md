---
description: Open a pull request from the current work—review diffs, commit only PR-relevant files, push branch, create PR link
---

## PR — Open a pull request from current focus

Prepare a clean commit (or commits), push to `origin`, and give the user a URL to open the PR. **Include only files that belong in the PR.**

## Before you change git state

1. **`git status`** and **`git diff`** — understand every modified and untracked path.
2. **Split by relevance** — production code, tests, lockfiles, and intentional doc/command updates belong together; **agent scratch artifacts** (e.g. `.tddy-*-submit.json`, `.tddy-red-phase-output.txt`, local `plan/` drafts) usually **must not** be committed unless the user explicitly wants them. **Never** commit secrets, tokens, or `.env`.
3. If unrelated edits are mixed in the working tree, **ask the user** which paths to include before staging.
4. **Do not use `git commit --no-verify`** — forbidden in this repo ([AGENTS.md](../../AGENTS.md)). If hooks fail, fix fmt/clippy/tests or the hook itself.

## Branch selection

1. **On `main` or `master`** (this repo tracks **`master`** upstream):
   - **Create a new branch** before committing: `git fetch origin` then `git checkout -b <meaningful-branch-name>` from the latest `origin/master` (or merge/rebase first if required).
   - Do **not** push local `master` to a differently named remote branch as a substitute for branching.

2. **Already on a feature branch** that matches the work:
   - Use it as-is (prefer **rebase/merge** from `origin/master` if the branch is behind and the user wants it current).

3. **On a branch unrelated to this change**:
   - Ask whether to use it, create a new branch from `origin/master`, or move changes with `git stash` / cherry-pick.

## Staging and commit

1. **`git add`** only agreed paths — use `git add -p` when helpful for partial hunks.
2. **Message**: use **Markdown** in the body (bullets, subsections). Subject line: imperative, ~72 chars. Describe *what* and *why*.
3. Run checks before or after staging as appropriate:
   - Rust: `./dev cargo fmt --all`, `./dev cargo clippy -- -D warnings`, `./test` (or `./verify` and read `.verify-result.txt`).
   - Web (`packages/tddy-web`): `./dev bun run build --filter tddy-web`, `bun test`, Cypress if the change touches UI/tests.
4. Resolve **lint errors** that block commit; do not “skip” with `--no-verify`.

## Push

```bash
git push -u origin <branch-name>
```

If the branch is new on the remote, upstream `-u` avoids ambiguity.

## PR creation URL

After push, Git often prints a **compare URL** (`https://github.com/org/repo/compare/...`). If not:

- **GitHub CLI** (if installed and authenticated): `gh pr create --fill` or `gh pr create --title "..." --body "..."`.
- Otherwise give the user: `https://github.com/<org>/<repo>/compare/master...<branch>` (adjust default branch if not `master`).

**Browser** (optional, ask in sandboxed environments):

- macOS: `open <url>`
- Linux: `xdg-open <url>` (or `gio open`)
- Do not assume a display is available in CI/agent-only environments.

## Checklist

- [ ] Only PR-relevant paths staged; user consulted if mixed.
- [ ] No secrets; no accidental agent-only artifacts.
- [ ] Branch strategy matches rules above.
- [ ] fmt / clippy / tests green (and web checks if applicable).
- [ ] Commit message is clear Markdown.
- [ ] Pushed with tracking; user has compare or `gh pr` URL.

## Related

- [pr-wrap.md](./pr-wrap.md) — validation workflow before merge
- [AGENTS.md](../../AGENTS.md) — `./test`, `./verify`, Judgment Boundaries
