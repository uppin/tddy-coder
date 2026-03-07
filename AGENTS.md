# AGENTS.md

**Project:** TDD-focused development workflow. Uses plan-tdd-one-shot command for feature development from planning through production readiness.

## Project Structure

| Package | Type | Description |
|---------|------|--------------|
| `packages/tddy-core` | Library | CodingBackend trait, Workflow state machine, output parser, Claude/Mock backends |
| `packages/tddy-coder` | Binary | CLI: `--goal plan`, reads stdin, produces PRD.md + TODO.md |

## Toolchain

**Rust workspace**: Root `Cargo.toml` defines workspace members. Build/test from repo root.

**Nix** provides the development environment (rustc, cargo, rustfmt, clippy, rust-analyzer).

### Setup (one-time)

```bash
nix flake lock   # Generate flake.lock
nix develop      # Enter dev shell
```

With **direnv**: `direnv allow` once; the shell loads automatically when you `cd` into the project.

### Commands

| Action | Command |
|--------|---------|
| Build | `cargo build` or `cargo build -p tddy-core` / `-p tddy-coder` |
| Test | `cargo test` or `cargo test -p tddy-core` |
| Lint | `cargo clippy -- -D warnings` |
| Format | `cargo fmt` |
| Run CLI | `cargo run -p tddy-coder -- --goal plan` (reads feature from stdin) |

## Judgment Boundaries

**NEVER**
- Add fallbacks without explicit developer consent — fallbacks make the system unsafe
- Create code branches in production code that only work in test environment
- Use `--no-verify` flag when committing or pushing
- Commit secrets, tokens, or `.env` files
- Modify `packages/*/docs/` directly — use changeset workflow via `docs/dev/1-WIP/`

**ASK**
- Before adding external dependencies
- Before deleting files

**ALWAYS**
- Challenge the developer's decisions — present alternatives and reasoning
- Developer is in charge of the code — do not replace parts of the system unless consented or requested
- Mark temporary or non-production code with FIXME or TODO annotations
- Clearly mark failing tests or unfinished parts in summaries with visual indicators

## Agent Verification (Terminal Output)

**Known issue:** Cursor's agent may not capture terminal command output (see [forum](https://forum.cursor.com/t/agent-doesnt-capture-terminal-output/143161)).

**Workarounds:**
1. **Legacy Terminal:** Cursor Settings → search "Legacy Terminal" → enable, then test in a new chat.
2. **Verify script:** Run `./verify` — writes `cargo test` output to `.verify-result.txt`. Agent can read that file for verification evidence.

**When claiming tests pass:** Run `./verify` (or have the user run it), then read `.verify-result.txt` to confirm. Do not claim success based on exit code alone when output is not visible.

## Cross-Cutting Guides

- [Testing practices](docs/dev/guides/testing.md) — anti-patterns, unit/integration/production test guidelines
- [Technology stack](docs/dev/guides/tech-stack.md) — core technologies, integration patterns

## Documentation Hierarchy

- `packages/*/docs/` — Technical implementation (HOW) per package
- `docs/ft/` — Product requirements (WHAT) by product area
- `docs/dev/1-WIP/` — Active changesets (cross-package deltas)
- `docs/dev/guides/` — Cross-cutting technical guides
