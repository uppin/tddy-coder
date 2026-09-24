# 2026-09-24 — `restructure snapshot` cannot rebase a stale plan's anchors

**Category:** Future enhancement
**Source:** `#carve` 13/15 `/green`, [#527](https://github.com/uppin/tddy-coder/pull/527), changeset
[`2026-09-23-restructure-engine-fixes`](../1-WIP/2026-09-23-restructure-engine-fixes.md)

A plan pins each file it touches by `sha256`, and its anchors are line/column ranges into those
files. When an unrelated change lands on the same files, the plan is refused with
`snapshot mismatch`, and that refusal is correct. `restructure snapshot` then rewrites only the
header. It does not touch the anchors, which now point at the wrong lines.

On #524, #508 (`#keyring` 1/9) edited `connection_service.rs`, `split_session.rs`,
`svc_spawn_split_agent.rs` and `svc_resolve_tddy_tools_path.rs` after the plans were written. Six
plans went stale (`01`, `04`, `04a`, `05`, `05a`, `07`). They were re-anchored by a throwaway script.
The script diffs each file between the plan's original ref and the working tree with `difflib`,
moves every anchor endpoint that sits on an unchanged line, and reports each endpoint that falls
inside a changed hunk instead of guessing. One endpoint (`05`, `join_split_livekit_room`) landed in
a hunk and was fixed by hand. The rest mapped cleanly and applied.

For example, #508 added three lines above the extract-methods in `svc_spawn_split_agent.rs`:

```jsonl
// the plan as written, against 4e260d7f
{"v":1,"snapshot":{"…/svc_spawn_split_agent.rs":"sha256:f554583c…"}}
{"op":"extract_method","anchor":{"kind":"range","file":"…/svc_spawn_split_agent.rs","start":{"line":185,"col":9},"end":{"line":195,"col":11}},"name":"split_agent_withdrawals"}

// what `restructure snapshot` writes: the header follows the tree, the anchor still points 3 lines too high
{"v":1,"snapshot":{"…/svc_spawn_split_agent.rs":"sha256:2760ce10…"}}
{"op":"extract_method","anchor":{…"start":{"line":185,"col":9},"end":{"line":195,"col":11}},"name":"split_agent_withdrawals"}

// what it needs to write
{"op":"extract_method","anchor":{…"start":{"line":188,"col":9},"end":{"line":198,"col":11}},"name":"split_agent_withdrawals"}
```

The heart of the throwaway script:

```python
old = git_show(ref, path).split("\n"); new = read(path).split("\n")
line_map = {}
for tag, i1, i2, j1, j2 in difflib.SequenceMatcher(None, old, new, autojunk=False).get_opcodes():
    if tag == "equal":
        for k in range(i2 - i1):
            line_map[i1 + k + 1] = j1 + k + 1
# an endpoint not in line_map sits inside a changed hunk: report it, never guess
```

## Why this was deferred

Out of #527's scope, which covers the engine defects the destructure's plans hit. The workaround was
cheap once written, but it is knowledge nobody else has.

## What would close it

```bash
tddy-tools restructure snapshot --rebase 4e260d7f plan.jsonl
# op 3 `join_split_livekit_room`: start line 126 is inside a changed hunk (lines 126–128 → 129–131); re-anchor by hand
```

`restructure snapshot --rebase <ref>`:

- Map every range anchor from the tree at `<ref>` to the working tree through the line diff.
- Refuse, naming the op and endpoint, when an endpoint falls inside a changed hunk.
- Only then rewrite the header.

A symbol anchor needs no mapping. The plan's own header could record the ref it was written
against, so the option would need no argument.
