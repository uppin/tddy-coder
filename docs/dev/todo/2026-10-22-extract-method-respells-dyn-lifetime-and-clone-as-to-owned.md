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

## 4. `parse_op`: the guard lifts re-type the operation's borrowed fields

Plan `parse_op` (`plan/codec.rs`), nine `extract_method` ops bottom-up (type/expr syntax; missing name; `also`/`to_file`;
cross-crate and `outside`; reparent arm; move_item arm; facade; test binary; code text). `check --deep` no findings, and
dry-run resolved 9 of 9. The apply's compile gate failed with 12 errors, none in a new function's own logic:

- `365:13` and `419:13`: `error[E0614]: type 'bool' cannot be dereferenced` (the lifted `to_file`/`reexport` checks read a
  `bool` through the wrong reference).
- `371:30`, `421:30`, `446:30`, `456:30`, `464:30`: `error[E0277]: the trait bound 'str: Into<String>' is not satisfied`
  and `the size for values of type 'str' cannot be known`, where the lifted `malformed(format!(..))` or
  `malformed("..")` call takes a `&str` the engine spelled as `str`.

Rolled back (`git checkout HEAD -- codec.rs`), journal removed. Deferred under the standing rule (engine defect, clean
rollback). The fix is in the engine's signature and call-site typing for lifted guards.

## 5. `apply_held_plan`: comment carry refuses the report tail; the loop's phases need outputs the engine cannot return

Function `apply_held_plan` (`runner/entry_points/store_run.rs`), 161 lines after the F2 hand edit (`HeldLoop`, consented,
`TODO(reshape-16)`). Developer decision 2026-10-22: defer.

- Refresh-and-report tail (lines 398–404 of the item, `if !legacy { refresh_plan(..)?; }` then `report_settled(..)`):
  `check --deep` refused with "rust-analyzer dropped 3 comment(s) from the extracted function ... cannot be paired with the
  range's to put them back" (the comments at 401–403, between the two statements). An engine refusal, not a compile failure.
- The resolve phase (lines 314–335), dry-run phase, and apply-and-settle phase need several outputs (`at`, `resolved`, `id`,
  `files`) read after the range, or contain an outer `continue` (`let Settled::Ready(ready) = settled else { continue; }`),
  so no single clean seam takes the function under 60 lines.

Wanted: comment carry for an extraction whose range sits between two statements that have comments in between, and
`extract_method` returning several locals a later part of the loop reads (or a loop-phase seam the engine accepts).

## 6. `cluster_edits` and `check_plan`: one further cut each still leaves them over 60, and one shape is outside the accepted families

Developer decision 2026-10-22 ("one more cut each, then defer"). Both partial cuts are kept in history, and both functions are exempted.

- `cluster_edits` (`crate_move/cluster.rs`), 71 lines before the extra cut. The extra cut, the `Gathering` destructure (lines
  224–236 of the item) into `unpack_gathering`, passed `check --deep` and applied, but the function was still 67 lines. Its
  verify shape was `mut merged,` and `mut dev_crates_named,` lost from the struct pattern: a tuple binding cannot keep a
  struct pattern's `mut`. Not in the accepted families, so it was reverted.
- `check_plan` (`runner/entry_points/check_entry_points.rs`), 99 lines before the extra cut. The extra cut, the registry choice
  (lines 225–240) into `choose_registry`, passed `check --deep` and applied, but the function would still be about 84 lines. Its
  verify shape was `let mut registry = if options.deep {` lost to `let registry = if …` in the new function, because the new
  function returns the binding and drops `mut`. Not in the accepted families, so it was reverted.

Wanted: `extract_method` keeps `mut` on a binding it moves into a returned tuple or a returned value, or `verify` accounts
for a `mut` dropped only where the new function returns the binding.

## 7. `choose_import`: `extract_method` drops the enclosing function's lifetime parameter from the new function (E0261)

Found in `#reshape` 21/24 (`backend-fn-sizes`), plan `n21-01-choose-import` (the `by_crate` tier, lines 76–81 of the item,
name `by_crate_tier`). Rolled back cleanly under the standing rule (compile gate, `git checkout HEAD --` of the one file,
journal removed); deferred by the developer's standing rule of 2026-10-22.

The engine writes the new function with the parameter the range reads, and drops the `<'a>` the enclosing `choose_import<'a>`
declares:

