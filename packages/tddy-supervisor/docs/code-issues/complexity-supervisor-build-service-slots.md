# complexity: build_service_slots

**Location:** `packages/tddy-supervisor/src/supervisor.rs:328` — `build_service_slots`
**Category:** complexity
**Detected:** 2026-11-10, `#agent-tools` sweep (#876) wrap re-measurement (brace matching, fn line to closing brace; nesting read by hand as control-flow depth)
**Metrics:** **65 lines** (2026-11-10) · nesting depth 3
**Thresholds breached:** length 65 > 60 (`/analyze-clean-code`); depth within budget
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — deferred by the user, 2026-11-10 (#876 wrap); unclaimed

Deferred by the user, 2026-11-10 (#876 wrap): left for a later refactoring node

## Measurement history

| Run | Lines | Nesting | Note |
|---|---|---|---|
| 2026-11-10 | 65 | 3 | new in #876: extracted from `Supervisor::new` (Tier C, 80 → 22); it holds the per-service user/group resolution, root refusal and listener binding |

## What would close it

Extract the per-service credential resolution (user, `group:` override, root refusal) from the listener binding. Anchor with `tddy-tools restructure anchors`, never by hand, then prove the seam with
`restructure check --deep` against a warm index.
