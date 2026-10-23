# 2026-10-23 — an opted-in code extension sees tddy's whole process environment as `shell-env`

**Category:** Open design question (trust boundary)
**Found in:** `#reshape` 24/24 (`extension-wasm-command`, PR #683), at green (changeset open item 3)
**Owner:** the developer's decision; then a follow-up PR in `tddy-zed-extensions`

## What happens

`wasm::the_process_worktree` (`packages/tddy-zed-extensions/src/wasm/mod.rs`) hands every opted-in extension a
worktree whose `shell_env` is `std::env::vars()` — every variable of the tddy process, tokens included. The
extension reads it through `worktree.shell-env()` and may return any of it in its `language-server-command`'s
`env`, which is then passed to the server it names. `which` searches that environment's `PATH`.

Related, and recorded in the feature doc rather than here: a re-install keeps the opt-in (`install::commit`), so a
new version's code runs under the consent given to the old one.

## Why it was not decided in #683

Zed itself gives an extension the user's login-shell environment, and the CLI runs from the user's shell, so the
process environment is the nearest equivalent. Narrowing it is a policy call: the developer deferred it ("record
it, do not change it").

## Wanted

A decision — (a) keep the whole environment (Zed's behaviour); (b) an allow-list (`PATH`, `HOME`, `LANG`, `LC_*`,
`TERM`, …) plus `lsp.<id>.binary.env`; (c) `PATH` only — then the rule in `the_process_worktree`, a
`wasm::tests` case pinning it, and the rule written into `docs/ft/coder/zed-extensions.md` § Extensions with code.
