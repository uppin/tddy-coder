# 2026-09-25 — `extract_variable` is refused whenever rust-analyzer names the new binding itself

**Category:** Future enhancement (engine defect; the operation is unusable on field reads)
**Source:** `#carve` 15/15, [#526](https://github.com/uppin/tddy-coder/pull/526), port-move pilot (T3).
The plan was not committed; it was one `extract_variable` over `self.session_agent_clones` in
`DaemonSessionHost::refuse_unready_clone`
(`packages/tddy-session-lifecycle/src/connection_service/svc_start_hosted_agent_clone.rs:145:21–146:34`,
`"name":"session_agent_clones"`).

## What happened

`check --deep` refused it after the assist ran:

```text
indexing (+0ms): assist: ExtractVariable in this file
indexing (+0ms): waiting for type inference at the anchor
indexing (+5ms): waiting for assist `extract into variable`
0: rust-analyzer's answer was unusable: rust-analyzer did not produce a `let var_name` to name
```

`Placeholder { keyword: "let", name: "var_name" }` (`backends/rust.rs`, `assist_for`) assumes the
assist always writes `let var_name = …;`. The rust-analyzer the dev shell ships (2026-03-30) names
the binding from the expression instead (`suggest_name::for_variable`: a field read suggests the
field's name), so there is no `let var_name` to find and rename. **Inferred, not observed:** the
engine discards the assist's text on this refusal, so the name it chose was not seen.

## Why it matters

This is the operation that would make a port move engine-only. A body that reads host fields can
only be extracted into a **free** function if no `self` is in the range. `extract_variable` over
each `self.<field>` would hoist the read into the host method as a local, and the following
`extract_method` would take it as a parameter:

```rust
// wanted, all engine-made: the host method keeps the reads, the logic becomes a free function
pub(crate) fn agent_clone_for(&self, session_id: &str, agent_id: &str) -> Result<AgentClone, Status> {
    let session_dir = self.session_dir_for(session_id)?;
    let session_agent_rosters = &self.session_agent_rosters;     // extract_variable
    let session_agent_clones = &self.session_agent_clones;       // extract_variable
    agent_clone_for(session_agent_rosters, session_agent_clones, session_id, agent_id, session_dir)
}
```

Without it, a field read in the middle of a body forces a hand substitution
(`self.session_agent_clones` → `state.session_agent_clones`) before anything can move.

## What would fix it

Find the binding the assist actually introduced rather than a fixed placeholder: diff the `let`
patterns before and after the assist inside the enclosing block (exactly one is new), and rename
that one, or keep rust-analyzer's name when it already equals the plan's `name`. The same check
belongs in `check --deep`. A plan whose `name` matches the suggestion needs no rename at all.

Two limits to document with it, both rust-analyzer's: each operation replaces **one** occurrence
(a field read three times needs three operations, or a range covering the first read only), and
the assist decides between `&self.x` and `self.x` from the autoref it sees.
