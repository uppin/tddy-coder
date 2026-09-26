# 2026-09-25 — `extract_method` accepts a `return` in a range that ends with a unit `if`, and rust-analyzer rewrites it into `ControlFlow`

**Category:** Future enhancement (engine defect; the tree does not compile after the apply, and the
code that does compile is not what the plan asked for)
**Source:** `#carve` 15/15, [#526](https://github.com/uppin/tddy-coder/pull/526), T3 port-move
pilot, second run: plan
`22787218:docs/dev/1-WIP/2026-09-23-carve-lifecycle-wiring-plans/16b-broadcast-roster-extract.jsonl`

## What happened

`DaemonSessionHost::broadcast_roster` returns `()`. Its body is a `let … else { return; }` and then
an `if let` with no `else`, the last thing in the function:

```rust
// before (after plan 16a hoisted `self.session_rooms`)
pub(crate) async fn broadcast_roster(&self, session_id: &str, roster: &SessionAgentRoster) {
    let session_rooms = &self.session_rooms;
    let Some(publisher) = session_rooms.agents_publisher(session_id) else {
        return;
    };
    if let Err(e) = publisher.publish(&roster.encode_to_vec()).await {
        log::warn!("could not broadcast session {session_id}'s roster on {}: {e}", /* … */);
    }
}
```

Plan 16b's range ran from the `let … else` to the closing brace of the `if let`. `check --deep`
reported `no findings`, and the dry run resolved it. That is the case the plan schema allows: "a range
that runs to the end of the function, ending with its tail expression", where "rust-analyzer keeps
the `return` verbatim". It did not keep it. It rewrote the `return` into a `ControlFlow` it matches at
the call, which is what the schema says it does for a range "ending with a statement":

```rust
// after the apply
pub(crate) async fn broadcast_roster(&self, session_id: &str, roster: &SessionAgentRoster) {
    let session_rooms = &self.session_rooms;
    if let ControlFlow::Break(_) = broadcast_roster(session_id, roster, session_rooms).await {
        return;
    }
}

async fn broadcast_roster(session_id: &str, roster: &SessionAgentRoster, session_rooms: &Arc<tddy_daemon_livekit::SessionRoomRegistry>) -> ControlFlow<()> {
    let Some(publisher) = session_rooms.agents_publisher(session_id) else {
        return ControlFlow::Break(());
    };
    if let Err(e) = publisher.publish(&roster.encode_to_vec()).await { /* … */ }
    ControlFlow::Continue(())
}
```

```text
svc_provision_agent_clone.rs:298:16: error[E0433]: failed to resolve: use of undeclared type `ControlFlow`
svc_provision_agent_clone.rs:379:140: error[E0425]: cannot find type `ControlFlow` in this scope
svc_provision_agent_clone.rs:381:16: error[E0433]: failed to resolve: use of undeclared type `ControlFlow`
svc_provision_agent_clone.rs:390:5: error[E0433]: failed to resolve: use of undeclared type `ControlFlow`
```

A `use std::ops::ControlFlow;` would make it compile, and the behaviour is the same. But the moved
function's body is no longer the method's body: it has a `ControlFlow` protocol that the original
never had, and the host is left with an `if let … { return; }` as its last statement, which does
nothing. The pilot rolled it back rather than keep that. It then extracted only the `if let` (plan
16c), which holds no `return` and moved cleanly.

## Why the check let it through

A unit-typed block-like expression at the end of a block (`if …`, `if let …`, `match …`, `loop`)
parses as the tail, so the lexical check reads the range as ending with the tail expression.
rust-analyzer's extract-function evidently treats a `()` tail the way it treats a statement: it
has no value to hand back, so it chooses the `ControlFlow` rewrite.

## What would close it

- In the tail-range exception, require the tail expression's type (the enclosing function's return
  type) to be something other than `()`. Refuse a `()` tail as `this seam cannot be cut here:`, naming
  the `return` lines, with the advice to start the range after the early exit, as 16c did.
- Or keep accepting it, but have the compile gate's failure name the `ControlFlow` rewrite explicitly,
  since the missing import is the only error and it hides the real change.

A red test: a `fn f(x: Option<u32>) { let Some(v) = x else { return; }; if v > 1 { println!("{v}"); } }`
fixture, extracting from the `let` to the end. Today `check --deep` is clean and the apply fails with
`E0433 ControlFlow`. It should be refused before any edit.
