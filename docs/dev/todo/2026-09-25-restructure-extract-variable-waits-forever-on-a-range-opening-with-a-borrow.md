# 2026-09-25 — `extract_variable` waits for ever on a range that opens with `&`

**Category:** Future enhancement (engine defect; the run never answers, and a warm daemon stays wedged
after its client is killed)
**Source:** `#carve` 15/15, [#526](https://github.com/uppin/tddy-coder/pull/526), T3 port-move
pilot, second run: the first form of plan
`docs/dev/1-WIP/2026-09-23-carve-lifecycle-wiring-plans/21a-agent-def-for-spawn-hoist.jsonl`

## What happened

The first form of 21a selected the whole borrow, `&self.model_registry`:

```jsonl
{"op":"extract_variable","anchor":{"kind":"range","file":"packages/tddy-session-lifecycle/src/connection_service/svc_resolve_listed_worktree.rs","start":{"line":186,"col":33},"end":{"line":186,"col":53}},"name":"model_registry"}
```

```rust
if let Some(registry) = &self.model_registry {
//                      ^ col 33          ^ col 53
```

On a warm `tddy-index-daemon` with `crate index ready`, `check --deep` printed
`waiting for type inference at the anchor` and then nothing more for over ten minutes, with
rust-analyzer at 0% CPU. The same plan's `apply --dry-run` did the same. The plan was then changed to
start at column 34 (`self.model_registry`), and on a restarted daemon it answered in 0.3 s after
warm-up. Every earlier `extract_variable` in this run (12a, 15a, 16a, 17a) started on `self` and
answered in seconds.

The wait is probably a hover at the range's start. On `&` the server has no hover to give, and
neither the `inactive-code` nor the `unlinked-file` diagnostic that ends the other two known waits
applies to it.

Killing the client did not free the daemon. The request kept waiting server-side, and the next
`check` against the same workspace queued behind it (`check arrived … which is already warm`, then
silence). The daemon had to be restarted, twice in this run.

## What would close it

- Probe type inference at the first token of the range that can carry a hover: skip leading
  punctuation (`&`, `&mut`, `*`, `!`, `-`, `(`), or hover on the range's innermost enclosing
  expression rather than on its first character.
- Bound the wait: once the index is ready, a hover that stays `null` for as long as a ready index
  takes to answer anything should end as `rust-analyzer's answer was unusable:`, naming the position.
- Cancel a request's server-side work when its client disconnects, so one hung run does not wedge the
  workspace for the next.

A red test: an `extract_variable` range over `&self.v` in `fn f(&self) -> bool { let r = &self.v; r.is_some() }`.
It should resolve or refuse within the index's ready time, not hang.

## Also seen in this run, cause not established

After the rollback of plans 20a–20c, which deleted a module file an apply had created, the daemon
logged `told the server … of 2 file change(s)`. rust-analyzer then re-indexed the workspace and
stopped reporting progress at `working: tddy_daemon_rpc (lib)`, before `crate index ready`, at 0% CPU
for over ten minutes. That run also used the `&` anchor, but it never reached the anchor wait, so it
may be a separate stall. The earlier rollbacks in the same run (plans 14a–14e and 16a–16b) were each
followed by a check that answered within seconds.
