# oversized-file: svc_resolve_os_user.rs

**Location:** `packages/tddy-session-lifecycle/src/connection_service/svc_resolve_os_user.rs`
**Category:** oversized-file
**Detected:** 2026-09-23 — `/pr-wrap` file-length gate on #508 (`#keyring` 1/9)
**Metrics:** **539 production lines** (539 total — the file has no `#[cfg(test)]` module) · budget 500 · **1.08× over**
**Thresholds breached:** length 539 > 500
**Restructure:** `extract_module --to_file` or a method move — not designed yet
**Status:** Open — **unclaimed** · pre-existing (538 on master); #508 added one line

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-23 | 539 | master 538 → 539 after #508 (`#keyring` 1/9: the exec-tool refusal's doc names the unlearned signing key instead of the shared secret) — split deferred to a follow-up after #keyring lands because dependents #509–#513 touch it |

## What the gate found

The whole file is one `impl DaemonSessionHost` block (from `:40`), so there is no module-level seam
for `extract_module` to address: closing it means moving a cohesive group of methods — the
exec-tool caller authorization is the obvious candidate — into a sibling `svc_*` module, the
pattern the rest of `connection_service/` already follows. 39 lines over budget; the cheapest
record in this directory to close.