```
fn by_crate_tier(offered: &[&'a str], in_scope: Vec<String>) -> Option<&str> {
packages/tddy-code-restructuring/src/backends/rust/import_text.rs:79:30: error[E0261]: use of undeclared lifetime name `'a`
```

The apply's compile gate caught it (`1 of 1 operation(s) were applied, and the tree no longer compiles`). Every tier of
`choose_import` that narrows `offered` reads `&[&'a str]`, so every such range hits the same defect; the one range that
avoids `offered` (the `modules` binding) saves three lines, not the five the function needs.

Wanted: `extract_method` declares every lifetime parameter of the enclosing function that the new function's signature
mentions (here `<'a>`), and spells the return type with it (`Option<&'a str>`, not an elided `Option<&str>`).

## 8. `request`: `extract_method` makes a read-only range a `&mut self` method, under a live borrow of a field (E0502)

Found in `#reshape` 21/24 (`backend-fn-sizes`), plan `n21-06-request-waiting` (the `Waiting::begin(..)` statement inside
the bridged branch of `request`, reading `self.wait_heartbeat`). Rolled back cleanly (HEAD restored for the one file, journal
removed); deferred by the developer's standing rule of 2026-10-22.

The engine writes `fn begin_wait(&mut self, method: &str, started: Instant) -> Waiting`, although the range only reads
`self.wait_heartbeat`. The branch holds `bridge` borrowed from `self.bridge` for the whole block, so the `&mut self` call is
refused by the borrow checker:

```
packages/tddy-code-restructuring/src/backends/rust.rs:893:31: error[E0502]: cannot borrow `*self` as mutable because it is also borrowed as immutable
```

Wanted: a range that reads `self` and writes nothing takes `&self`.

## 9. `next_import`: a tuple return of two borrowed bindings gets an elided lifetime (E0106)

Found in `#reshape` 21/24 (`backend-fn-sizes`), plan `n21-08-next-import-lost` (the `let lost = Lost { .. }; let inside = ..;`
pair, lines 126–130 of the item). Rolled back under the standing rule; deferred 2026-10-22.

The engine writes `fn lost_and_inside(unresolved: &[UnresolvedName], block: &ModuleBlock, name: &UnresolvedName) -> (Lost<'_>, bool)`.
Two reference inputs make the `'_` ambiguous:

```
packages/tddy-code-restructuring/src/backends/rust/imports.rs:294:104: error[E0106]: missing lifetime specifier
```

Wanted: the return names the lifetime of the input it borrows (`Lost<'a>` with `name: &'a UnresolvedName`), as the enclosing
`next_import` writes its own.

## 10. `survey`: the tail range is refused, and the one cut that reaches 60 needs a nine-parameter helper

Found in `#reshape` 21/24 (`backend-fn-sizes`), `detach_method/survey.rs`. Plan `n21-09-survey-tail` (the `Ok(Surveyed { .. })`
tail, lines 162–168) was refused by `check --deep`:

```
this seam cannot be cut here: rust-analyzer offers no "extract into function" assist for the given range (it offered none)
```

The other range that brings the function under 60 (the `prefix`, `caller_module` and `sites.push` statements, lines 146–157,
plan `n21-10-survey-site`) passes `check --deep`, but its helper `record_site` takes nine parameters, and
`clippy::too_many_arguments` fires above seven. Not applied. Deferred 2026-10-22.

Wanted: rust-analyzer's extract assist accepts a tail that builds a struct literal inside `Ok(..)`; and the extraction offers a
way to pass the sites' context without one parameter per local (a struct, or a range that takes fewer locals).

## 11. `assisted_edit`: the rename block rewrites `&format!(..)` and `&name` at its call (E0308)

Found in `#reshape` 21/24 (`backend-fn-sizes`), plan `n21-28-assisted-rename` (lines 1665–1675 of the item: `let named = ..`,
the residual-placeholder check, and the inferred-placeholder check). The helper took `name: &String` and
`placeholder: Placeholder`; the call site lost the `&` in front of `format!(..)` and in front of `name`:

```
packages/tddy-code-restructuring/src/backends/rust.rs:1711:17: error[E0308]: mismatched types: expected `&str`, found `String`
```

Rolled back under the standing rule; deferred 2026-10-22. The function is still 110 lines after the earlier, verified cuts
(`finish_relocation`), so it is exempted as a whole.

Wanted: the call site keeps the borrow the extracted parameter takes (`&format!(..)` for a `&str` parameter, `&name` for a
`&String` one) — a rewrite of the argument, not only of the signature.

## 12. `extract_method` drops a `mut` from a caller's binding when the mutation moves into the new function

Found in `#reshape` 21/24, plan `n21-26-assisted-finish` (`assisted_edit`, lines 1703–1722: the range started before
`facade::refuse_mangled_rewrite`, after `report.extend(..)` had mutated `report`). The new function takes `mut report`, and the
caller's `let (preserved, mut report) = ..` became `let (preserved, report) = ..`. `verify --against HEAD` reported the dropped
`mut`; a dropped `mut` is not an accepted shape, so the cut was rolled back. The range that starts after the `extend` is
accepted (commit `7f0a7a45f`), and keeps `mut` where it is mutated.

Wanted: a `mut` stays on the caller's binding when the caller still needs it, or the new function borrows the value it mutates.

## 13. `extract_method` of a `let` binding that is returned writes `let x = ..; x`, which clippy rejects (`let_and_return`)

Found in `#reshape` 21/24 (`backend-fn-sizes`), commit `b9a290293` (`offered_assist`'s target choice, plan `n21-19-offered-target`,
the `let target = if assist.at_caret { .. } else { range };` statement). The new function is written as
`fn target_range(range: Range, assist: Assist) -> Range { let target = if assist.at_caret { .. } else { range }; target }`,
which compiles and passes `verify`. `cargo clippy -p tddy-code-restructuring --all-targets -- -D warnings` fails:

```
error: returning the result of a `let` binding from a block
    --> packages/tddy-code-restructuring/src/backends/rust.rs:1177:5
```

Not fixed by hand, because a hand edit beyond a post-move build fix is a stop under the 2026-10-22 rules: the node is stopped
at this point for a decision. The fix is an expression body (`if assist.at_caret { .. } else { range }`), a three-line edit.

Wanted: when the range is a `let` whose binding is the function's returned value, `extract_method` writes the initialiser as the
new function's tail expression, and not a binding followed by its name.
