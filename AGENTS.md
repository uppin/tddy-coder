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

**Bun workspace**: Root `package.json` with `workspaces: ["packages/tddy-web"]`. Run `bun install` from repo root. See [Bun Workspace](#bun-workspace) for build and test commands.

**Nix** provides the development environment (rustc, cargo, rustfmt, clippy, rust-analyzer, bun, node).

### Setup (one-time)

```bash
nix flake lock   # Generate flake.lock
nix develop      # Enter dev shell
```

With **direnv**: `direnv allow` once; the shell loads automatically when you `cd` into the project.

### Root scripts

| Script | Purpose |
|--------|---------|
| `./dev` | Enter nix dev shell with profile (persists across `nix gc`). With args: run command inside shell, e.g. `./dev cargo test` or `./dev echo "Hello"`. |
| `./release` | Build optimized production binaries (`tddy-coder`, `tddy-tools`, `tddy-daemon`). Output: `target/release/...`. |
| `./install` | Install **`tddy-daemon`**, **`tddy-coder`**, **`tddy-tools`** as a systemd service: `sudo ./install --systemd` (optional `--build` runs `./release`). Copies **`packages/tddy-web/dist`** to the web bundle path (requires `bun run build` first). Path overrides: `INSTALL_PREFIX`, `INSTALL_BIN_DIR`, `INSTALL_CONFIG_DIR`, `INSTALL_SYSTEMD_DIR`, `INSTALL_WEB_BUNDLE_DIR`; test harness: `INSTALL_NO_SYSTEMCTL=1` skips root check and `systemctl`. |
| `./test` | Build tddy-coder + tddy-tools, run all tests. Writes output to `.verify-result.txt` (agent workaround for Cursor terminal capture). Usage: `./test` — all tests; `./test -p tddy-core` — one package; `./test -- test_name` — specific test. |
| `./clean` | Remove stale Cargo build fingerprints, deps, incremental. Keeps newest per crate in `target/debug` and `target/release`. Frees disk space without full `cargo clean`. |
| `./verify` | Run `cargo test` and write output to `.verify-result.txt`. Use when agent terminal capture fails; read that file for verification evidence. |
| `./web-dev` | Start **`tddy-daemon`** (see **`DAEMON_CONFIG`** / **`dev.daemon.yaml`**) and the **`tddy-web`** Vite dev server with **`/rpc`** proxy. See [docs/ft/web/local-web-dev.md](docs/ft/web/local-web-dev.md). |

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
| Web install | `./dev bun install` — install web workspace dependencies |
| Web build | `./dev bun run build` (from root or `packages/tddy-web`) |
| Storybook | `./dev bun run storybook` — dev server at http://localhost:6006 |
| Cypress component | `./dev bun run cypress:component` (from root or `packages/tddy-web`) |
| Cypress e2e | `./dev bun run cypress:e2e` (from root or `packages/tddy-web`; builds Storybook, serves on ephemeral port, runs tests) |

### Bun Workspace

The web packages live in `packages/tddy-web`. Bun and node come from the nix dev shell.

**Running bun/node with nix**

Either enter the shell first, or run commands via `./dev`:

```bash
./dev                    # Enter shell, then: bun install, bun run storybook, etc.
./dev bun install        # One-off: install deps
./dev bun run storybook  # One-off: start Storybook
```

**Setup**
```bash
bun install   # From repo root; installs all workspace deps
```

**Build**
```bash
bun run build                    # tddy-web app → dist/
bun run build-storybook          # Static Storybook → storybook-static/
```

**Tests**
```bash
bun run cypress:component        # Cypress component tests (Button, etc.)
bun run cypress:component:debug  # Same, with DEBUG=cypress:*
bun run cypress:e2e              # Builds Storybook, serves on ephemeral port, runs e2e tests
```

**Storybook**
```bash
bun run storybook                # Dev server at http://localhost:6006
```

All commands can be run from repo root (they use `--filter tddy-web`) or from `packages/tddy-web`.

### LiveKit Testkit (tddy-livekit, tddy-livekit-testkit)

Tests can reuse a running LiveKit container instead of starting one per run. Speeds up repeated test execution.

**Start or reuse a server:**
```bash
./run-livekit-testkit-server   # Reuses container "tddy-livekit-testkit" if running; prints LIVEKIT_TESTKIT_WS_URL=ws://127.0.0.1:PORT
```

The script reuses the same container (`tddy-livekit-testkit`) across runs. No new container is created on each invocation.

**Run tests against it:**
```bash
export LIVEKIT_TESTKIT_WS_URL=ws://127.0.0.1:PORT   # Use port from script output
cargo test -p tddy-livekit -p tddy-livekit-testkit
```

Or: `eval $(./run-livekit-testkit-server | grep '^export ')` then run tests.

Without the env var, tests start a fresh container via testcontainers (default).

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
- [Changelog merge hygiene](docs/dev/guides/changelog-merge-hygiene.md) — format for `changelog.md`, `changesets.md`, and optional `docs/dev/changesets.d/` shards

## Documentation Hierarchy

- `packages/*/docs/` — Technical implementation (HOW) per package
- `docs/ft/` — Product requirements (WHAT) by product area
- `docs/dev/1-WIP/` — Active changesets (cross-package deltas)
- `docs/dev/changesets.d/` — Optional long-form cross-package changeset shards ([README](docs/dev/changesets.d/README.md)); index remains `docs/dev/changesets.md`
- `docs/dev/guides/` — Cross-cutting technical guides
- **`plans/`** (repo root, optional) — Persisted **grill-me** **Create plan** output (the brief: problem, Q&A, analysis, preliminary plan) for version control in the working copy. Use a descriptive basename, e.g. **`plans/<feature-slug>-grill-me-brief.md`**. If a feature PRD or guide in **`docs/ft/`** specifies a different path under the repo, use that instead. If nothing is specified, default to **`plans/<SOME-PLAN-NAME>.md`** (replace `<SOME-PLAN-NAME>` with a stable, human-readable label for the effort). Session-scoped **`artifacts/grill-me-brief.md`** remains the runtime path during the session; **`plans/`** is the documented convention for copying or checking in the same content for the team repo.
