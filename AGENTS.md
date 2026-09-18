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

### Verification: scope it locally, run it in full on CI

**Never run a full-repo build or test locally.** A workspace-wide `cargo build`, a bare `./test`, a
workspace-wide `cargo clippy` or a full Cypress run costs tens of minutes to hours on a dev machine,
competes with other worktrees for `target/`, and reports **pre-existing failures from packages you
did not touch** — noise that is routinely misread as damage from the current change.

**Locally: scope every gate to the packages you actually touched, and say that you scoped it.**

| Gate | Local (scoped) | Never locally |
|------|----------------|---------------|
| Test | `./test -p <pkg>` (repeat `-p` per package), `./test -- <test_name>` | bare `./test` |
| Build | `cargo build -p <pkg>` | `cargo build` / `--workspace` |
| Lint | `cargo clippy -p <pkg> -- -D warnings` | `cargo clippy -- -D warnings` |
| Web | the single spec or story under change | full `cypress:e2e` (~50 min, 207 specs) |

**For full-repo verification, push and read the PR checks — do not reproduce CI locally.** CI is the
authority on whole-workspace health, and it runs in parallel on clean machines:

```bash
git push origin "$(git branch --show-current)"
scripts/ci-status.sh --watch        # block until the run finishes
scripts/ci-status.sh --failures     # failing test names, files, assertion messages, log tails
```

`scripts/ci-status.sh` reports per-check state plus **pass/fail test counts** for the current
branch's PR; a bare number targets that PR (`scripts/ci-status.sh <PR>`). See
[docs/dev/guides/ci.md](docs/dev/guides/ci.md) for what each check covers and what the gate
deliberately skips.

