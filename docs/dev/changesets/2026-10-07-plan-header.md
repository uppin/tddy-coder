# 2026-10-07 — `restructure snapshot` writes the header a plan is missing

**Type:** Feature

`restructure snapshot <plan>` writes line 1 for a plan whose first line is an operation, computing the
header from the files the operations' anchors name. A plan no longer needs a hand-computed `sha256` to
get past `check --deep`. Every other reader keeps refusing a headerless plan and now names
`restructure snapshot` as the remedy. Behaviour:
[rust-code-restructuring.md](../../ft/coder/rust-code-restructuring.md) § Plan format.

## What changed

- **The tolerant read and the header (`tddy-code-restructuring`, `plan/codec/headerless.rs`, new)** —
  `Plan::starts_with_an_operation` (a first line carrying an `op` key and none of `v`/`snapshot`/
  `files`), `Plan::parse_headerless` (every non-blank line is an operation, so every operation refusal
  surfaces here exactly as it would from `check`), `Plan::anchored_files` (the primary and `also`
  anchors' files, sorted and de-duplicated), and `Plan::header_for_anchored_files`. Version follows
  Decision O1: **v2** when every anchor of every operation is `item`/`items`, **v1** when any is
  `range`/`symbol`. Serialised through `hint_of` / `apply::hash_file` — the same path `rehashed_header`
  uses, so a second `snapshot` rewrites nothing. `anchored_file_path` validates each file (empty,
  absolute or `..`, not a regular file) before anything is written, so a refusal leaves the plan
  byte-identical.
- **`snapshot` inserts (`runner/entry_points/check_entry_points.rs`)** — `snapshot` routes a plan whose
  first line is an operation to `insert_a_header`, which splices `header + "\n"` above the first
  non-blank line, carrying leading blank lines and every operation byte for byte; `snapshot_resolving`
  routes through `plan_file_has_item_anchors`, so a headerless plan is answered in process with no
  server.
- **The refusal (`plan/codec.rs`)** — `Plan::parse`'s first-line refusal names `restructure snapshot`
  when the first line is an operation, and keeps the plain text otherwise.
- **Tests (no source change)** — `tddy-tools` `tests/restructure_cli_acceptance.rs` pins that the CLI
  writes the header and never dials a named daemon, and that `check` names the remedy; `tddy-index-daemon`
  `tests/code_index_service_acceptance.rs` pins the `Snapshot` RPC over a headerless plan with no server.

## Code issues

| Record | Measurement |
|---|---|
| `tddy-code-restructuring` `dead-code-plan-filehint-modified` | Touched, **unchanged**: the header is written through the existing `hint_of`, so this node adds **no second writer of `modified`** — one write site (`file_hint.rs:8,13`), no read site outside tests. History row appended; record kept |

## Backlog

Resolves `docs/dev/todo/2026-10-05-restructure-snapshot-cannot-write-a-missing-plan-header.md` — the
one-line plan wrapping an emitted anchor is now snapshotted instead of hand-headed. Entry deleted.

## Stack

Node 5 of 8 of the `#sharpen` stack. Depends on `tidy-engine-files` (#588, merged) for the split
`plan/codec.rs` it edits. Dependents: [#593](https://github.com/uppin/tddy-coder/pull/593)
`retarget-impl`, [#594](https://github.com/uppin/tddy-coder/pull/594) `repoint-call`,
[#595](https://github.com/uppin/tddy-coder/pull/595) `repoint-facade`.
