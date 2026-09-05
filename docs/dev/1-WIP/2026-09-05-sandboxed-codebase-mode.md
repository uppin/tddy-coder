# Changeset: sandboxed-codebase-mode

PRD: [`docs/ft/coder/sandboxed-codebase-mode.md`](../../ft/coder/sandboxed-codebase-mode.md)
Reuses: [`docs/ft/daemon/remote-codebase-mode.md`](../../ft/daemon/remote-codebase-mode.md) § Workspace tool sandbox (#427, landed).

## Responsibility

A third `--codebase-mode` for `tddy-sandbox-app` on macOS: **`sandboxed`**, which inverts the
placement — Claude Code runs on the host as an ordinary child process, the checkout and every build
run inside a `--workspace-tools` jail, and the only route between them is `mcp__tddy-tools__*`
dispatched as `in_jail_tool_request`.

Five deltas:

1. **`tddy-sandbox-app`** — `resolve_codebase_mode` returns a three-valued `CodebaseMode` instead of
   a bool; mounts, runner argv and the `run_macos` flow branch on it; a new `host_agent` module
   serves the host tool-IPC socket and spawns host `claude` with inherited stdio.
2. **`tddy-sandbox-runner` (binary)** — `--workspace-tools` no longer requires `--stdio` (the app's
   loopback-gRPC transport serves it too), and starts the CONNECT egress shim when
   `--egress-shim-port` is given.
3. **`tddy-sandbox-runner` (host relay)** — a dispatcher for sending `in_jail_tool_request` over a
   running relay, one call outstanding at a time.
4. **`tddy-sandbox-recipes`** — the host-agent argv builder: same `mcp__tddy-tools__*` allowlist,
   plus a full native-tool withdrawal that does *not* withdraw the MCP forms.
5. **PRD** — new feature doc, above.

## Boundaries

- Does **not** touch `mounted` or `managed`. Every existing flag, default and alias keeps its
  meaning; the only shared-code change is `resolve_codebase_mode`'s return type.
- Does **not** touch the Linux path beyond refusing the new mode with a clear message.
- Does **not** build a second jail: the `--workspace-tools` runner mode and the
  `in_jail_tool_request` / `in_jail_tool_response` frames both already exist and ship today. This
  changeset gives the standalone app a way to drive them.
- Does **not** change `tddy-tools`. `SessionToolTransport::SandboxIpc` already keys off
  `TDDY_SANDBOX_TOOL_IPC`; who serves that socket was never its concern.
- Does **not** re-home specialized subagents onto the host MCP server — the combination is refused.
- Does **not** consolidate `tddy_daemon::workspace_tool_sandbox`'s own in-jail exchange onto the new
  relay dispatcher.
- Does **not** touch `--agent-kind cursor`.

## Public API this changeset defines

| Crate | Item | Contract |
|---|---|---|
| `tddy-sandbox-app` | `enum CodebaseMode { Mounted, Managed, Sandboxed }` | replaces the `bool` |
| `tddy-sandbox-app` | `resolve_codebase_mode(Option<&str>, bool) -> Result<CodebaseMode, String>` | AC 1 |
| `tddy-sandbox-app` | `build_sandbox_mounts(CodebaseMode, &Path, &Path) -> Vec<MountSpec>` | AC 2 |
| `tddy-sandbox-app` | `build_workspace_tools_runner_argv(WorkspaceToolsRunnerArgs) -> Vec<String>` | AC 3 |
| `tddy-sandbox-app` | `host_agent::host_mcp_env(&Path, Option<&str>, Option<&Path>) -> BTreeMap<String, String>` (crate-internal) | AC 8 — the config file itself is written by `tddy_sandbox_recipes::write_claude_mcp_config`, through `append_host_agent_mcp_args` below |
| `tddy-sandbox-app` | `host_agent::build_host_agent_argv(HostAgentArgs) -> Result<Vec<String>>` | AC 7 |
| `tddy-sandbox-app` | `sandboxed_session::serve_host_tool_ipc(&Path, &Path, Arc<InJailToolDispatcher>) -> Result<JoinHandle<()>>` (private; reached through `provision`) | AC 8 |
| `tddy-sandbox-app` | `sandboxed_session::provision_with_interrupt(params, impl Future) -> Result<…>` | AC 12 |
| `tddy-sandbox-app` | `sandboxed_session::ProvisioningInterrupted` | AC 12 |
| `tddy-sandbox-app` | `SandboxedCodebaseSession::host_dir() -> &Path` | AC 11 |
| `tddy-sandbox-runner` | `InJailToolDispatcher::execute(&ExecuteToolRequest) -> ExecuteToolResponse` | AC 6 |
| `tddy-sandbox-runner` | `run_host_relay_with_in_jail_tools(...) -> Result<(JoinHandle<()>, InJailToolDispatcher)>` | AC 6 |
| `tddy-sandbox-runner` (bin) | `--workspace-tools` + gRPC transport, + optional `--egress-shim-port` | AC 4, 5 |
| `tddy-sandbox-recipes` | `build_host_agent_disallowlist() -> Vec<String>` | AC 7 |
| `tddy-sandbox-recipes` | `append_host_agent_mcp_args(...) -> Result<()>` | AC 7 |

## Acceptance tests (Step 6)

`packages/tddy-sandbox-app/tests/sandboxed_codebase_seatbelt_acceptance.rs` — real Seatbelt jail,
driven through the host tool-IPC socket exactly as the host `tddy-tools --mcp` would (AC 9):

1. `a_write_dispatched_from_the_host_lands_in_the_checkout_inside_the_jail`
2. `a_shell_dispatched_from_the_host_runs_with_the_checkout_as_its_working_directory`
3. `a_read_of_a_host_file_outside_the_checkout_is_refused_by_the_jail`
4. `a_build_inside_the_jail_reaches_the_network_through_the_host_connect_relay`

## Unit / integration tests (Step 7)

- `codebase_mode.rs` — AC 1 (six existing tests re-expressed against `CodebaseMode`, plus
  `sandboxed`, the `sandboxed` + `--remote-codebase` contradiction, and the error naming all three).
- `spawn.rs` — AC 2, AC 3.
- `host_agent.rs` — AC 7, AC 8.
- `tddy-sandbox-recipes/src/claude_cli.rs` — AC 7.
- `tddy-sandbox-runner` — AC 4, AC 5, AC 6.
- `main.rs` (linux cfg) — AC 10.

## TODO

- [x] Create/update PRD documentation — `docs/ft/coder/sandboxed-codebase-mode.md`
- [x] Create changeset — this document
- [x] Failing acceptance tests (Step 6 — `/plan-red`)
- [x] Failing unit/integration tests (Step 7 — red phase)
- [x] Implement production code making tests pass (`/green`)
- [x] Apply PRD status Planned → Implemented (`/wrap-context-docs`)
- [x] Add `docs/dev/changesets/2026-09-05-sandboxed-codebase-mode.md` (`/wrap-context-docs`)
      — master replaced the `changesets.md` index with one file per changeset and no index at all

## Validation Results

Final, 2026-09-05, after four rounds of pre-PR review (`/validate-changes`, `/validate-tests`,
`/validate-prod-ready`, `/analyze-clean-code`).

| Target | Result |
|---|---|
| `tddy-sandbox`, `-app`, `-runner`, `-recipes`, `-darwin` | **352 passed, 0 failed** |
| `tddy-daemon` real-jail suites (6) | **30 passed, 0 failed** |
| `cargo build --workspace --tests` | clean |
| `cargo clippy --all-targets -- -D warnings` (7 packages) | clean |
| `cargo fmt --all -- --check` | clean |
| Live end-to-end (`--codebase-mode sandboxed`, real `claude`, real jail) | agent answers through jailed tools; build cache reachable at `~/.tddy/sandbox-codebase-home/<repo-key>` |

**Pre-existing failures, confirmed against a clean `HEAD` worktree and unrelated to this branch:**
`sandbox_behavior_acceptance` (5) and `sandboxed_session_lifecycle_acceptance` (2), both panicking
with `ConnectionServiceImpl::self_arc called before set_self_handle` — a daemon test-harness gap.

### What review found that the tests did not

Four confinement holes reached this branch and were fixed before it left:

| Finding | Fix |
|---|---|
| The jail's egress shim was **dead wiring** — no proxy env inside the jail, so a jailed build had no network at all | proxy env threaded to the tool engine; proved by a `curl` from *inside* the jail |
| Ctrl-C during provisioning **orphaned the jail** (`SandboxHandle` has no `Drop`, then `exit(130)`) | interrupt moved inside `provision`'s ready-wait |
| The untrusted checkout's `.claude/settings.json` hooks and `.mcp.json` **executed unconfined on the host** | `--strict-mcp-config` + `--setting-sources user` |
| The jail could **rewrite the host agent's MCP config**, whose `command`/`env` the unconfined host runs | config moved to `<session_dir>/host/`, outside every grant |

And a required test found a fifth, in the feature this changeset exists for: the per-repo build home
landed under a base **no grant named**, so the jail could not look up its own `$HOME` and the
persistent cache was unreachable in production. Every earlier test had hidden it by placing the home
in the same temp tree as the checkout, whose ancestors *are* granted.

Five tests in this suite passed while proving nothing and were repaired: two egress tests (one dialed
the shim from the host, one targeted a `NO_PROXY`-excluded address), the ancestor-listing test
(resolved a symlink before reaching the listing), the agent-home test (`ENOENT` on a host without
`~/.claude` satisfied both assertions), and the concurrency guard in the fake (structurally
unfalsifiable). Every confinement assertion now carries an in-test control or an explicit premise.

**`./test` never built `tddy-sandbox-runner`.** The jail acceptance suites spawn it as a binary from
`target/debug`, and cargo sets no `CARGO_BIN_EXE_` for packages that do not own it, so a change to
the runner was tested against whatever build happened to be on disk. Fixed in `./test`. This
weakened every jail acceptance test in the repo, not only this feature's.

## Status

Implementation complete and reviewed. Ready for PR; this WIP source is removed after it lands.
