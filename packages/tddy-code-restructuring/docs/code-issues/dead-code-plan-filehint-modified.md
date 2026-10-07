# dead-code: FileHint.modified — written into the v2 header, read by nothing

**Location:** `packages/tddy-code-restructuring/src/plan.rs:133` — `FileHint::modified`
**Category:** dead-code
**Detected:** 2026-10-02 by the `/pr-wrap` validation of #537
**Metrics:** 1 field · 1 write site (`hint_of`, `plan/codec/file_hint.rs:8,13`) · 0 read sites outside tests
**Restructure:** no — delete the field and its serialisation, or give it a reader
**Status:** Open

## Measurement history

| Run | Read sites | Note |
|---|---|---|
| 2026-10-02 | 0 | first detection |
| 2026-10-03 | 0 | #539 (`#live-plan` 7/15) rewrites the field whenever it refreshes a plan's file hints, and still reads it nowhere: the drift report and the live-plan comparisons use `sha256` only. The field's doc comment says "never read; it is there for a person". `hint_of` moved to `plan/codec.rs` in the carve |
| 2026-10-04 | 0 | #567 (`#live-plan` 15/15) added `type_`, `expr` and `order` to `RefactorOp` in `plan.rs`, which moved the field from line 127 to 133. Re-ran the `grep` below: still one write site (`hint_of`, `plan/codec.rs:217`), no read site outside tests. Unchanged |
| 2026-10-05 | 0 | the same-crate moves added kinds to `plan.rs` and rules to `plan/codec.rs`; the field is still at `plan.rs:133`, and the write site, `hint_of`, is now at `plan/codec.rs:219` (it was `:217`). Re-ran the `grep` below: still one write site and no read site outside tests. Unchanged |
| 2026-10-06 | 0 | `#sharpen` 1/8 (`feature/sharpen/tidy-engine-files`) moved `hint_of` into `plan/codec/file_hint.rs` behind the facade at `plan/codec.rs:213`; the write site is now `file_hint.rs:8,13`, the field still at `plan.rs:133`. Re-ran the `grep` below: still one write site and no read site outside tests. Unchanged |
| 2026-10-07 | 0 | `#sharpen` 5/8 (`feature/sharpen/plan-header`) writes a header for a headerless plan through the existing `hint_of` (`headerless.rs`), adding **no second writer of `modified`**; the write site stays `file_hint.rs:8,13` and the field `plan.rs:133`. Re-ran the `grep` below: still one write site and no read site outside tests. Unchanged |
| 2026-10-07 | 0 | `#sharpen` 7/8 (`feature/sharpen/repoint-call`) adds a `callee` field to `RefactorOp` in `plan.rs`, below `FileHint`; the field stays at `plan.rs:133` and the write site stays `file_hint.rs:8,13`. Re-ran the `grep` below: still one write site and no read site outside tests. Unchanged — the PR touched the file but not this field |

## What the tool found

Reproduce, from the repo root:

```bash
grep -rn "modified" packages/tddy-code-restructuring/src --include='*.rs'
```

The v2 header is `{"v":2,"files":{path:{"sha256","modified"}}}`. `hint_of` reads the file's mtime and
writes it as RFC 3339; the drift report compares `sha256` only. The only other hits for `modified`
are the field's own definition, unit-test fixtures and an unrelated `content modified` LSP error
string. Not measured: external readers of a plan file (a plan is plain JSON, so a script could read it).

## Why it matters here

A header field that a reader of the schema reasonably takes to be consulted — "hint: hash and update
time" — is not. A plan author may believe a changed timestamp is reported, and it is not. The header
also pays an `fstat` per named file at snapshot time for a value nothing uses.

## What would close it

Either remove `modified` from `FileHint`, `hint_of`, the header codec and the docs that describe it
(`docs/item-anchors.md`, `references/plan-schema.md`), or give it a reader: the drift report could
say how long ago a drifted file was written. Ordinary work, not a `/code-restructuring` job.

## Verified by hand

2026-10-02: ran the grep above at the working tree of #537 and read every hit.
