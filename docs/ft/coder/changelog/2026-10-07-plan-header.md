# 2026-10-07 — `restructure snapshot` writes the header a plan is missing

Feature: [Rust code restructuring](../rust-code-restructuring.md).

`restructure anchors <file> --items …` emits an anchor, and wrapping it in a one-line plan is the
obvious next step — but `check --deep` refuses such a plan with `plan is malformed: first line must
be a snapshot header`, and `snapshot`, the command the skill names for "rewrite the header", refused
the same file with the same text, because it only rewrote a header that was already there. The author
wrote the v2 header by hand with `shasum -a 256`. `restructure snapshot <plan>` now writes line 1 for
a plan whose first line is an operation: a header computed from the files the operations' anchors
name, serialised through the same `hint_of` / `hash_file` path a headed plan's header goes through, so
a second `snapshot` of the plan it wrote changes nothing.

- **Which header.** A plan whose every anchor is an `item`, or a run of `items`, gets the **v2**
  header — hints that never refuse a run. A `range` or `symbol` anchor names coordinates that depend
  on the whole file, so it gets the **v1** header, whose snapshot refuses on drift: the header whose
  promise matches the anchors.
- **Validated before anything is written.** An anchor whose `file` is empty, leaves the workspace, or
  is not a regular file is refused, and the plan file is left byte-identical. Every other refusal an
  operation would earn from `check` — a code-bearing field, an unknown field, a split group — surfaces
  from `snapshot` too.
- **Every other reader names the remedy.** `check`, `apply`, `status`, `load` and `Plan::parse` still
  refuse a headerless plan, and the refusal now ends `… run \`restructure snapshot <plan>\` to write
  one` when the first line is an operation. A first line that is neither header nor operation keeps
  the plain refusal — naming a command that cannot help would be worse than silence.
- **It stays in process.** `snapshot` of a headerless plan never starts a language server and never
  dials the index daemon, pinned by tests with and without `TDDY_INDEX_SOCKET` set.
