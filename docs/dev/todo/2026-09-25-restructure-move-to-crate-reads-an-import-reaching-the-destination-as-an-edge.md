# 2026-09-25 — `move_module_to_crate` reads a header import that reaches the destination as an edge back to the origin: a `super::` path, or a module the origin re-exports the destination's items through

**Category:** Future enhancement (engine defect; the move is refused, nothing is written)
**Source:** `#carve` 15/15, [#526](https://github.com/uppin/tddy-coder/pull/526), T3 port-move
pilot, second run: plan
`docs/dev/1-WIP/2026-09-23-carve-lifecycle-wiring-plans/14e-clone-deletion-to-session-agents.jsonl`

## What happened

Plans 14a and 14b moved the free predicate `peer_has_no_such_session` from lifecycle's
`connection_service/split_start.rs` into `tddy_session_agents::peer_session_answer`. After the build
correction, lifecycle keeps reaching it through the parent's re-export chain, unchanged:

```rust
// tddy-session-lifecycle/src/connection_service/split_start.rs (after 14b and its build fix)
pub(crate) use tddy_session_agents::peer_session_answer::*;
// tddy-session-lifecycle/src/connection_service.rs (unchanged)
pub use split_start::*;
```

Plans 14c and 14d extracted `delete_clone_on_peer`'s self-free tail and gave it a module of its own.
`extract_module` restored the import as a relative path, which is correct and compiles:

```rust
// tddy-session-lifecycle/src/connection_service/svc_provision_agent_clone/clone_deletion.rs
use super::super::peer_has_no_such_session;   // -> tddy_session_agents::peer_session_answer
```

Plan 14e then moved `clone_deletion` into `tddy-session-agents`, the crate that defines what that
path reaches. Both the gate and `apply` refused it:

```text
0: `packages/tddy-session-lifecycle/src/connection_service/svc_provision_agent_clone/clone_deletion.rs`, which operation 0 moves to `packages/tddy-session-agents`, names `tddy_session_lifecycle::super::peer_has_no_such_session`, which stays behind in `packages/tddy-session-lifecycle`. So the destination would depend on the crate it left, while the facade it leaves there names the destination: a cycle `apply` refuses. …
0: plan is malformed: `packages/tddy-session-lifecycle/src/connection_service/svc_provision_agent_clone/clone_deletion.rs` still names `tddy-session-lifecycle` (tddy_session_lifecycle::super::peer_has_no_such_session), so the destination would depend on the crate it left while that crate goes on naming the module it lost — move what those paths reach, or move the module's own dependencies with it
```

`tddy_session_lifecycle::super::…` is not a path. The header reader puts the origin's crate name in
front of a relative path instead of resolving `super::super::` from the moved file's own module
(`connection_service::svc_provision_agent_clone::clone_deletion`, so `super::super` is
`connection_service`). Even resolved, it would have to follow `connection_service`'s
`pub use split_start::*` and `split_start`'s `pub(crate) use tddy_session_agents::…::*` to see that
the item is the destination's own. The plan-schema note says the check reads the header "with the
same re-export resolution `apply` uses, so a path the origin only re-exports from another crate is
not an edge". This case is exactly that, and it is refused.

## What would close it

- Resolve `self::`, `super::` (any depth) and `crate::` against the moved file's module path before
  the stays-behind test, never by string prefixing.
- Then run the resolved path through the same re-export resolution as a `crate::` path, including
  glob re-exports and chains of them, so an item that lives in the destination (or in any third crate)
  is not an edge back to the origin.
- Rewrite such a header line to the defining crate's path during the move (here
  `use crate::peer_session_answer::peer_has_no_such_session;`, since the destination defines it), as
  the move already does for a `crate::` line.

A red test: a two-crate fixture where `a::outer::inner` imports `super::helper`, `a::outer` holds
`pub use b::helper_mod::*;`, and `inner` is moved to `b`. That should be clean under `check --deep`,
and after `apply` the moved file should read `use crate::helper_mod::helper;`.

## A second shape of the same gap: a module import whose items the destination defines

Plan 19b moved `agent_roster.rs`'s record helpers into `tddy_session_agents::agent_records`, and
lifecycle's `agent_roster` module now re-exports them
(`pub use tddy_session_agents::agent_records::*;`). Plan 20a extracted `remote_roster_record_for`'s
self-free tail, and 20b gave it a module. Its import pass wrote the module import the origin file
used:

```rust
// tddy-session-lifecycle/src/connection_service/svc_turn_end_reporter/remote_roster_record.rs
use crate::connection_service::agent_roster;
// …
true => agent_roster::qualified_agent_id(&row.name, owning_daemon)   // -> tddy_session_agents::agent_records
```

Plan 20c (`move_module_to_crate` → `tddy-session-agents`) was refused the same two ways:

```text
0: `…/svc_turn_end_reporter/remote_roster_record.rs`, which operation 0 moves to `packages/tddy-session-agents`, names `tddy_session_lifecycle::connection_service::agent_roster`, which stays behind in `packages/tddy-session-lifecycle`. So the destination would depend on the crate it left, while the facade it leaves there names the destination: a cycle `apply` refuses. …
0: plan is malformed: `…/remote_roster_record.rs` still names `tddy-session-lifecycle` (tddy_session_lifecycle::connection_service::agent_roster), so the destination would depend on the crate it left while that crate goes on naming the module it lost — move what those paths reach, or move the module's own dependencies with it
```

Here the header does name a module that stays. But every path the file reaches **through** that module
lands in the destination. The move could re-point each `agent_roster::<item>` to the crate that
defines `<item>` (here `crate::agent_records::qualified_agent_id`) and drop the module import. That is
what it already does for a `crate::<module>::<item>` header line. The plan cannot express it: a
cluster would have to take `agent_roster.rs`, which holds `impl RemoteSnapshotSource for
DaemonSessionHost` and so can never leave lifecycle.

`resolve_specialized_agent_defs` (`agent_roster::started_agent_id`) and `roster_record_for`
(`agent_roster::roster_record`) import `agent_roster` the same way, so their tails wait on this too.

## What the pilot did

Stopped each method and rolled it back (14a–14e, and 20a–20c), with nothing done by hand.
`tear_down_agent_clone` calls the same predicate as `delete_clone_on_peer`, so it waits on this too.
