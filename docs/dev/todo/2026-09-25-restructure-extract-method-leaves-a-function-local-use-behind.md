# 2026-09-25 — `extract_method` leaves a function-local `use` behind, so the extracted function names a type nothing imports

**Category:** Future enhancement (engine defect; the tree does not compile after the apply)
**Source:** `#carve` 15/15, [#526](https://github.com/uppin/tddy-coder/pull/526), port-move pilot (T3),
plan `docs/dev/1-WIP/2026-09-23-carve-lifecycle-wiring-plans/07a-refuse-unready-clone-extract.jsonl`,
op 0 (`extract_method` over the `self`-free tail of `DaemonSessionHost::refuse_unready_clone`)

## What happened

The method imports the enum it matches on with a `use` **inside its own body**, and the range
extracted starts after it:

```rust
// packages/tddy-session-lifecycle/src/connection_service/svc_start_hosted_agent_clone.rs, before
    pub(crate) fn refuse_unready_clone(&self, session_id: &str, record: &SessionAgentRecord)
        -> Result<(), Status>
    {
        use tddy_service::proto::session_agents_svc::AgentCloneState;   // stays: outside the range
        let clone = self.session_agent_clones.get(session_id, &record.daemon_instance_id);
        let (state, error) = match clone {                               // range starts here
            Some(clone) => (clone.state, clone.error),
            None => (AgentCloneState::Unspecified, String::new()),
        };
        match state { AgentCloneState::Ready | AgentCloneState::Local => Ok(()), … }   // range ends
    }
```

rust-analyzer's "Extract into function" writes the new function at module level (no `self` in the
range, so it is freestanding), where the function-local `use` is not in scope:

```rust
// as the engine left it
        use tddy_service::proto::session_agents_svc::AgentCloneState;   // now unused
        let clone = self.session_agent_clones.get(session_id, &record.daemon_instance_id);
        refuse_unready_clone(session_id, record, clone)
    }
}

fn refuse_unready_clone(session_id: &str, record: &SessionAgentRecord, clone: Option<AgentClone>)
    -> Result<(), Status>
{
    let (state, error) = match clone {
        Some(clone) => (clone.state, clone.error),
        None => (AgentCloneState::Unspecified, String::new()),          // E0433
    …
```

```text
svc_start_hosted_agent_clone.rs:409:18: error[E0433]: failed to resolve: use of undeclared type `AgentCloneState`
(6 × E0433)
error: could not compile `tddy-session-lifecycle` (lib) due to 6 previous errors
```

The compile gate caught it, so the run was reported as failed rather than as applied.

## Why

The import pass that restores what a cut stranded runs for `extract_module`, whose items leave the
scope of the **file's** `use` declarations. An `extract_method` whose function lands outside the
enclosing function also leaves the scope of that function's **own** items (`use`, `const`, nested
`fn`), and nothing restores those.

## What was fixed by hand

The `use` line moved from the method body to the file header (one line removed, one added). The
later `extract_module` (`07b`) then carried it into the new module by itself, and the header copy
was dropped as unused.

## What would fix it

After an `extract_method` whose new function is not nested in the one it came from, run the same
import pass over the new function: for each name left unresolved that a function-local `use` of
the origin binds, add that `use` to the new function's body (keeping it local, as written), and
drop it from the origin if nothing there still names it. A plain `check` can predict this
statically: a function-local `use` binding a name that occurs inside the range.
