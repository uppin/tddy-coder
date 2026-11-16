# oversized-file: useCreateSessionPaneState.tsx

**Location:** `packages/tddy-session-create-web/src/useCreateSessionPaneState.tsx`
**Category:** oversized-file
**Detected:** 2026-11-16 by `/pr-wrap` step 3.5 file-length gate on PR #919 (`#remote-shell` 5/7)
**Metrics:** **503 production lines** (whole file; TypeScript has no `#[cfg(test)]` cut) — 495 at the base of #919, grown 8 by that PR
**Thresholds breached:** 503 > 500 production-line budget
**Restructure:** hand split (TypeScript — the `code-restructuring` engine is Rust-only)
**Status:** Open — unclaimed

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-16 | 495 → 503 | #919 (`#remote-shell` 5/7): the Host field holds a derived host's id, resolved to its parent daemon and target for the start request. **Not split**: successor PR #920 (`feature/remote-shell/remote-hooks`) also modifies this file, and a split here would turn its diff into a conflict (`/pr-wrap` step 3.5, stack-branch rule). Split after the `#remote-shell` stack lands |

## What the tool found

The hook assembles the whole create-session pane state: session type, agent selection, placement,
host selection, project options and the start request. The derived-host resolution is the eight
lines that took it over the budget; the size is the pane's, accumulated over many PRs.

## What would close it

Extract the host/placement resolution (the chosen host's id, its parent daemon and its target)
into a `useCreateSessionHostPlacement` hook beside the component, under the hand-split discipline:
green baseline first (the package's Cypress component specs), mechanical moves only, same green
after. Do it on a follow-up branch after the `#remote-shell` stack lands, when no stack PR still
edits this file.

## Verified by hand

2026-11-16 (#919 wrap): counted with the step 3.5 gate loop — 495 at
`origin/feature/remote-shell/coder-shell`, 503 at HEAD; confirmed #920 modifies the same file in
its own diff (`git diff 0999b9cc7..origin/feature/remote-shell/remote-hooks --name-only`).
