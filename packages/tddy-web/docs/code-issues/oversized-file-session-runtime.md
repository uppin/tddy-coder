# oversized-file: SessionRuntime.tsx

**Location:** `packages/tddy-web/src/components/sessions/SessionRuntime.tsx`
**Category:** oversized-file
**Detected:** 2026-10-20 by the `/pr-wrap` step 3.5 file-length gate on PR #657 (`#host-desktops` 5/8)
**Metrics:** **646 lines** (whole file, TS) · budget 500 · **1.29× over** — `wc -l packages/tddy-web/src/components/sessions/SessionRuntime.tsx`
**Thresholds breached:** length 646 > 500
**Status:** Open — unclaimed; 646 (2026-10-20)

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-10-20 | 646 | first detection — already over before #657; #657 renamed one import and one call (`requestFullscreenForConnectedTerminal` to `requestElementFullscreen`) and did not grow it (646 at its base `feature/host-desktops/hosts-mount`). Alert only; no split attempted |

## What would close it

Not yet analysed. A skim shows one exported component `SessionRuntime` (from line 98, about 450 lines) followed by the `SessionChildRuntimeProps` interface and the `SessionChildRuntime` component (from line 556), so the child-runtime pair is a plausible first seam to move into a sibling file. What the `SessionRuntime` body would split into was not examined. The `code-restructuring` engine is Rust-only, so this is a hand TS split: green baseline before, mechanical moves only.
