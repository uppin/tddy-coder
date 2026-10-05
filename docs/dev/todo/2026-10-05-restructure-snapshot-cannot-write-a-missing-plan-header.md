# 2026-10-05 — `restructure snapshot` cannot write the header a plan needs, so every plan starts with a hand-computed hash

**Category:** Known limitation (engine capability)
**Source:** #carve 17/21 (PR #532) stage A

## What I ran

`restructure anchors <file> --items A,'<X as Y>',...` emits the `items` anchor (good), and I wrapped it in a
one-line `move_item` plan. `check --deep` then refused:

```
Error: the index daemon refused this run (InvalidArgument): plan is malformed: first line must be a snapshot header
```

`restructure snapshot plan.jsonl`, the command the skill names for "rewrite the header", refuses the same
file with the same text, because it only rewrites a header that is already there.

## What I did by hand

Wrote the v2 header myself: `{"v":2,"files":{"<path>":{"sha256":"sha256:<shasum -a 256 of the file>"}}}`. With
it, `check --deep` reported `no findings` and `apply` ran (1 operation, 5 files, compile gate clean).

## What would have made it unnecessary

Any of: `anchors` (or a new `restructure plan new`) emitting a header-plus-operation skeleton; or `snapshot`
creating the header when line 1 is an operation. A v2 header is only hints for item anchors, so the engine
can compute it from the plan's own anchors' `file` fields.

## Minimal reproduction

```bash
printf '%s\n' '{"op":"move_item","anchor":{...an emitted items anchor...},"name":"m","to":"c","reexport":"none"}' > p.jsonl
tddy-tools restructure snapshot p.jsonl     # Error: plan is malformed: first line must be a snapshot header
```

## Worked

`move_item` with `name` (create-and-fill) moved a struct and two trait impls out of `agent_roster.rs` in one
operation: doc comments and a FIXME preserved, callers re-pointed with `reexport: none` (two consumer files),
unused-import tidy removed the copied header. `check --deep` against the warm daemon took seconds.
