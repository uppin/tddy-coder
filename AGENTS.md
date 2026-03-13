# AGENTS.md

**Project:** TDD-focused development workflow. Uses plan-tdd-one-shot command for feature development from planning through production readiness.

## Project Structure

| Package | Type | Description |
|---------|------|--------------|
| `packages/tddy-core` | Library | CodingBackend trait, Workflow state machine, output parser, Claude/Mock backends |
| `packages/tddy-coder` | Binary | CLI: `--goal plan`, reads stdin, produces PRD.md + TODO.md |
| `packages/tddy-web` | Web app | React dashboard for dev progress tracking (Storybook, Cypress) |

## Toolchain

**Rust workspace**: Root `Cargo.toml` defines workspace members. Build/test from repo root.

**Bun workspace**: Root `package.json` with `workspaces: ["packages/tddy-web"]`. Run `bun install` from repo root.

**Nix** provides the development environment (rustc, cargo, rustfmt, clippy, rust-analyzer, bun).

### Setup (one-time)

```bash
nix flake lock   # Generate flake.lock
nix develop      # Enter dev shell
```

With **direnv**: `direnv allow` once; the shell loads automatically when you `cd` into the project.

### Commands

All `./` scripts use nix dev shell via `--profile ./.nix-profile` for a consistent toolchain.

| Action | Command |
|--------|---------|
| Dev shell | `./dev` — enter nix dev shell with a GC-rooted profile. With args, runs the command inside the shell (e.g. `./dev cargo clippy`) |
| Build | `cargo build` or `cargo build -p tddy-core` / `-p tddy-coder` |
| Release | `./release` — optimized production build (output: `target/release/tddy-coder`, `target/release/tddy-tools`) |
| Test | `./test` — builds tddy-coder + tddy-tools, then runs all tests (output also written to `.verify-result.txt`). Supports args: `./test -p tddy-core` or `./test -- test_name` |
| Clean | `./clean` — removes stale Cargo build fingerprints from `target/debug/build` and `target/release/build`, keeping only the newest per crate |
| Lint | `cargo clippy -- -D warnings` |
| Format | `cargo fmt` |
| Run CLI | `cargo run -p tddy-coder -- --goal plan` (reads feature from stdin) |
| Web install | `bun install` — install web workspace dependencies |
| Web build | `bun run build` (from root or `packages/tddy-web`) |
| Storybook | `bun run storybook` — dev server at http://localhost:6006 |
| Cypress component | `bun run cypress:component` (from `packages/tddy-web`) |
| Cypress e2e | `bun run cypress:e2e` (from `packages/tddy-web`; requires Storybook running) |

## Judgment Boundaries

**NEVER**
- Add fallbacks without explicit developer consent — fallbacks make the system unsafe
- Use direct stdout/stderr (e.g. `println!`, `eprintln!`) in code paths that run under the TUI — it corrupts the ratatui display
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

## Demo Plans (tddy-coder)

When a feature includes a demo (e.g. `demo-plan.md`), the demo must run **via a pre-made shell script** that launches the app in its own terminal window.

- **Do** create a `demo.sh` script in the plan directory that runs the app in a separate terminal (e.g. `open -a Terminal` on macOS, `gnome-terminal` on Linux).
- **Do not** run interactive commands directly (e.g. `cargo run`) — that would share stdin/terminal with the parent and cause freezes.
- When the user chooses Run, the agent executes the demo script using tools (Bash). The script handles launching the app in its own window.

## Cross-Cutting Guides

- [Testing practices](docs/dev/guides/testing.md) — anti-patterns, unit/integration/production test guidelines
- [Technology stack](docs/dev/guides/tech-stack.md) — core technologies, integration patterns

## Documentation Hierarchy

- `packages/*/docs/` — Technical implementation (HOW) per package
- `docs/ft/` — Product requirements (WHAT) by product area
- `docs/dev/1-WIP/` — Active changesets (cross-package deltas)
- `docs/dev/guides/` — Cross-cutting technical guides
