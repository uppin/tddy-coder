# oversized-file: sandbox_session.rs

**Location:** `packages/tddy-daemon-sandbox/src/sandbox_session.rs`
**Category:** oversized-file
**Detected:** 2026-09-19 — `/analyze-clean-code` on PR #518
**Metrics:** **916 production lines** (2026-09-24; 911 at detection, of 1,147 total then) · budget 500 · **1.8× over**
**Thresholds breached:** length 916 > 500; `dial_and_bridge` **12 parameters** > 5
**Restructure:** `extract_module --to_file` — seam not yet designed
**Status:** Open — regressed 2026-09-24 (911 → 916 in #509, `#keyring` 2/9, transport stamping; deferred with consent) — **unclaimed**

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-19 | 911 | first detection; 888 → 911 in PR #518 (`impl Drop for SandboxSessionState`) |
| 2026-09-24 | 916 | 911 on the merge-base with `origin/master` (`4e7157d2`) → 916 after #509 (`#keyring` 2/9): the `RequestTransport::Pipe` stamp on the jail's stdio endpoint, which rustfmt wraps over several lines. Grown; decomposition deferred with the developer's consent (`docs/dev/todo/2026-09-24-keyring-desktop-login-grew-thirteen-over-budget-files.md`). First test gate is `#[cfg(all(test, unix))]` at L917 — the real test module |

## What the tool found

Holds `SandboxSessionState` and its lifecycle, the spawn plan builder, `terminate_sandbox_process`,
`build_allow_read_paths` (nesting 6) and `dial_and_bridge` — which takes **12 parameters**, the
highest count in any package this PR touched.

## What would close it

`dial_and_bridge`'s parameter list is the obvious first move: an options struct, in the shape
`SandboxRunnerSpawn` and `SandboxedCodebaseParams` already use next door. That is `extract_type` —
**not available for Rust** in the restructure vocabulary, so it is a hand change under the usual
discipline, or a `rename_symbol` plus manual struct introduction.

The file-level seam is likelier to be the spawn-plan construction (`build_allow_read_paths` and its
neighbours) versus the session lifecycle, but no seam has been proven with `check --deep`.
