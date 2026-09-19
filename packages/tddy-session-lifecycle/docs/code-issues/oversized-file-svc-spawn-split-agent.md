# oversized-file: svc_spawn_split_agent.rs

**Location:** `packages/tddy-session-lifecycle/src/connection_service/svc_spawn_split_agent.rs`
**Category:** oversized-file
**Detected:** 2026-09-19 — `/pr-wrap` step 3.5 file-length gate
**Metrics:** **502 production lines** (counted to the first `#[cfg(test)]`) · budget 500
**Thresholds breached:** length 502 > 500
**Restructure:** `extract_module --to_file`, after a two-brace impl split — seam designed, not applied
**Status:** Open — **unclaimed**
**Verified:** ⚠ seams hand-verified caller-free by grep; the cut itself is **not** proven by `check --deep`

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-19 | 502 | first detection — **this PR pushed it over**, 398 → 502 |

## What the tool found

The file is uses + a single `impl DaemonSessionHost`. No module-level items besides the impl, which
is why `restructure anchors` cannot express a seam inside it without help.

Two of its functions also breach the 60-line ceiling: `spawn_split_agent` **233** (was 213) and
`delete_paired_codebase_session` **85** (was 61).

## What would close it — designed seam

**Seam:** the trailing block — `tear_down_codebase_session`, `delete_paired_codebase_session`.
Neither is called by any sibling in this impl; every caller is in another file and reaches them
through the type (`svc_materialize_staged_attachment.rs:345,371,404`,
`session_coordinate_handlers.rs:630`), so the move needs no `reexport` and no caller churn.

**Operation:** two-brace impl split after `split_forward_deadline` (closing `}` at L346),
re-snapshot, then `extract_module --to_file` over the trailing block →
`svc_spawn_split_agent/codebase_session_teardown.rs`.

**Estimate:** ~156 lines out (+3 for the brace split) → parent **~349**.

## Constraints a later session must know

- `anchors` resolves **only module-level items** — `module_outline()`
  (`packages/tddy-code-restructuring/src/backends/rust.rs:1531`) never descends into `impl`
  children. `DaemonSessionHost::method` and bare `DaemonSessionHost` are both refused. Splitting the
  one inherent `impl` into two gives it a second module-level item to anchor; Rust allows any number
  of inherent impls and the split moves no code.
- `places_of` (`backends/rust.rs:2309`) resolves by **first match**, and both halves of a split
  inherent impl carry the same outline name `impl DaemonSessionHost`. Only the **leading** block is
  addressable by `--items`.
- The leading block here is the namesake `spawn_split_agent`, so extracting it would invert the
  file's meaning. The trailing range must be **derived**: start = leading block's `anchors` end + 2,
  end = the impl's closing brace at EOF. Gate with `check --deep` and a `--dry-run` diff.
- **`agent_session_token_for` cannot move.** `spawn_split_agent` calls it at L105, same impl — the
  one geometry `extract_module` refuses, and no ordering fixes it.
