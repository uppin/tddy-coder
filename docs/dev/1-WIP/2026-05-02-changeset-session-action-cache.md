# Changeset: Session workflow action cache (`tddy-tools submit` replay)

**Date:** 2026-05-02  
**Status:** 🚧 In Progress *(implementation in working tree; not yet committed)*  
**Type:** Feature (workflow engine + integrations + tests)

## Product behavior

- **`BackendInvokeTask`** may **skip `CodingBackend::invoke`** when a prior successful **`tddy-tools submit`** result for the same session is persisted under **`{session_dir}/.workflow/action-cache.json`**, keyed by **graph × task × goal** and a **deterministic fingerprint** of goal prompt (and optional system prompt / model).
- **Opt-out**: context flag **`disable_action_cache`** or environment **`TDDY_DISABLE_ACTION_CACHE`** (`1`, `true`, `yes`).
- **`MockBackend`** is **not cache-eligible** (**`action_invoke_cache_eligible` → false**) so queued mock invokes stay in lockstep with test harness submits.
- **`FlowRunner`** injects **`workflow_engine_graph_id`** and **`workflow_engine_current_task_id`** into the **`Context`** for stable cache identity.
- Persistence: **atomic write-then-rename**; malformed on-disk JSON is reset with a logged warning.

## Affected packages

| Package | Role |
|---------|------|
| `tddy-core` | `workflow::action_cache`, `BackendInvokeTask`, `CodingBackend`, mock/stub, session action plumbing tweaks |
| `tddy-integration-tests` | Workflow graph acceptance for cache hit/miss/restart/`WaitForInput` |
| `tddy-tools` | CLI formatting-only + tests aligned with cache behavior |

## Affected documentation (*planned — not authored in this slice*)

- `docs/dev/changesets.md` — index row when promoting from 1-WIP.
- Feature / product docs as decided by stakeholders (no PRD matched this branch in `docs/ft/*/1-WIP/`).

## References

- `packages/tddy-core/src/workflow/action_cache.rs`
- `packages/tddy-core/src/workflow/task.rs` (`BackendInvokeTask`)
- `packages/tddy-integration-tests/tests/workflow_graph.rs`

---

## Implementation Progress

**Last Synced with Code**: 2026-05-02 (via `@validate-changes`)

**Core features**:

- [x] **Per-session action cache module** (`action_cache.rs`) — ✅ Complete *(fingerprinting, atomic persist, lookup, env/context opt-out; unit tests in-module)*
- [x] **Workflow integration** — ✅ Complete *(pre-invoke lookup, post-submit persist in `BackendInvokeTask`; runner context keys)*
- [x] **Backend eligibility** — ✅ Complete (`CodingBackend::action_invoke_cache_eligible`; `MockBackend` → `false`; real backends default `true`; `StubBackend::invocation_count_snapshot` for tests)
- [x] **Integration tests** — ✅ Complete *(new/updated scenarios in `workflow_graph.rs`)*
- [x] **Tools / CLI tests** — ✅ Complete *(acceptance updates; formatting-only churn in `session_actions_cli.rs`)*
- [ ] **Cross-package changelog / docs index** — 🔲 Not started *(per repo changeset hygiene when merging)*

**Testing** (workspace):

- [x] `./verify` (**2026-05-02 — PR-wrap**): ✅ `cargo test` **exit 0** (~518s); `.verify-result.txt` shows no failures.

**`/green`** was previously skipped *(no RED failures)* — workspace lint debt later cleared during **PR Wrap** (below).

---

### Change Validation (@validate-changes → PR-wrap)

**Last Run**: 2026-05-02 (final pass after `./dev cargo fmt` / workspace clippy / `./verify`)

**Status**: ✅ Passed — workspace **`cargo clippy --workspace --all-targets -- -D warnings`** clean; **`./verify`** exit 0

**Risk Level**: 🟢 Low *(build + lint + tests; feature semantics unchanged by lint-only / test-structure edits outside action-cache core)*

**Changeset sync**: Still 🚧 1-WIP — this document remains the authoritative working delta for session action cache until **`/wrap-context-docs`** runs post-merge coordination.

**PR preparation** *(command: PR Wrap)*:

| Step | Result |
|------|--------|
| 1 Validate changes + refactor-style fixes | ✅ Clippy hygiene on branch-related code + workspace fallout (see SCM diff) |
| 2 Validate tests | ✅ Resolved `manual_contains`, `doc_lazy_continuation`, `field_reassign_with_default`, `useless_conversion` in touched tests |
| 3 Prod readiness | ✅ Removed redundant `#[allow(dead_code)]` from `action_cache.rs`; gated macOS-only imports in `tddy-livekit-screen-capture` |
| 4 Clean code | ⚠️ `BackendInvokeTask` cache block still long — optional helper extraction deferred |
| 5 Re-validate | ✅ This section |
| 6 `cargo fmt` + clippy + test | ✅ |
| 7 Wrap docs | ⚠️ **Not executed** (`/wrap-context-docs`): changeset stays 1-WIP until maintainer merges index + stable dev docs |

**Build / lint**:

- `./dev cargo fmt --all`
- `./dev cargo clippy --workspace --all-targets -- -D warnings` — ✅
- `./dev bash -lc './verify'` — ✅ (~518s)

**Risk snapshot** *(unchanged product concerns)*:

| Area | Level | Notes |
|------|--------|-------|
| Test infrastructure | Low | Mock not cache-eligible |
| Production logic | Medium-low | FNV fingerprint collision risk (by design); single-writer assumption on cache file |
| Security | Low | Session-local cache artifact only |

---

## Refactoring / follow-ups

### From PR-wrap / refactoring *(carry-forward)*

- [ ] **Repository hygiene**: do **not** commit untracked root artifacts (`.action-cache-red-test-output.txt`, `.red-submit-session-action-cache.json`, `.verify-green-full.txt`).
- [ ] **Optional**: extract **`BackendInvokeTask`** cache hit / persist into private helpers *(readability)*.
- [ ] **`/wrap-context-docs`**: prepend **`docs/dev/changesets.md`** + stable package doc bullets when merging; archive this WIP changeset.

**Scoped Clippy allowances** *(documented defaults for verbose test setups)*:

- `tddy-daemon`: `#![allow(clippy::field_reassign_with_default)]` on `tests/telegram_session_control_integration.rs`, `#[allow(...)]` on `telegram_notifier` `acceptance_unit_tests` submodule.
