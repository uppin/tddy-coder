# 2026-10-05 — `restructure` has no operation that moves methods from one type's `impl` to another's, so every impl header is hand-edited

**Category:** Future enhancement (missing capability; nothing breaks, the edit is done by hand)
**Source:** #carve 17/21 (PR #532) stage B1

## What I ran

Stage B1 moves 22 methods from `impl DaemonSessionHost` to `impl AgentRoster` (the owned handle D1 chose, whose
fields carry the host's names so a body is unchanged). The only edit per file is the `impl` header and the
`use` of the type, yet no operation does it. The closest, probed against the warm daemon:

```jsonl
{"v":2,"files":{"<file>":{"sha256":"sha256:<digest>"}}}
{"op":"change_param_type","anchor":{"kind":"item","item":"tddy_session_lifecycle::connection_service::svc_turn_end_reporter::DaemonSessionHost::roster_record_for_agent_id","file":"<file>","fingerprint":"sha256:<emitted>"},"name":"self","type":"AgentRoster"}
```

`restructure check <plan> --deep`:

```
0: this seam cannot be cut here: `self` is not a parameter of the function the anchor names
Error: 1 finding(s) — see above. Nothing was written.
```

`plan-schema.md` also says "a member of an `impl` cannot move alone" for `move_item`, and the `<Type>` anchor
moves only a *whole* impl block, keeping its self type. The signature operations that replaced the old
`restructure-has-no-signature-operations` todo (#567) change parameters and return types, never the receiver.

## What I did by hand

- `svc_provision_agent_clone.rs`, `svc_turn_end_reporter.rs`: `impl DaemonSessionHost {` -> `impl AgentRoster {`
  and `use super::DaemonSessionHost;` -> `use super::agent_host_callbacks::AgentRoster;`.
- `svc_resolve_listed_worktree.rs`: the file mixed one T1 method with six T3 ones in a single
  `impl DaemonSessionHost`. I closed the block after the T1 method, opened `impl AgentRoster {` before the
  first T3 member (`}` + blank + header, moving nothing), and kept both `use` lines.

## What would have made it unnecessary

An operation `retarget_impl` (item anchor `<Type>` or a run of `impl` members) with `to_type`: rewrites the
header's self type, splits the block at the anchored members when they are a proper subset (same type, new
block), and re-points the paths the old type was named by in that block (`DaemonSessionHost::f(..)` -> the
new type). It must refuse when the moved members use a field the new type lacks (the compile gate says so
after, but `check --deep` could name it before) and keep every comment, as `move_item` does.

## Minimal reproduction

```rust
struct A { n: u32 }
struct B { n: u32 }
impl A { fn get(&self) -> u32 { self.n } }   // want: impl B { fn get(&self) -> u32 { self.n } }
```

No operation turns this into `impl B`. `change_param_type` on `self` is refused as above.

## Related

[`2026-09-25-restructure-has-no-operation-to-read-a-methods-fields-through-a-state-parameter`](2026-09-25-restructure-has-no-operation-to-read-a-methods-fields-through-a-state-parameter.md)
(the same family: a method's receiver is the thing the engine cannot change).
