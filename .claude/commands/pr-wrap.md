# PR preparation (wrap for merge)

Full step-by-step template: **`.cursor/commands/pr-wrap.md`**.

## Checklist

- [ ] `/validate-changes` → fix issues → update changeset Validation Results
- [ ] `/validate-tests` → fix → update changeset
- [ ] `/validate-prod-ready` → fix → update changeset
- [ ] `/analyze-clean-code` → fix → update changeset
- [ ] `/validate-changes` again (final)
- [ ] Toolchain: `./dev cargo fmt --all`, `./dev cargo clippy -- -D warnings`, `./test` (or `./verify` + read `.verify-result.txt`)
- [ ] If `packages/tddy-web` changed: `./dev bun run build --filter tddy-web`, `bun test`, Cypress as needed
- [ ] `/wrap-context-docs` when changeset is complete (see wrap-context-docs command)
- [ ] Summary for user; then optional `/pr`

## Rules

- **Never** `git commit --no-verify` ([AGENTS.md](../../AGENTS.md)).
- Use **`./dev`** for Rust/Bun so the nix toolchain is consistent.
- **`./test`** / **`./verify`** build `tddy-acp-stub` before tests that need it.

## Task subagent

Use **Task** with subagent type `refactor` only when you want a focused refactor pass; otherwise fix in the main session.
