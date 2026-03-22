# Create pull request

Full workflow (branch rules, artifacts to exclude, `./dev`, `gh pr`): **`.cursor/commands/pr.md`**.

## Short process

1. **Branch**: On `master`/`main`, create `git checkout -b <name>` from updated `origin/master`; never push `master` to a random remote branch name.
2. **Review**: `git status`, `git diff`; exclude agent scratch files (`.tddy-*-submit.json`, etc.) unless the user wants them.
3. **Ask** if unrelated changes are mixed.
4. **Verify**: `./dev cargo fmt --all`, `./dev cargo clippy -- -D warnings`, `./test` (see [AGENTS.md](../../AGENTS.md)). Fix failures; **never** `--no-verify`.
5. **Commit** with Markdown body; **stage only PR-relevant files**.
6. **Push**: `git push -u origin <branch>`.
7. **PR**: `gh pr create` or use the compare URL Git prints; optional `xdg-open` / `open` for browser.
