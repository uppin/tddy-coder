# `restructure anchors` resolves no item, on either the warm or the cold path

**Category:** broken
**Command:** `tddy-tools restructure anchors <file> --items …`
**Measured:** `#carve` 5/11, against master tip `d5a157cc`
**Claimed by:** [#537](https://github.com/uppin/tddy-coder/pull/537) — `#live-plan 1/7`
**Lands after:** nothing — the stack's root

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
