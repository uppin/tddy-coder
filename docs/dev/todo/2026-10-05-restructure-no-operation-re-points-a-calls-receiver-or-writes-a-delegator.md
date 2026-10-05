# 2026-10-05 — `restructure` cannot re-point a call's receiver or leave a forwarding delegator, so 28 call sites and 7 wrappers are hand-written

**Category:** Future enhancement (missing capability; nothing breaks, the edits are done by hand)
**Source:** #carve 17/21 (PR #532) stage B1

## What I ran

After a method moves to another type, its callers and the host calls inside its body change by a few tokens,
none of which a call-site operation expresses (`add_call_arg`, `remove_call_arg`, `change_call_arg` and
`reorder_call_args` edit an argument list; nothing edits the receiver or the callee path). I did not try an
operation for them; I read `plan-schema.md`'s table and the probe in
[`2026-10-05-restructure-no-operation-retargets-an-impl-to-another-type`](2026-10-05-restructure-no-operation-retargets-an-impl-to-another-type.md).

## What I did by hand

- **Callee/receiver in a moved body** (14 sites): `self.common_room_slot(x)` -> `self.peer_routing.common_room_slot(x)`
  (also `eligible_instance_ids`), `self.session_dir_for(id)` -> `session_dir_lookup::session_dir_for(&self.tddy_data_dir, id)`,
  `self.mint_first_admission_token(..)` -> the free function with `&self.config, &self.session_admissions`,
  `self.split_forward_deadline()` -> the free function over `&self.config`, `self.agent_roster_state()` -> `self.state()`, `self.local_exec_tools()` -> `self.host.local_exec_tools()`.
  Each host method is a one-line forward to the thing now named directly.
- **Callers outside the moved files** (14 sites): `x.m(..)` -> `x.agent_roster().m(..)`.
- **Delegators** (7): `pub async fn m(&self, ..) -> R { self.agent_roster().m(..).await }` for the methods a
  consumer crate or an in-`src` test still calls on the host; signature copied from the moved method.
- **Dead wrappers** (2): `mint_first_admission_token` and `session_dir_for` on the host lost their last caller
  once the bodies read the free functions; only the compiler's `dead_code` warning found them.

## What would have made it unnecessary

Two capabilities, either alone removes most of it:

1. `repoint_call` — an item anchor with a relative range over one call (as the call-site operations have) and a
   `callee` text parsed as one path/method chain: rewrites the receiver or callee, keeping the arguments.
   A bulk form over every reference to a method (the server already knows them) would turn 28 edits into one.
2. `leave_delegator` as a variant of `retarget_impl` (see the sibling todo): after moving a member, keep a
   forwarding method with the old signature on the old type, in a named file, for the callers a `reexport`-like
   field names (`outside`, `tests`, `named`). Then report any wrapper with no caller left.

## Minimal reproduction

```rust
struct Host { n: u32 }
impl Host { fn n(&self) -> u32 { self.n } fn twice(&self) -> u32 { self.n() * 2 } }
// move `twice` to `impl Handle` whose field `n` is read directly: `self.n()` must become `self.n`;
// keep `Host::twice` forwarding to `self.handle().twice()`.
```
