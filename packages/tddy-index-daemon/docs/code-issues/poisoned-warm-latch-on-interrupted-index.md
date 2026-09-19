# poisoned-warm-latch: a killed warm leaves the workspace flagged warm but unloaded

**Location:** `packages/tddy-index-daemon/src/warm.rs` (the latch), with
`packages/tddy-code-restructuring/src/backends/rust.rs:2175` (`ensure_indexed`) and
`./run-index-daemon --status` as the two places it becomes invisible
**Category:** defect
**Detected:** 2026-09-19 — hit three times in one session during a `/pr-wrap` restructure
**Metrics:** **52 min 51 s** per lost cold graph, measured (`answered (+52m51s)` in the daemon log) ·
**3 graphs burned in one session** ≈ 2.5 h
**Status:** Open — **unclaimed**
**Distinct from:** [`docs/dev/todo/2026-09-16-warm-ready-means-a-live-server-not-a-loaded-graph.md`](../../../../docs/dev/todo/2026-09-16-warm-ready-means-a-live-server-not-a-loaded-graph.md),
which closes the readiness **probe**. This is about a warm that is *interrupted* after the latch is
set, which that entry does not cover.

## Measurement history

| Run | Cold-graph cost | Graphs lost | Note |
|---|---|---|---|
| 2026-09-19 | 52m51s | 3 | first detection; box at load 30–48 from ~10 concurrent worktrees |

## What the tool found

Three separate mechanisms that are individually survivable and together produce something that
reads exactly like a tool bug.

### 1. A `restructure` call killed mid-warm poisons the workspace

The warm latch is set **before** the crate graph finishes loading. Kill the call after that point
and the daemon keeps serving that workspace as warm. Afterwards every
`textDocument/documentSymbol` answers empty, and because `ensure_indexed`
(`backends/rust.rs:2175`) treats an empty outline as a real answer once `self.indexed` is set,
`anchors` refuses **every** real item name in 2–7 ms:

```
`SplitAgentWiring` is not an item — …/split_session.rs defines at module level
```

Confirmed against four different files, including ones where the item was plainly visible.

### 2. `./run-index-daemon --status` cannot distinguish healthy from poisoned

It reports `1 warm workspace` in **both** states. It dials the socket, so it proves a live server —
which is exactly what the sibling TODO says `Warm.ready` was fixed to mean — but it says nothing
about whether that server's graph is queryable.

### 3. A `timeout` wrapper or a pipeline hides the failure

- A pipeline reports the exit status of its **last** command, so `timeout`'s 124 surfaces as
  **exit 0 with empty output**.
- `head`/`tail` in the pipeline buffer the answer, so a **successful** run also looks empty.

So the natural defensive invocation — `timeout 300 … | tail` — both causes the poisoning and
conceals it.

## The reliable probe

A **foreground, unpiped** `anchors` naming an item you can see at module level:

```bash
export TDDY_INDEX_SOCKET=…
tddy-tools restructure anchors <file.rs> --items <AKnownModuleLevelItem>
```

A healthy answer is a range; note it starts one line **above** the item when a doc comment is
absorbed by the trivia walk-up, which is correct:

```json
{"file":"…/split_session.rs","kind":"range","start":{"line":51,"col":1},"end":{"line":60,"col":2}}
```

A refusal means poisoned. The only fix is `./run-index-daemon --stop`, restart, and pay the cold
graph again.

## What would close it

1. **Do not set the latch until the graph is queryable** — or clear it when the warming task is
   cancelled. Cancellation is the trigger; nothing currently unwinds the flag.
2. **Make `--status` report graph state, not just liveness** — it already has to dial; have it ask
   for an outline it knows should be non-empty.
3. **Distinguish "empty outline" from "answered"** in `ensure_indexed` (`backends/rust.rs:2175`).
   An empty `documentSymbol` for a file with items is not a real answer, and treating it as one is
   what turns a loading index into a refusal that names the caller's item as the problem.

## If you are about to change this code

Never wrap a `restructure` call in `timeout` and never pipe it. The tool's waiting behaviour is the
design — a run waits until the server is ready or until you stop it — and both a timeout and a
pipeline destroy that signal.
