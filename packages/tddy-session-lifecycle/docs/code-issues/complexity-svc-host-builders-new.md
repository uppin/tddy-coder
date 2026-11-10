# complexity: DaemonSessionHost::new

**Location:** `packages/tddy-session-lifecycle/src/connection_service/svc_host_builders.rs:33` — `DaemonSessionHost::new`
**Category:** complexity
**Detected:** 2026-11-10, `#agent-tools` sweep (#876) structural re-measurement
**Metrics:** **97 lines** (2026-11-10) · nesting depth 1 · 0 branch keywords · 10 parameters · 0 early returns
**Thresholds breached:** length 97 > 60 (`/analyze-clean-code`); depth and complexity within budget
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — deferred by the user, 2026-11-10 (#876 sweep); unclaimed

Deferred by the user, 2026-11-10 (#876 sweep): the restructure engine refused the seam (missing imports for builder types / early returns in the range); revisit after the engine fix

## Measurement history

| Run | Lines | Nesting | Note |
|---|---|---|---|
| 2026-11-10 | 112 → 97 | 1 | #876 Tier C: `build_host_stores` and `default_agent_tool_sockets` extracted. The body is now a ~78-line `Self { … }` builder literal; every field-expression range offers only "extract into variable", in both `extract_method` variants. |

## What would close it

Bring it under 60 lines once the engine can extract a builder-literal field group (or a
builder/Default refactor is consented to). Anchor with `tddy-tools restructure anchors`, never
by hand, then prove the seam with `restructure check --deep` against a warm index.
