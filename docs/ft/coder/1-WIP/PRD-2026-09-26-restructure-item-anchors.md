# Restructure item anchors — an anchor is an LSP path, a line/col is only a hint - PRD

**Date**: 2026-09-26
**PRD Type**: Enhancement + Bug Fix

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md) — the plan's anchor
  vocabulary, the snapshot header (schema v2), `restructure anchors`, and the § Known limitations
  entries about hand-written line numbers going stale.
- **Related Feature**: [Warm code-intelligence daemon](../warm-code-intelligence-daemon.md) — the
  `Anchors` RPC returns the new anchor shape; `Check`/`Apply` resolve it.
- **Related surface**: `.agents/skills/code-restructuring/SKILL.md` and
  `references/plan-schema.md` — the authoring workflow ("Do not hand-write line numbers").

## Summary

A plan anchor today is either a bare symbol name, resolved to the **first** outline node that
matches anywhere in the file, or an absolute line/column range that is trusted exactly. Both break
as soon as the tree moves under the plan: the name is ambiguous, and the range points at whatever
now sits on those lines.

This PRD adds an **item anchor**: a fully-qualified path to the enclosing item
(`tddy_core::workflow::Stack::new`) plus a line/column **relative to that item**. The path is
resolved to an exact position by navigating rust-analyzer's document outline, so an edit anywhere
outside the item leaves the anchor correct. The absolute line/column is kept, but only as an
orientation hint for humans and error messages; nothing resolves through it. Each item anchor
carries a fingerprint of the item's text, so an edit **to the anchored item itself** is refused
rather than silently re-targeted. The snapshot header becomes a per-file *hint* (hash + update time)
that no longer refuses a plan on its own.

## Background

- `find_symbol` (`packages/tddy-code-restructuring/src/backends/rust.rs:2226`) walks the outline and
  returns the first node whose `name` equals the anchor's `path`. `new` in a file with two `impl`
  blocks is whichever comes first.
- `Anchor::Range` is read verbatim by `anchor_range` and `rename_symbol`. The `PositionLedger`
  corrects it inside one run only.
- On #524, one unrelated PR (#508) made six plans stale; they were re-anchored by a throwaway difflib
  script ([`2026-09-24-restructure-snapshot-cannot-rebase-a-stale-plan.md`](../../dev/todo/2026-09-24-restructure-snapshot-cannot-rebase-a-stale-plan.md)).
- The authoring command the skill mandates, `restructure anchors`, resolves nothing: the outline
  comes back empty (code issue
  [`broken-restructure-anchors-empty-outline.md`](../../../packages/tddy-code-restructuring/docs/code-issues/broken-restructure-anchors-empty-outline.md)).
  Any outline-based resolution depends on that being fixed first.

## Proposed Changes

### What's Changing

1. **`"kind":"item"` anchor.**

   ```jsonc
   {"kind":"item",
    "item":"tddy_core::workflow::Stack::new",           // authoritative: the enclosing item
    "file":"packages/tddy-core/src/workflow/stack.rs", // where to look — a hint the resolver trusts for lookup only
    "start":{"line":4,"col":9},"end":{"line":14,"col":11}, // relative: line 1 = the item's first line
    "fingerprint":"sha256:…",                           // the item's text when the anchor was written
    "hint":{"line":188,"col":9}}                        // absolute, orientation only
   ```

   - `item` is crate-rooted: the crate's name, the module path, then the item and member segments.
     A member of a trait impl that is ambiguous by name is addressed with the trait,
     `…::<Stack as Display>::fmt`.
   - `start`/`end` are relative to the item's full range (outer attributes and doc comments
     included); `line` counts from 1 at the item's first line, `col` is the column on that line. Both
     omitted means "the item itself" — the position of its name, which is what symbol operations
     (`rename_symbol`, `move_module_to_crate`, `inline_method`) act on.
   - A range must lie inside its item; one that does not is refused as malformed.
2. **`"kind":"items"` anchor** — a contiguous run of sibling items, for `extract_module`:
   `{"kind":"items","file":…,"items":["crate::m::A","crate::m::B"],"fingerprints":[…]}`, resolved to
   the span from the first item's first line to the last item's last line, trivia included.
   Non-adjacent items are refused, as `anchors --items` refuses them today.
