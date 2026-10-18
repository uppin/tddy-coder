# 2026-10-18 — Code pane and code-index leftovers from the warm-on-demand change

**Category:** Deferred from `code-index-warm-on-demand`
**Source:** `code-index-warm-on-demand` changeset — Refactoring Needed (`/analyze-clean-code`,
`/validate-tests`, red/green notes)

Small items that change no behaviour, left open when the change wrapped. None blocks other work.

## Web (`packages/tddy-web/src/components/session/`)

- **CC19 — one error-message helper.** `err instanceof Error ? err.message : String(err)` is still
  written out in `streamReopening.ts` (`streamEndOf`) beside `navigationLookup.ts`'s `errorMessage`, and
  across older files of the same directory. One exported `messageOf(err)` would do.
- **CC23 (web half) — one margin constant.** `CodeNavigationOverlays.tsx` clamps the hover card with a
  `1rem` margin in `positionBeside` and caps its width at `calc(100% - 2rem)`; tie both to one named
  constant so they cannot drift.
- **CC26 — naming.** `WorktreeCodePane.tsx`: `const code = useCodeSession(..)` → `codeSession`.
- `codeNavigationApi.ts` holds only `CodePosition`, `CodeLocationRef` and `toLocationRef`;
  `codeLocations.ts` would name it.
- `ProbeStore` remembers probe answers until a file is opened; an index that goes back to loading after
  an edit does not clear them, so an underline can be briefly stale.
- `renderSimpleMarkdown`'s headings and fences are pinned only through `CodeHoverCard.cy.tsx`; no unit
  test.
- `WorktreeCodePaneCodeSession.cy.tsx` keeps ~4 behaviours per test in its long loading → progress →
  ready timelines (W12) — deliberate, recorded here in case it is split later.

## Rust tests

- `packages/tddy-index-daemon/tests/root_progress_acceptance.rs` copies `unary_at`, `a_message_carrying`
  and the `subject` workspace fixture from `code_index_service_acceptance.rs` (and `live_plans_acceptance.rs`
  has its own copies); a shared `tests/support/` module would remove them.
- `packages/tddy-daemon/tests/code_navigation_acceptance.rs` and `plan_dialog_acceptance.rs` still carry
  their own stand-in host and `CodeIndexService` fake; fold them onto `tests/support/mod.rs`.
- `cypress/support/rpc/codeSessionBackend.ts` records bidi frames itself because
  `tddy-connectrpc-testkit` records unary requests only; move it into the testkit
  (`backend.framesTo(method)`) when a second bidi spec needs it.

## Rust production

- `tddy-index-daemon`: `WatchRoot` / `Warm` copy `ActivePhase.percentage` (`u64` in the chatter) into the
  proto's `uint32` with `as` — LSP percentages are 0–100 by spec, so a checked conversion would only
  document that. `RootSignals`' failure record is pruned only when a root is served again (bounded by the
  distinct roots asked).
- `tddy-index-daemon/src/plan_flush.rs` `flush_eventually` (CC9, moved verbatim at pr-wrap) is nesting 5;
  extract `report_outcomes` and `flush_unlocked`.

## Why deferred

The change's own clean-code findings that affected readability of new code (CC1–CC7, CC11–CC18,
CC20–CC22, CC24, CC25) were fixed before wrap; these were left as cosmetic or pre-existing, at the
developer's call, to keep the PR's diff on the feature.
