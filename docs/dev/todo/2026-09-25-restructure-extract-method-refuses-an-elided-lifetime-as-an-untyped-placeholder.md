# 2026-09-25 — `extract_method` refuses any signature holding an elided lifetime `'_`, as if it were an untyped `_`

**Category:** Future enhancement (engine defect; a correct extraction is refused, and the remedy it
prints is wrong)
**Source:** `#carve` 15/15, [#526](https://github.com/uppin/tddy-coder/pull/526), port-move pilot (T3),
step 2 (`DaemonSessionHost::agent_clone_for`). The plan was not committed, because the step was
rolled back.

## What happened

The port pattern lends a receiver the host's fields as a borrowed view,
`tddy_session_agents::AgentRosterState<'a>`, and the extracted body reads `state.<field>`:

```rust
    pub(crate) fn agent_clone_for(&self, session_id: &str, agent_id: &str)
        -> Result<AgentClone, Status>
    {
        let session_dir = self.session_dir_for(session_id)?;
        let state = self.agent_roster_state();      // AgentRosterState<'_>
        let record = state                           // range starts here
            .session_agent_rosters
            .entry(session_id, &session_dir, agent_id)?
            .ok_or_else(|| Status::not_found(…))?;
        state.session_agent_clones
            .get(session_id, &record.daemon_instance_id)
            .ok_or_else(|| Status::failed_precondition(…))  // range ends here
    }
```

`check --deep` (cold) refused the `extract_method` over the tail:

```text
0: rust-analyzer's answer was unusable: rust-analyzer wrote `fn agent_clone_for(session_id: &str, agent_id: &str, session_dir: PathBuf, state: tddy_session_agents::AgentRosterState<'_>) -> Result<tddy_session_agents::session_agent_clone::AgentClone, Status> {` — it produced the extraction before it could infer the types the signature needs, and `_` is not legal there (E0121). The crate graph was most likely still loading; retrying the operation against a warm server resolves it.
```

Every type in that signature is inferred and correct. `AgentRosterState<'_>` in parameter position is
legal Rust (an elided lifetime), and the extraction would have compiled.

## Why

`refuse_inferred_placeholder` (`backends/rust.rs`) tests the declaration line with
`carries_placeholder_type`, which splits on every non-identifier character and looks for a `_`
token. `'` is not an identifier character, so `'_` yields the token `_`. The refusal is then
reported as `server_defect`, which says to retry against a warm server; no retry can help.

## What would fix it

Skip a `_` that follows a `'` (it is a lifetime), and more generally only count a `_` in a type
position: after `:` or `->`, or as a generic argument (`<_>`, `(_, _)`), not after `'`. Add the case
to the tests beside the existing `fun_name`/`var_name` ones:

```rust
assert!(!carries_placeholder_type("fn f(state: AgentRosterState<'_>) -> Result<(), Status> {"));
assert!(!carries_placeholder_type("fn f(s: &'_ str) {"));
assert!(carries_placeholder_type("fn f(s: _) {"));
assert!(carries_placeholder_type("fn f() -> (_, _) {"));
```

## Why it matters for the port moves

Every state struct that borrows the host's fields carries a lifetime, so **every** `extract_method`
over a body that reads the state is refused. The owned alternative (cloning each `Arc` and the whole
`DaemonConfig` on every call) or keeping the state as a second copy inside the host exists only to
dodge this check, so it was not taken.
