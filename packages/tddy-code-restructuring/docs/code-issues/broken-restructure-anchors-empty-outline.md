# `restructure anchors` resolves no item, on either the warm or the cold path

**Category:** broken
**Command:** `tddy-tools restructure anchors <file> --items …`
**Measured:** `#carve` 5/11, against master tip `d5a157cc`
**Claimed by:** [#537](https://github.com/uppin/tddy-coder/pull/537) — `#live-plan 1/7`
**Status:** Open — partially fixed in #537 (2026-10-02)
**Lands after:** nothing — the stack's root
**Remainder owned by:** unowned — needs a repo-scale cold and warm run of the command above; #537 does not finish it

## What happens

Every item in every file is refused, including items that are unambiguously defined at module level
in a file the run never touched:

```
$ tddy-tools restructure anchors packages/tddy-core/src/workflow/ids.rs --items GoalId
restructure: running against the warm index daemon at …/tddy-index-1414353149.sock
Error: the index daemon refused this run (InvalidArgument): plan is malformed:
  `GoalId` is not an item `packages/tddy-core/src/workflow/ids.rs` defines at module level
```

`workflow/ids.rs` opens with `pub struct GoalId(String);`. Also refused: `Changeset`, `Stack`,
`read_changeset` in `changeset.rs`. Relative and absolute `<FILE>` behave the same.

The cold path does not get that far:

```
$ tddy-tools restructure anchors packages/tddy-core/src/workflow/ids.rs --items GoalId
   indexing (+0ms): acquiring shared rust-analyzer client (first run may take minutes)
Error: rust-analyzer LSP
Caused by: lsp server exited
```

## Diagnosis

`places_of` (`src/backends/rust.rs:2335`) matches `--items` against the document outline and names
back anything absent. The daemon log shows the refusals arriving **3–89 ms** after the request on an
already-warm workspace, so nothing is waiting on rust-analyzer: the outline comes back empty and
every name is therefore "not defined". The cold path exits before producing one at all.

```
18:47:31 anchors arrived for `…/green-…-10`, which is already warm
18:47:31 refused as InvalidArgument (+21ms): `read_changeset` is not an item … defines at module level
```

The cold index itself does complete — the first call took 18 minutes and left the daemon warm — so
the failure is in obtaining or reading the outline, not in indexing.

## Why it matters

The skill makes this command mandatory: *"`restructure anchors <file.rs> --items A,B,C`. **Do not
hand-write line numbers.**"* With it broken, every `extract_module` and `extract_module_to_file`
plan has to be written against hand-counted ranges — the exact failure mode the instruction exists
to prevent, since a hand-counted range clips helpers the moved code needs.

On `#carve` 5/11 this removed the mechanical path for Phase A entirely; the four-way split of
`changeset.rs` was done by hand in `7a7f043d`.

## Progress in #537 (2026-10-02)

- **Reproduced and fixed, in the item-anchor resolver:** a file whose outline is genuinely empty used to
  wait forever once the resolver waited out an empty answer; `settled_outline` now believes an empty
  outline only when the outline has items, the index is loaded, or the server was observed quiescent,
  and refuses through the existing `IndexingIncomplete` path otherwise. Pinned by an acceptance test on a
  comments-only file (it ran 180 s and failed against the old predicate).
- **Not shown fixed:** the warm-path refusal above (every name "not defined" 3–89 ms after the request).
  The fixtures answer `documentSymbol` immediately, so no test fails on the old `places_of` path; the
  cause (an empty outline while rust-analyzer is still loading) is inferred, not reproduced.
- **Not addressed:** the cold path's `lsp server exited`.

## What would close it

Run the command in the header against a built binary at repo scale, once on a cold daemon and once on
a warm one, and see `GoalId` resolve. Delete this record on that measurement, not before. A server that
never sends `experimental/serverStatus` still waits on an empty outline until its caller cancels.
