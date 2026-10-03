# 2026-10-03 — code navigation grew four files already over the 500-production-line budget

**Category:** Deferred decomposition
**Source:** `#live-plan` 8/15, [#574](https://github.com/uppin/tddy-coder/pull/574), `/pr-wrap` file-length gate.

## What is left

This PR added wiring to four non-test source files that were already over budget, and deferred their
decomposition with the developer's consent (the additions are the registration of one service, 3-14
lines each):

| File | Production lines |
|---|---|
| `packages/tddy-daemon/src/runtime.rs` | 1,620 → 1,631 — see its `oversized-file-runtime` code-issue record |
| `packages/tddy-service/build.rs` | 695 → 709 — see its `oversized-file-build` code-issue record |
| `packages/tddy-web/src/components/sessions/SessionMainPane.tsx` | 657 → 666 |
| `packages/tddy-web/src/components/sessions/SessionsDrawerScreen.tsx` | 966 → 969 |

## Why it was left

Splitting a composition root, a build script and two shell components is its own refactor, and other
`#live-plan` nodes touch the same files (the rename fallout would conflict through the stack).

## What would close it

Each file at or under 500 production lines, measured with the `/pr-wrap` step 3.5 gate. The two Rust
files already have code-issue records; the two web files have none yet — add records when they are
next touched.
