# Chain PR integration base — production readiness validation

## Executive summary

`tddy-core` implements chain-PR integration base handling in `worktree.rs`: strict validation (`validate_chain_pr_integration_base_ref`), scoped fetch (`fetch_chain_pr_integration_base`), session setup with optional user base (`setup_worktree_for_session_with_optional_chain_base`), and resume resolution (`resolve_persisted_worktree_integration_base_for_session`). `changeset.rs` persists `effective_worktree_integration_base_ref` and `worktree_integration_base_ref` with appropriate serde defaults.

The library layer is directionally sound for production use (validation before `git`, no shell interpolation). **Gaps for “prod-ready” end-to-end behavior:** (1) the legacy helper `setup_worktree_for_session_with_integration_base` does **not** persist effective/user integration-base fields, so observability and resume parity differ from the optional-chain API; (2) **no product entry point** (`tddy-service` daemon, `tddy-workflow-recipes` hooks) calls `setup_worktree_for_session_with_optional_chain_base` or threads a chain-base parameter from RPC/UI—production paths still use `setup_worktree_for_session` only; (3) default-path setup performs **redundant fetches** (`git fetch origin` inside default resolution, then `git fetch origin <branch>`). Address wiring and persistence parity before treating chain PR base as a shipped feature.

---

## Checklist

| Area | Status | Notes |
|------|--------|--------|
| Ref validation (injection / unsafe args) | **Pass** | `origin/` prefix, segment rules, forbids `..`, `--`, whitespace, common shell metacharacters; args passed via `Command::args` (no shell). |
| Git invocation safety | **Pass** | Literal argv; validated ref fragments only. |
| Error typing / user vs internal | **Partial** | Uniform `Result<_, String>`; useful messages but no structured error type or stable codes for UI. |
| Logging level & content | **Partial** | `info` for operations and paths; `debug` for outcomes and fetch stderr snippets. Ref names logged at info—usually acceptable; no obvious token leakage. |
| Configuration | **Partial** | Remote is hardcoded `origin`; no env/config override (consistent with existing integration-base helpers). |
| Serialization / schema | **Pass** | Optional fields, `skip_serializing_if`, `Default` in `Changeset`. |
| Persistence parity (legacy vs new API) | **Fail** | `setup_worktree_for_session_with_integration_base` omits `effective_worktree_integration_base_ref` / `worktree_integration_base_ref`. |
| Performance (fetch) | **Partial** | Redundant `fetch origin` + `fetch origin <branch>` on default optional-chain path. |
| Operational / resume | **Partial** | `resolve_persisted_*` prefers persisted effective ref; does not re-validate ref shape on read (trust on-disk YAML). |
| Daemon / CLI / RPC integration | **Fail** | Daemon and recipe hooks use `setup_worktree_for_session` only—optional chain base not exposed. |

---

## Findings

### 1. Validation and security

- **`validate_chain_pr_integration_base_ref`** allows multi-segment paths under `origin/` and rejects empty segments, `..`, `--`, whitespace, and a set of shell-oriented characters. This aligns with passing a single refspec fragment to `git fetch origin <branch_path>` without shell interpretation.
- **Residual edge cases:** Ref names may still include characters Git treats specially in other commands (not covered here). Narrowing to a Git ref-name charset (e.g. rejecting `*`, `?`, `[`, `@` where inappropriate) would reduce surprise if these strings are ever forwarded to broader git plumbing. Not blocking for the current `fetch` argv usage.
- **`validate_integration_base_ref`** (single segment) remains the gate for non-chain `fetch_integration_base`; chain path uses the separate validator—clear separation.

### 2. Error handling

- Public APIs return **`Result<_, String>`**. Callers (e.g. daemon) surface the string to workflow completion. There is no distinction between “user fixable” (bad ref) and infrastructure (git missing, disk). Acceptable for an internal library; product layer may want typed errors later.
- Failure paths attach **full `git` stderr** to the returned `Err` in several places—appropriate for operators; ensure UI does not echo raw stderr to untrusted telemetry without scrubbing if that becomes a requirement.

### 3. Logging

