# Item anchors

How a plan names the code an operation acts on without naming the lines it sat on when the plan was
written. The product contract is
[Rust code restructuring](../../../docs/ft/coder/rust-code-restructuring.md#item-anchors); this page is how
the crate delivers it. The authoring syntax is the skill's `references/plan-schema.md`.

## The two anchor kinds

| Anchor | Names | Resolves to |
|---|---|---|
| `item` | one item by crate-rooted path, plus a range **relative to that item** | the item's range with the relative range added to its first line |
| `items` | a contiguous run of sibling items, for `extract_module` | the span from the first item's first line to the last item's last line, trivia included |

`item` carries `item`, `file`, optional `start`/`end` (line 1 is the item's first line, attributes and
doc comments included; both omitted means the item itself, at its name), `fingerprint` and `hint`.
The `hint` is an absolute line and column kept for readers and error messages. Nothing resolves
through it. `symbol` and `range` anchors remain the vocabulary of v1 plans and parse as they always did.

## Where the code lives

| Module | Responsibility |
|---|---|
| `plan.rs` | `Anchor::Item` / `Anchor::Items`, `ItemPath` (`crate::m::T::f`, `crate::m::<T as Trait>::f`), `Fingerprint`, the v1 and v2 headers, validation (a relative range must lie inside its item; `items` must be adjacent) |
| `item_anchor.rs` | What every language shares: `module_path_of` (package name from the manifest with `-` read as `_`; `lib.rs`/`main.rs` are the crate root, `a.rs` and `a/mod.rs` are module `a`), relative to absolute ranges, the fingerprint check, `resolve_item_anchors`, `item_anchor_at`, `items_anchor`, `span_of`, and the `ItemResolver` trait |
| `backends/rust/item_path.rs` | The Rust resolver: walks rust-analyzer's `documentSymbol` outline segment by segment (modules, types, impl blocks keyed by self type and trait) |
| `registry.rs` | Routes an item anchor to the backend that owns the file. A file no backend claims is `NoBackend`; a backend with no item resolver (`LanguageBackend::item_resolver` returns `None`) is `UnsupportedOp { op: "item anchors" }` |
| `runner/entry_points.rs` | `open_run_resolving_anchors`, `item_anchors` (the `anchors` command), the static-check finding for item-anchored operations |

## Resolution

Every item anchor is resolved **once, at run open**, against the tree the run starts on, and lowered
into the `range` and `symbol` anchors every operation already understands. From there the
`PositionLedger` carries the coordinates through the run exactly as it carries a hand-written range.

The resolver refuses — it never guesses and never searches another file — when:

| Condition | Error |
|---|---|
| the item's crate or module prefix does not match `file` | `MalformedPlan` |
| a segment is absent from the file | `MalformedPlan`, "not declared in `file`" |
| a segment matches more than one outline node (two inherent impls, a trait-member collision) | `MalformedPlan`, asking for `<T as Trait>::m` |
| a relative range lies outside its item | `MalformedPlan` |
| the item's text no longer hashes to `fingerprint` | `ItemChanged { item, file }` — `FailedPrecondition` |
| the run continues a journal that already applied operations | `ItemAnchorsOnContinuedRun { applied }` — `FailedPrecondition` |
| no backend, or no item resolver | `NoBackend`, `UnsupportedOp` |

`ItemChanged` names the item and its file. It does not name the operation, because it is raised while
the plan is resolved as a whole, before any operation runs.

**The fingerprint** is the SHA-256 of the item's whole lines, indentation included, not of the exact
server range. An edit outside the item moves it and leaves the anchor correct. An edit inside it
changes the fingerprint and is refused, because it could have moved or removed what the relative range
names. Counting relative lines from the item's first line, attributes and doc comments included, keeps
the two consistent: any edit that could shift a relative line also changes the fingerprint.

**Resolution precedes the baseline compile gate.** `open_run_resolving_anchors` is the one order both
apply loops use (the command line's and the daemon's): item anchors resolve, then the baseline
`cargo check`, then `.restructure/` is written. An item refusal therefore surfaces before minutes of
compiling and leaves no run state behind.

**A continued run refuses item anchors.** The ledger translates coordinates read from the original
tree; a run whose journal already holds completed operations no longer has that tree, and coordinates
read from the edited one would be translated through those edits a second time. The refusal is its
own error rather than a malformed plan, since nothing is wrong with the plan. The `#live-plan`
stack's plan store (#538 onward) is what keeps item anchors current across runs.

**Static `check` cannot examine item anchors.** A static check has no server to resolve with, so it
reports each item-anchored operation as a finding saying to run `check --deep`; a plan of item
anchors never passes a static check green. `check --deep` resolves them first.

## The outline wait

`settled_outline` reads `documentSymbol` and believes an **empty** answer only when the server has
been observed quiescent or the index is marked loaded; otherwise it waits and polls, and a degraded
index is refused rather than vouched for. A file that genuinely defines nothing (comments only) is
therefore answered, where an unconditional wait on an empty outline never returned. The wait ends
when the caller's cancellation token fires, as every wait in this crate does, and then names where
the index got to (`IndexingIncomplete`). The daemon's `Anchors` handler carries a token that fires
when the request is dropped.

A server that never sends `experimental/serverStatus` still waits on an empty outline until its
caller stops waiting. The wait has no deadline of its own.

## Plan header, schema v2

```jsonl
{"v":2,"files":{"<path>":{"sha256":"sha256:<digest>","modified":"<RFC 3339>"}}}
```

A per-file **hint**. A file whose hash drifted, or which is gone, is reported on the progress line and
the plan runs: an item anchor does not depend on the rest of the file. A v1 header
(`{"v":1,"snapshot":{…}}`) is read as written, including the `snapshot mismatch` refusal. `restructure
snapshot` rewrites the header of whichever version the plan has. `modified` is written and not read.

## The `anchors` command

| Form | Emits |
|---|---|
| `anchors <file> --items A,B` | an `items` anchor for adjacent module items |
| `anchors <file> --at L:C[-L:C]` | an `item` anchor for the innermost item enclosing the position, with the relative range, fingerprint and hint filled in |

In process the command returns the anchor value; through the daemon, `AnchorsResponse.anchor_json`
carries the anchor's JSON and `range` its absolute span. See
[the daemon's contract](../../tddy-index-daemon/docs/code-index-service.md).

## Testing

`tests/item_anchor_acceptance.rs` and `tests/anchors_command_acceptance.rs` drive the resolver against
small fixture crates (`tests/harness/`) through a real rust-analyzer, because what the outline says is
the point of the feature. Every refusal above has a test naming it. Unit tests cover `ItemPath`
parsing, the header codecs, relative-to-absolute ranges, module paths and the outline walk over recorded
`documentSymbol` JSON.
