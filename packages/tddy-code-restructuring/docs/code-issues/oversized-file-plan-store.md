# oversized-file: plan_store.rs — the plan store: load/unload, op lookup, per-op refresh, flush policy

**Location:** `packages/tddy-code-restructuring/src/plan_store.rs`
**Category:** oversized-file
**Detected:** 2026-10-02 by the `/pr-wrap` file-length gate on #538
**Metrics:** **522 production lines** (2026-10-02, #538) · budget 500 · measured before the first `#[cfg(test)]`
**Restructure:** required — `extract_module --to_file` (`/code-restructuring` territory)
**Status:** Open — split deferred until the `#live-plan` stack lands; #539 also edits this file

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-02 | new → 522 | #538 (`#live-plan` 2/7) introduced the file: 1.04x the budget. `refreshed` (76 lines, nesting 5) is the largest function |

Reproduce, from the repo root:

```bash
awk '/#\[cfg\(test\)\]/{exit} {n++} END{print n}' packages/tddy-code-restructuring/src/plan_store.rs
```

## What the gate found

522 production lines in a file created by #538, 22 over budget. Three concerns share it: the
held-plan registry (load/unload/list/lookup), the per-op refresh of pending anchors, and the flush
policy (dirty tracking, debounce, clobber refusal, atomic write).

## Why it was not split in #538

#539 (`live-plans`) extends this file's refresh to every loaded plan. Splitting it inside #538 would
rewrite the paths #539 edits. The stack rule forbids it; the deferral stands until the stack lands.

## What would close it

After the `#live-plan` stack lands, `extract_module` the refresh (`plan_store/refresh.rs`: `refreshed`
and its helpers) and the flush policy (`plan_store/flush.rs`). Ordinary `/code-restructuring` work;
follow-up: `docs/dev/todo/2026-10-02-split-oversized-plan-store-rs.md`.

## Verified by hand

2026-10-02: counted with the command above: 522. Inline test modules begin after the counted region.