- **`log::info`** records repo root, session dir, integration ref strings, and boolean opt-in. Paths and branch/ref names are operational data, not credentials.
- **`log::debug`** records fetch stderr on failure—reasonable; avoids noisy logs on success paths.
- User-selected chain base is logged at **info** when `Some`—intentional audit trail; confirm product privacy expectations for branch names in shared logs.

### 4. Configuration

- **`origin`** is fixed in all fetch helpers. Multi-remote repos cannot select a remote via config—consistent with existing design, not a regression for this feature.

### 5. Performance and operations

- **`setup_worktree_for_session_with_optional_chain_base(None)`** calls **`resolve_default_integration_base_ref`**, which runs **`git fetch origin`** (no refspec), then **`fetch_integration_base`**, which runs **`git fetch origin <single-branch>`**. The second fetch is often redundant immediately after a full origin fetch. Consider deduplicating (e.g. resolve without implicit fetch, or pass “already fetched” state)—operational efficiency, not correctness.
- **`resolve_persisted_worktree_integration_base_for_session`** may call **`resolve_default_integration_base_ref`** when no persisted base exists—another full `git fetch origin`. Callers should avoid hammering this in tight loops.

### 6. Persistence and legacy API (cross-check)

- **`setup_worktree_for_session_with_optional_chain_base`** sets **`cs.effective_worktree_integration_base_ref`** always and **`cs.worktree_integration_base_ref`** only when the user supplied a chain base (`Some`). Matches documented intent.
- **`setup_worktree_for_session_with_integration_base`** updates worktree/branch/repo_path but **does not** set `effective_worktree_integration_base_ref` or `worktree_integration_base_ref`. Any session created through this path (or through **`setup_worktree_for_session`**, which delegates to it) will **not** record the effective base in the changeset. Resume and observability for those sessions lag the optional-chain API unless callers are updated or the legacy function gains the same writes.

### 7. RPC / product wiring (cross-check)

- **`packages/tddy-service/src/daemon_service.rs`** uses **`setup_worktree_for_session`** after planning—no parameter for chain PR base, no call to **`setup_worktree_for_session_with_optional_chain_base`**.
- **`packages/tddy-workflow-recipes/src/tdd/hooks_common.rs`** (`ensure_worktree` path) also calls **`setup_worktree_for_session`** only.
- No grep hits for the optional-chain API outside **`tddy-core`** and **integration tests**. End users cannot select a chain base through current daemon/workflow entry points until RPC/session context carries that field and the worktree hook reads it.

---

## Prioritized recommendations

1. **P0 — Wire product entry points:** Extend session/RPC/context (where workflow receives user intent) with an optional chain PR base ref; call **`setup_worktree_for_session_with_optional_chain_base`** from the same places that create worktrees today when that value is present. Until then, the feature exists only for direct library/test callers.

2. **P0 — Persistence parity for legacy path:** In **`setup_worktree_for_session_with_integration_base`**, set **`effective_worktree_integration_base_ref`** to the explicit ref used (and **`worktree_integration_base_ref`** only if you later distinguish “user selected” vs “caller supplied”). Alternatively, document that only the optional-chain API persists these fields and accept inconsistent changesets—less desirable for resume tooling.

3. **P1 — Reduce redundant fetches:** After **`resolve_default_integration_base_ref`**, skip **`fetch_integration_base`** if the default resolution already performed an equivalent fetch, or split “resolve ref name” from “fetch” so hooks can fetch once.

4. **P2 — Optional validation on read:** When loading persisted refs for downstream git use, re-run **`validate_chain_pr_integration_base_ref`** / **`validate_integration_base_ref`** so tampered `changeset.yaml` fails fast with a clear error.

5. **P2 — Error taxonomy:** Introduce a small enum or structured error for worktree/git failures if the web/UI must map to specific user messages.

---

## Return to parent

- **File written:** yes  
- **Path:** `/var/tddy/Code/tddy-coder/.worktrees/chain-pr-base-branch/plans/validate-prod-ready-report.md`  
- **One-line summary:** Core validation and optional-chain setup are solid, but legacy setup omits persisted effective ref, default path double-fetches, and daemon/workflow still only call `setup_worktree_for_session`—chain PR base is not wired for production.
