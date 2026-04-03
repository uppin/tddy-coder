# Session actions (`tddy-tools`)

**Status:** 🚧 In Progress

**Packages:** `tddy-tools`

**Summary:** YAML-defined session actions under `session_dir/actions/`, JSON Schema validation, Cmd argv interpolation, built-in `acceptance_sweep`, CLI `tddy-tools actions list|run`. PRD: session artifact `artifacts/PRD.md` (019d38e2-…).

## Implementation Progress

**Last synced with code:** 2026-04-03 (via @validate-changes)

**Core features**

- [x] `session_actions` module (`discovery`, `validation`, `interpolation`, `run_session_action_json`) — ✅ Complete (`packages/tddy-tools/src/session_actions/`)
- [x] CLI `actions list` / `actions run` — ✅ Complete (`cli.rs`, `main.rs`)
- [x] Cmd executor + argv templates — ✅ Complete
- [x] Built-in `acceptance_sweep` runner — ✅ Complete
- [~] MCP executor in `run_session_action_json` — ⚠️ **Not wired** (`executor.type: mcp` bails at runtime; `map_mcp_tool_arguments` exists for mapping/tests)
- [~] `actions list` output — ⚠️ Docstring mentions “schema summaries”; JSON currently exposes `id` only

**Testing**

- [x] Unit tests (`actions_interpolation_unit.rs`) — ✅ Complete (6 tests)
- [x] Integration tests (`actions_cli_integration.rs`) — ✅ Complete (3 tests)

**Repo state**

- [ ] Git commit — 🔲 Not done (branch `feature/session-actions-tddy-tools`; core module + tests untracked or uncommitted)

## Change Validation (@validate-changes)

**Last run:** 2026-04-03  
**Status:** ⚠️ Warnings (see below)  
**Risk level:** 🟡 Medium (feature gaps + uncommitted work, not security-critical)

**Changeset sync**

- 🆕 Changeset created (no prior `docs/dev/1-WIP/` entry for this work)

**Analysis summary**

- Packages built: `tddy-tools` — ✅ `cargo build -p tddy-tools`, ✅ `cargo clippy -p tddy-tools -- -D warnings` (via `./dev`)
- Tests: ✅ `cargo test -p tddy-tools` — all tests passed (including 9 session-actions tests)
- Files analyzed: `Cargo.toml`, `cli.rs`, `lib.rs`, `main.rs`, `session_actions/*.rs`, `tests/actions_*.rs`

**Risk assessment**

| Area              | Level | Notes |
|-------------------|-------|--------|
| Build validation  | 🟢 Low | Clean build + clippy |
| Test infrastructure | 🟢 Low | No test-only production branches observed |
| Production code   | 🟡 Medium | MCP run path incomplete; list output vs docs |
| Security          | 🟢 Low | Input validated against schema before argv/spawn; templates bound to validated JSON |
| Code quality      | 🟢 Low | No clippy -D warnings; watch function size in `mod.rs` over time |

### Refactoring / follow-ups

- [ ] Implement or explicitly document MCP execution path for `actions run` (or narrow PRD scope)
- [ ] Align `actions list` JSON with “schema summaries” or adjust help text
- [ ] Commit and push feature branch; reconcile with `origin/master` (branch behind)
