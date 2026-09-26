# 2026-09-25 — `restructure` has no operation that makes a method read its fields through a borrowed state value

**Category:** Future enhancement (missing capability; nothing breaks, the edit is done by hand)
**Source:** `#carve` 15/15, [#526](https://github.com/uppin/tddy-coder/pull/526), T3 port-move
pilot, second run: plan `22787218:docs/dev/1-WIP/2026-09-23-carve-lifecycle-wiring-plans/09a-agent-clone-for-extract.jsonl`
and every T3 method after it that reads a host field in the range it moves

## What is missing

A method of a type that cannot leave its crate (`impl DaemonSessionHost` in
`tddy-session-lifecycle`, `E0116` anywhere else) can only have its body moved if the body stops
naming `self`. rust-analyzer's "Extract into function" writes a **free** function only when the range
holds no `self`, and a `&self` method in the same `impl` otherwise. So the body has to read what it
needs from some other value first.

The receiving crate defines that value, a borrowed view of the host's fields
(`tddy_session_agents::AgentRosterState<'a>`), and the host lends it for one call
(`DaemonSessionHost::agent_roster_state(&self)`). Both are new wiring and are hand-written by design.
What is **not** wiring is the edit inside the body: every `self.<field>` in the range becomes
`state.<field>`. No operation does that today, so it is done by hand, token for token, and counted
as a gray-zone edit:

```rust
// before
pub(crate) fn agent_clone_for(&self, session_id: &str, agent_id: &str) -> Result<AgentClone, Status> {
    let session_dir = self.session_dir_for(session_id)?;
    let record = self
        .session_agent_rosters
        .entry(session_id, &session_dir, agent_id)?
        .ok_or_else(|| Status::not_found(/* … */))?;
    self.session_agent_clones
        .get(session_id, &record.daemon_instance_id)
        .ok_or_else(|| Status::failed_precondition(/* … */))
}

// after the hand edit: one inserted builder call, and two `self` → `state` substitutions
pub(crate) fn agent_clone_for(&self, session_id: &str, agent_id: &str) -> Result<AgentClone, Status> {
    let session_dir = self.session_dir_for(session_id)?;
    let state = self.agent_roster_state();
    let record = state
        .session_agent_rosters
        .entry(session_id, &session_dir, agent_id)?
        .ok_or_else(|| Status::not_found(/* … */))?;
    state.session_agent_clones
        .get(session_id, &record.daemon_instance_id)
        .ok_or_else(|| Status::failed_precondition(/* … */))
}
```

From there the engine does the rest (`extract_method` over `let record …` to the tail, then
`extract_module` with `to_file`, then `move_module_to_crate`).

`extract_variable` does not close this. It hoists **one** occurrence of an expression into a `let`
placed immediately before the statement holding it, so a field read in the middle of the body leaves
a `self` in the middle of the body, and the range after the head still holds it.

## What would close it

An operation that rebinds the field reads of a range to a value the plan names, verified by the
engine rather than by text replacement:

```jsonl
{"op":"read_fields_through","anchor":{"kind":"range","file":"…/svc_provision_agent_clone.rs","start":{"line":375,"col":9},"end":{"line":392,"col":15}},"binding":"state","builder":"agent_roster_state","fields":["session_agent_rosters","session_agent_clones"]}
```

What it would do:
- insert `let state = self.agent_roster_state();` before the range;
- rewrite each `self.<field>` in the range whose field is listed, and only those, to `state.<field>`,
  using rust-analyzer's references of the field (not a text search), so a `self.<field>` in a string,
  a comment or a macro body is seen for what it is;
- refuse a `self.<field>` in the range whose field is **not** listed, or a `self.<method>(…)`, naming
  the line, since either one leaves the range unmovable anyway;
- refuse when the builder's return type has no field of that name, or when its type differs from the
  host's by more than one auto-ref (`&'a Arc<T>` for a host `Arc<T>` is fine; a clone is not).

The compile gate catches the one case a lexical check cannot: a borrow of `state` alive across an
`.await` in a future that must be `Send`.

A borrowed state field is already a reference, so a token-for-token substitution of `&self.<field>`
leaves `&state.<field>`, a `&&T` the compiler dereferences again. That compiles, and clippy fails it as
`needless_borrow` (`start_hosted_agent_clone`, plan 22: `local_instance_id_for_config(&state.config)`
and `Arc::clone(&state.hosted_agent_clones)`). The operation should drop the `&` when the builder's
field is a reference, and keep it when it is a `Copy` value (`roster_keepalive_interval`).

## Where it was needed

The second T3 run counts every hand substitution per method in the changeset's "Port-move pilot (T3),
second run" section. Each one is exactly the edit above.