**When claiming a change is green**: quote the scoped local run for the packages you touched, and
the CI result for everything else. Never claim whole-workspace green from a local run, and never
report a full-repo run you did not actually complete.

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
| `./release` | Build optimized production binaries (`tddy-coder`, `tddy-tools`, `tddy-daemon`, `tddy-supervisor`, `tddy-remote-git-repo`, `tddy-session-sync`, `tddy-sandbox-runner`, `tddy-index-daemon`). Output: `target/release/...`. **`./release --desktop`** additionally builds what `./install --desktop` ships, in the only order that works: the CLI binaries, then the `tddy-web` bundle, then `tauri build` — the application *embeds* the bundle, so building it first bakes in a stale dashboard with no error. A `tauri build` that fails after the `.app` exists (on macOS the `dmg` target drives Finder over AppleScript and needs Automation permission) says so and names the bundle, so `./install --desktop` can install it. An unrecognised argument is an error, not a no-op. |
| `./install` | Install **`tddy-supervisor`**, **`tddy-daemon`**, **`tddy-coder`**, **`tddy-tools`**, **`tddy-remote-git-repo`** (git's `GIT_SSH_COMMAND` shim), **`tddy-session-sync`** (mirrors a session's worktree) **`tddy-sandbox-runner`** (runs *inside* every jail the daemon spawns, so it must land beside `tddy-daemon` — the daemon resolves it as a sibling of its own executable) and **`tddy-index-daemon`** (the warm rust-analyzer index, spawned by the daemon and resolved the same sibling way) — the first two are clients, shipped so they are on `PATH`: `sudo ./install --systemd` (optional `--build` runs `./release`). **System mode** installs one unit, **`tddy-supervisor.service`** (root), which starts `tddy-daemon` as an unprivileged child declared in `supervisor.yaml` — so no `tddy-daemon.service` is written, and an inherited one is masked. **`--user`** installs a per-user `tddy-daemon.service` via `systemctl --user` and **no supervisor**: rootless, it could neither setuid nor delegate cgroups, so it would broker nothing. `--headless` installs without requiring/shipping **`packages/tddy-web/dist`** (daemon serves `/rpc` + `/api/config`, no UI). Path overrides: `INSTALL_PREFIX`, `INSTALL_BIN_DIR`, `INSTALL_CONFIG_DIR`, `INSTALL_SYSTEMD_DIR`, `INSTALL_WEB_BUNDLE_DIR`, `INSTALL_DAEMON_LOG_DIR`, `INSTALL_AUTH_STORAGE_DIR`, `INSTALL_SUPERVISOR_SOCKET_PATH`, `INSTALL_DAEMON_SOCKET_PATH`; test harness: `INSTALL_NO_SYSTEMCTL=1` skips the root check and `systemctl` (it does not redirect paths — override the four dirs too). |
| `./install --desktop` | Install **Tddy Desktop** locally — a *different deployment* from `--systemd`, and the two are an error in one run. The application's own process **is** a `tddy-daemon`, so this installs **no unit, no service user, no web bundle directory, no served port** and neither `tddy-daemon` nor `tddy-supervisor`. (`desktop.yaml` still declares `listen.web_port` — `runtime::build` requires it, and here it names the loopback port a GitHub sign-in returns on, not a listener.) The rendered config leaves `github:`, `livekit:` and `users:` unset, so the installed app opens on its settings and has **no sessions** until an identity is configured — see `docs/dev/todo/2026-09-18-desktop-install-configures-no-identity.md`. macOS: `Tddy Desktop.app` into `~/Applications`, plus a `tddy-desktop` launcher on `PATH` that `exec`s the binary *inside* the bundle (a symlink would break bundle identity and the daemon's sibling-binary lookup). Linux: the binary into `$BIN_DIR`, a `.desktop` entry and a hicolor icon under `$XDG_DATA_HOME`. Both: `tddy-coder`, `tddy-tools`, `tddy-sandbox-runner`, `tddy-index-daemon`, `tddy-remote-git-repo`, `tddy-session-sync` on `PATH` (on macOS also inside `Contents/MacOS`, which is how the sibling lookup resolves them). Config is rendered from `desktop.yaml.production` to **`~/.tddy/desktop.yaml`** — **the only file a release build reads** (no `TDDY_DAEMON_CONFIG`, no `dev.desktop.yaml`, no upward walk; see `packages/tddy-desktop/src-tauri/src/config_source.rs`) — and an existing one is never overwritten. `--build` runs **`./release --desktop`**, which owns that three-step order because the app *embeds* the bundle; without it the install preflights and names that script. Path overrides: `INSTALL_TDDY_HOME`, `INSTALL_BIN_DIR`, `INSTALL_DESKTOP_APP_DIR`, `INSTALL_XDG_DATA_DIR`, `INSTALL_DAEMON_LOG_DIR`, `INSTALL_AUTH_STORAGE_DIR`. |
| `./publish.sh` | Package the release binaries + web bundle as a `.deb` and upload it to an apt repo: `./publish.sh <repo-path> [--build]` (`<repo-path>` is rsync-style, local or `host:dir`). Installs to `/usr/bin` (`tddy-daemon`, `tddy-coder`, `tddy-tools`, `tddy-remote-git-repo`, `tddy-session-sync`, `tddy-sandbox-runner`, `codex-acp`), `/usr/share/tddy/web`, `/etc/tddy/daemon.yaml` (conffile), `/lib/systemd/system`. Overrides: `PUBLISH_PKG_NAME`, `PUBLISH_VERSION`, `PUBLISH_ARCH`, `PUBLISH_MAINTAINER`, `PUBLISH_DEPENDS`, `PUBLISH_INCOMING`, `PUBLISH_GPG_KEY_ID`. Refresh repo metadata afterwards (e.g. `reprepro processincoming default`). **Does not yet ship `tddy-supervisor` or its unit** — see `docs/dev/todo/`. |
| `./test` | Build tddy-coder + tddy-tools, then run tests. Writes output to `.verify-result.txt` (agent workaround for Cursor terminal capture). Usage: `./test -p tddy-core` — one package; `./test -- test_name` — specific test; `./test` — **everything, which is a CI job, not a local one** (see [Verification](#verification-scope-it-locally-run-it-in-full-on-ci)). |
| `./clean` | Remove stale Cargo build fingerprints, deps, incremental. Keeps newest per crate in `target/debug` and `target/release`. Frees disk space without full `cargo clean`. |
| `./verify` | Run `cargo test` and write output to `.verify-result.txt`. Use when agent terminal capture fails; read that file for verification evidence. Unscoped, so it runs the whole workspace — prefer `./test -p <pkg>`, which writes the same file, and leave the full run to CI. |
| `scripts/ci-status.sh` | Report GitHub Actions status for the current branch's PR: per-check state plus **pass/fail test counts**. `--failures` adds failing test names, files, assertion messages and failing-step log tails; `--watch` blocks until the run finishes; a bare number targets that PR. See [docs/dev/guides/ci.md](docs/dev/guides/ci.md). |
| `./web-dev` | Start **`tddy-daemon`** (see **`DAEMON_CONFIG`** / **`dev.daemon.yaml`**) and the **`tddy-web`** Vite dev server with **`/rpc`** proxy. See [docs/ft/web/local-web-dev.md](docs/ft/web/local-web-dev.md). |
| `./vm-tests` | Run the **VM-backed production tests** — the ones that boot a real QEMU guest. Deliberately **not** part of `./test`: every one is `#[ignore]`d, so a default run reports them as ignored and boots nothing. `./vm-tests` — all suites; `./vm-tests <substring>` — matching tests only; `./vm-tests --list` — show the suites. Requires **`TDDY_CLOUDINIT_BASE_IMAGE`** (exported or in `.env`); nothing is downloaded. Warm the cache with `./run-vm-testkit` first so the bakes are a one-time cost. Each suite runs `--test-threads=1` — not optional, since these bind fixed host ports and QEMU derives its monitor socket path from the port alone. |
| `./run-index-daemon` | Start or reuse this checkout's **`tddy-index-daemon`** — the warm rust-analyzer index `tddy-tools restructure` runs against when **`TDDY_INDEX_SOCKET`** is set, instead of paying a cold crate-graph load (six to ten minutes on this workspace) per invocation. Narration on stderr, one line on stdout: `export TDDY_INDEX_SOCKET=<path>`, so `eval $(./run-index-daemon | grep '^export ')` works. `--status` **dials** the socket (`tddy-index-daemon --ping`) rather than trusting a pid, and exits non-zero when nothing answers; it starts and builds nothing. `--stop` stops it. The daemon is given a **session of its own** (`setsid`), so it outlives the shell that started it — a background job started with `nohup` alone dies with its starting shell's process group. One daemon per checkout, keyed by its path; socket, pid file and log live under **`TDDY_INDEX_RUNTIME_DIR`** (default `$TMPDIR`) because an AF_UNIX path cannot hold a worktree path plus a subdirectory. |
| `./run-vm-testkit` | Warm the **`tddy-vm-testkit`** image cache under `tmp/.tddy` so the VM-backed cgroups production tests don't pay for it. `--status` reports what is cached, bakes nothing. Requires **`TDDY_CLOUDINIT_BASE_IMAGE`** (exported or in `.env`) pointing at a cloud image **already on disk** — nothing is ever downloaded. First run bakes three chained images and takes hours; later runs are a boot plus an incremental `./release`. See [docs/ft/vm/tddy-vm.md](docs/ft/vm/tddy-vm.md) § VM testkit. |

### Commands

All `./` scripts use nix dev shell via `--profile ./.nix-profile` for a consistent toolchain.

| Action | Command |
|--------|---------|
| Dev shell | `./dev` — enter nix dev shell with a GC-rooted profile. With args, runs the command inside the shell (e.g. `./dev cargo clippy`) |
| Build | `cargo build -p <pkg>` — **scope it**, see [Verification](#verification-scope-it-locally-run-it-in-full-on-ci). Full-workspace builds belong on CI |
| Release | `./release` — optimized production build (output: `target/release/tddy-coder`, `target/release/tddy-tools`, `target/release/tddy-daemon`, `target/release/tddy-supervisor`, `target/release/tddy-remote-git-repo`, `target/release/tddy-session-sync`, `target/release/tddy-sandbox-runner`). `./release --desktop` adds the `tddy-web` bundle and the Tauri application, which is what `./install --desktop` expects to find |
| Test | `./test -p <pkg>` or `./test -- test_name` — **scope it to the packages you touched**, see [Verification](#verification-scope-it-locally-run-it-in-full-on-ci). A bare `./test` runs everything and is a CI job, not a local one. Output is also written to `.verify-result.txt` |
| Clean | `./clean` — removes stale Cargo build fingerprints from `target/debug/build` and `target/release/build`, keeping only the newest per crate |
| Lint | `cargo clippy -p <pkg> -- -D warnings` — scoped; the workspace-wide run is CI's |
| Format | `cargo fmt` |
| Run CLI | `cargo run -p tddy-coder -- --goal plan` (reads feature from stdin) |
| Web install | `./dev bun install` — workspace JS deps (includes **`@zed-industries/codex-acp`** for **`./install`**) |
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
  (**one exception**: `packages/*/docs/code-issues/` records, which `/analyze-code-issues` and
  `/green` write straight in — they are standing measurements with their own reconciliation
  contract, and routing one through a changeset would delete it at the next wrap)

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

**When claiming tests pass:** Run the **scoped** gate for the packages you touched — `./test -p <pkg>`, which also writes `.verify-result.txt` — then read that file to confirm. Do not claim success based on exit code alone when output is not visible, and do not claim whole-workspace green from a local run: that answer comes from CI via `scripts/ci-status.sh` (see [Verification](#verification-scope-it-locally-run-it-in-full-on-ci)).

## Demo Plans (tddy-coder)

When a feature includes a demo (e.g. `demo-plan.md`), the demo must run **via a pre-made shell script** that launches the app in its own terminal window.

- **Do** create a `demo.sh` script in the plan directory that runs the app in a separate terminal (e.g. `open -a Terminal` on macOS, `gnome-terminal` on Linux).
- **Do not** run interactive commands directly (e.g. `cargo run`) — that would share stdin/terminal with the parent and cause freezes.
- When the user chooses Run, the agent executes the demo script using tools (Bash). The script handles launching the app in its own window.

## Cross-Cutting Guides

- [Testing practices](docs/dev/guides/testing.md) — anti-patterns, unit/integration/production test guidelines
- [Technology stack](docs/dev/guides/tech-stack.md) — core technologies, integration patterns
- [Changelog and changeset hygiene](docs/dev/guides/changelog-merge-hygiene.md) — one file per entry in `changelog/`, `changesets/`; no index
- [Continuous integration](docs/dev/guides/ci.md) — what each PR check runs, how to query results, what the gate deliberately skips

## Documentation Hierarchy

- `packages/*/docs/` — Technical implementation (HOW) per package
- **`packages/*/docs/code-issues/`** — **Standing analyzer and structural findings**, one file per
  issue, named `<category>-<file-slug>[-<symbol>].md` with **no date** (a finding is a property of
  the code, so a re-run updates the file that already names that symbol). Written by
  `/analyze-code-issues`, read by planning **Step 2b**. An issue carrying `**Claimed by:** #NNN` has
  a PR already in flight to fix it — a change landing in that code **stops and asks** the developer
  whether to proceed and add to the debt, wait for that PR, or narrow scope. **Deleted at wrap once
  closed** — with the final measurement recorded in the change-history entry first — so the listing
  is always the open set; a **partly** fixed record is narrowed, never deleted. Policy:
  [`deferred-work`](.agents/skills/deferred-work/SKILL.md)
- `docs/ft/` — Product requirements (WHAT) by product area
- `docs/dev/1-WIP/` — Active changesets (cross-package deltas)
- `docs/dev/changesets/` — Cross-package changeset history: **one file per changeset**, `YYYY-MM-DD-<slug>.md`. Add a new file; never append to an existing one, and never add an index
- `docs/dev/guides/` — Cross-cutting technical guides
- **`plans/`** (repo root, optional) — Persisted **grill-me** **Create plan** output (the brief: problem, Q&A, analysis, preliminary plan) for version control in the working copy. Use a descriptive basename, e.g. **`plans/<feature-slug>-grill-me-brief.md`**. If a feature PRD or guide in **`docs/ft/`** specifies a different path under the repo, use that instead. If nothing is specified, default to **`plans/<SOME-PLAN-NAME>.md`** (replace `<SOME-PLAN-NAME>` with a stable, human-readable label for the effort). Session-scoped **`artifacts/grill-me-brief.md`** remains the runtime path during the session; **`plans/`** is the documented convention for copying or checking in the same content for the team repo.

<!-- CODEGRAPH_START -->
## CodeGraph

In repositories indexed by CodeGraph (a `.codegraph/` directory exists at the repo root), reach for it BEFORE grep/find or reading files when you need to understand or locate code:

- **MCP tool** (when available): `codegraph_explore` answers most code questions in one call — the relevant symbols' verbatim source plus the call paths between them, including dynamic-dispatch hops grep can't follow. Name a file or symbol in the query to read its current line-numbered source. If it's listed but deferred, load it by name via tool search.
- **Shell** (always works): `codegraph explore "<symbol names or question>"` prints the same output.

If there is no `.codegraph/` directory, skip CodeGraph entirely — indexing is the user's decision.
<!-- CODEGRAPH_END -->
