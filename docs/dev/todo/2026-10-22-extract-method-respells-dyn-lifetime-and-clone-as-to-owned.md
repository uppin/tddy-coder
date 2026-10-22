# 2026-10-22 — `extract_method` respells a `&mut dyn Trait` parameter as `'static`, and `.clone()` as `.to_owned()`

**Category:** Engine defect (`extract_method` signature and body rewrites)
**Found in:** `#reshape` 18/24 (`fn-sizes-rest`), plan 03 (`refreshed`, the `Items` and `Item` arms of `plan_store/refresh.rs`)
**Owner:** the `#reshape` sweep node

## 1. `&mut dyn Trait` becomes `&mut (dyn Trait + 'static)` (E0521)

The engine writes a parameter the function body reads from a `&mut dyn ItemResolver` as
`resolver: &mut (dyn ItemResolver + 'static)`. The original's elided object lifetime is not `'static`, so the compiler
rejects the cut:

```
packages/tddy-code-restructuring/src/plan_store/refresh.rs:87:13: error[E0521]: borrowed data escapes outside of
function: `resolver` escapes the function body here, argument requires that `'1` must outlive `'static`
```

The apply failed its compile gate after both ops were applied; the run was rolled back (`git checkout HEAD -- refresh.rs`,
journal removed). Plan: `.restructure/03-refreshed-…` (not kept), two `extract_method` ops, `Items` arm
(`refreshed_items`, lines 54–74 of the item) then `Item` arm (`refreshed_item`, lines 28–46). The `Item` arm's signature
was `(edited: &[&str], resolver: &mut (dyn ItemResolver + 'static), item: &ItemPath, start: &Option<Position>, end:
&Option<Position>, file: String, mut anchor: Anchor) -> Result<Anchor>`.

## 2. `.clone()` on a borrowed slice or `Vec` becomes `.to_owned()`

In the same plan, `verify --against HEAD` reports `items.clone()` and `fingerprints.clone()` (in
`Anchor::Items { items: items.clone(), .. }`) as lost, and `items.to_owned()` as gained. The new function takes
`items: &[ItemPath]` and `fingerprints: &[Fingerprint]`, and the engine rewrote the clone to `to_owned()`. It is
equivalent in type, but it is not a pair the verify tool accepts, and the developer did not accept it (2026-10-22: stop
on it). Any cut that shows it is rolled back and deferred.

## Wanted

- `extract_method` writes a `&mut dyn Trait` parameter with the lifetime the original spells, not `'static`.
- `extract_method` does not rewrite `.clone()` on a borrowed value; or `verify` accounts for that pair.
- Both are fixed in the engine, then `refreshed` (and any cut deferred under this entry) is re-run.

## 3. An outer local read or written by the moved code is not passed to the new function

Plan `sightings` (`crate_move/source_scan/sighting_walk.rs`), developer decision 2026-10-22: the brace arm was cut
alone, and the path arm (`at = last + 1; continue;`) is deferred with it.

- Path arm (lines 112–123 of the item), two ops in one plan: the apply failed its compile gate with
  `error[E0425]: cannot find value 'at'` at 112:41, and `error[E0308]: mismatched types: expected usize, found &mut usize`
  at 127:43 and 135:53. The engine left the outer cursor `at` unbound in the new function and passed `&mut at` where the
  function takes a `usize`. Rolled back; journal removed.
- Brace arm (lines 65–81), single op `brace_frame`: `check --deep` no findings, dry-run 1 of 1, apply failed its compile
  gate with `error[E0425]: cannot find value 'at'` at 123:41. The moved match guard `Some((opens, name)) if opens == at`
  reads the outer cursor, and the engine did not pass `at` as a parameter. Rolled back; journal removed.

Wanted: `extract_method` passes every outer local the moved code reads or writes as a parameter, as the `&mut` or value
it needs, and a function body that writes an outer cursor it also `continue`s past is refused, not written.
