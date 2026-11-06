# 2026-11-02 — Declared tools resolve through a registry; the workspace jail runs rg through one

**Type:** Feature · **Stack:** `#agent-tools` 1/12 ([#790](https://github.com/uppin/tddy-coder/pull/790)) · **Packages:** tddy-actions, tddy-sandbox-recipes, tddy-daemon-sandbox, tddy-sandbox-darwin, tddy-session-lifecycle, tddy-session-split (tests)

State A: the workspace jail ran `Grep` through `host_ripgrep_grants`, a `FIXME(grep-in-jail)` patch that granted the host `rg`'s whole directory and granted nothing (so every `Grep` failed with `spawn failed`) when `rg` was missing.

State B: `tddy-actions` gains the `tool` module (YAML `ToolDef`, `ToolRegistry` with embedded built-ins plus `<tddyhome>/tools` overrides, `ToolHost` resolution of a dep closure). `tddy-sandbox-recipes::tool_grants::lower_tools` lowers a resolved set to a literal exec read of each binary, its loader reads, and a jail-local `bin/` of links first on `PATH`. `JailedWorkspaceSandboxProvisioner::for_tddy_home` loads and resolves `WORKSPACE_JAIL_TOOLS = ["rg"]` at each jail start and refuses with `InvalidSpec` naming the tool. `tddy-sandbox-darwin` emits `process-exec*` for exec literals. `host_ripgrep_grants` and `tests/ripgrep_in_jail_seatbelt.rs` are deleted (superseded by `declared_tools_workspace_jail_acceptance.rs`). A closure with two tools sharing one `binary.command` is refused (`DuplicateCommand`).

Resolved backlog: `docs/dev/todo/2026-09-26-grep-is-unreachable-inside-every-jail.md` (deleted). Code issues: `oversized-file-workspace-tool-sandbox.md` 741 → 684, narrowed; `oversized-file-svc-host-builders.md` added at 503 (split deferred until the stack lands — #791 edits the file). Not exercised locally: Linux (CI covers it).

Product: [sandbox-builder.md § Declared tools](../../ft/coder/sandbox-builder.md#declared-tools), [remote-codebase-mode.md](../../ft/daemon/remote-codebase-mode.md). Package: [tddy-actions declared-tools.md](../../../packages/tddy-actions/docs/declared-tools.md).

## Validation (2026-11-02, `/validate-changes` + `/pr-wrap`, macOS)

- Stack gate: base `master`, already current; `origin/master..HEAD` is this PR's commits only.
- Gaps closed in wrap: `ToolError::DuplicateCommand` (two tools in one closure with one `binary.command`); an empty `PATH` search reads `PATH (empty)`; the sibling-binary Seatbelt test now has a positive `rg --version` control and requires `Operation not permitted`.
- Developer decisions taken at green: `-n` added to the acceptance test's rg argv; `tests/ripgrep_in_jail_seatbelt.rs` deleted as superseded.
- Scoped gates: `./test -p tddy-actions` 24 passed; `./test -p tddy-sandbox-recipes` 70 passed; `./test -p tddy-sandbox-darwin` 30 passed; `cargo test -p tddy-daemon-sandbox` (lib + every test target except `sandbox_stdio_seatbelt_acceptance`, which does not compile on macOS on master) 46 passed; clippy `-D warnings` clean on all four.