3. **Resolution.** The Rust backend computes the module path of `file` (package name from its
   manifest; module path from the file's place under `src/`), refuses when `item`'s prefix does not
   match it, then walks the outline down the remaining segments. It refuses — never guesses — when a
   segment is absent, when a segment matches more than one node, or when the resolved item's text no
   longer hashes to `fingerprint` (**the anchored item changed**; class `FailedPrecondition`, naming
   the item). An item that has moved to another file is "not declared in `file`"; nothing searches
   elsewhere.
4. **When resolution happens.** Every item anchor is resolved **at run open**, against the tree the
   run starts on, into the original-snapshot coordinates the ledger already translates through the
   run. A plan written against an older tree therefore runs as long as its items are intact.
5. **Schema v2 header**: `{"v":2,"files":{"<path>":{"sha256":"…","modified":"<RFC 3339>"}}}`. A
   hint: a drifted hash is reported, not refused. v1 plans (`snapshot`, `range`, `symbol`) parse and
   run exactly as today, including the `snapshot mismatch` refusal.
6. **`restructure anchors` emits item anchors and works.**
   - `anchors <file> --items A,B` → an `items` anchor (the empty-outline defect fixed).
   - `anchors <file> --at L:C[-L:C]` → an `item` anchor for the innermost item enclosing that
     position, with the relative range, fingerprint and hint filled in — the way an author turns a
     line they read into an anchor that survives.
   - `restructure snapshot <plan>` writes a v2 header for a v2 plan.
7. **TypeScript**: the TypeScript backend refuses `item`/`items` anchors by name rather than
   approximating them. Rust v1 only.

### What's Staying the Same

- Every operation, its fields, and its refusal classes.
- The ledger, journal and checkpoint machinery inside a run.
- v1 plans, byte for byte.
- The plan file is still read from disk per run in this PR — holding plans in the daemon is a
  successor's.

## Impact Analysis

### Technical Impact

- `tddy-code-restructuring`: `plan.rs` (anchor kinds, v2 header, parse/validate), `backends/rust.rs`
  (item-path resolver replacing first-match `find_symbol` for item anchors; `places_of`'s outline
  defect), `runner` (run-open resolution), `restructure_args.rs`/`restructure_cli.rs` (`anchors --at`).
- `tddy-index-daemon`: `AnchorsRequest` gains the `--at` position; `AnchorsResponse` returns the
  anchor JSON rather than a bare range.
- `tddy-tools`: `index_client.rs` carries the new `anchors` argument to the daemon.
- `backends/rust.rs` is already an oversized-file code issue; the resolver goes into its own module
  under `backends/rust/`.

### User Impact

Plan authors stop hand-writing line numbers that rot. A plan written yesterday runs today unless
somebody edited the very function it cuts — and then it says which one.

## Acceptance Criteria

- [ ] An `item` anchor into `Stack::new` resolves to the same exact range after lines are inserted
      above `Stack` in the same file.
- [ ] An `item` anchor whose `hint` is wrong by any amount resolves exactly; the hint is never read.
- [ ] Two `impl` blocks both defining `new`: `…::A::new` and `…::B::new` resolve to their own
      methods; a trait-impl collision is refused until `<T as Trait>::m` is used.
- [ ] An edit inside the anchored item is refused as `FailedPrecondition`, naming the item and the
      op; an edit outside it is not.
- [ ] An item absent from its `file`, or whose crate/module prefix does not match `file`, is refused;
      nothing searches another file.
- [ ] A relative range reaching outside its item is refused as malformed.
- [ ] `anchors <file> --items A,B` emits an `items` anchor on the warm and the cold path (the
      empty-outline code issue closed).
- [ ] `anchors <file> --at 188:9-198:11` emits an `item` anchor relative to the innermost enclosing
      item, and applying an `extract_method` through it produces the same edit as the equivalent
      range anchor.
- [ ] A v2 plan whose per-file hash drifted for an unrelated edit runs; a v1 plan with the same
      drift is still refused with `snapshot mismatch`.
- [ ] The TypeScript backend refuses an item anchor by name.

## References

- [Rust code restructuring](../rust-code-restructuring.md)
- [Warm code-intelligence daemon](../warm-code-intelligence-daemon.md)
- [Reusable LSP](../reusable-lsp.md)
