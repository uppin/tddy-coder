# 2026-09-24 — `ListSessions`' entry mapping is still inline: `session_entry_from_listing` was not started

**Category:** Future enhancement
**Source:** `#carve` 14/15, [#524](https://github.com/uppin/tddy-coder/pull/524), change history
[`2026-09-23-carve-lifecycle-destructure`](../changesets/2026-09-23-carve-lifecycle-destructure.md)
("Consent list" item 7, the `session_coordinate_handlers` State B row)

## Why deferred

**Not started, and not refused.** The destructure split `session_coordinate_handlers.rs` (818 → 414
production lines) with plan `08`, which moved resume and signal/delete into
`session_coordinate_handlers/svc_resume_session.rs` and `svc_signal_delete_session.rs`. The second
half of that row, lifting the list mapping out as a free function, was never planned into a
restructure plan. It was not needed to take any file under 500, and the run ended at the developer's
"file the TODOs and move on" (2026-09-24).

## What is left

`list_sessions_at_session_coordinate` (`connection_service/session_coordinate_handlers.rs:38`,
about 128 lines) builds each `SessionEntry` inline, inside the `spawn_blocking_with_timeout`
closure. The mapping reads nothing from `self`: only the listed session, its directory, the local
daemon id and the two `Arc`s the closure already captured.

```rust
// before: session_coordinate_handlers.rs:65–155 (trimmed)
                for s in sessions {
                    let session_dir = sessions_base_blocking.join(SESSIONS_SUBDIR).join(&s.session_id);
                    let mut entry = SessionEntry {
                        session_id: s.session_id,
                        created_at: s.created_at,
                        …                                  // every field, many with a comment
                        last_activity: None,
                    };
                    let mut conn_entry: ConnSessionEntry = super::family_proto_bridge::wire_same(&entry)…?;
                    if let Err(e) = session_list_enrichment::apply_session_list_status_to_proto(&session_dir, &mut conn_entry) { … }
                    entry = super::family_proto_bridge::wire_same(&conn_entry)…?;
                    if matches!(entry.session_type.as_str(), "claude-cli" | "cursor-cli") { … }   // agent-status inference
                    out.push(entry);
                }

// after: the loop body is one call
                for s in sessions {
                    out.push(session_entry_from_listing(
                        s,
                        &sessions_base_blocking,
                        &local_daemon_id,
                        &session_agent_inference,
                        &agent_activity_hub,
                    )?);
                }

fn session_entry_from_listing(
    s: session_reader::SessionEntry,
    sessions_base: &Path,
    local_daemon_id: &str,
    inference: &SessionAgentInferenceStore,
    hub: &AgentActivityHub,
) -> anyhow::Result<SessionEntry> { … }
```

The discovery proposed a file for it, `session_list_entries.rs`, beside the other two handler
files. The parameter types in the sketch are the ones the closure holds today
(`session_reader::SessionEntry`, `tddy_session_agents`' `SessionAgentInferenceStore`,
`tddy_daemon_kernel::AgentActivityHub`), possibly behind the `Arc`s it clones; the engine writes
what rust-analyzer infers.

## What would close it

A restructure plan with an `extract_method` over the loop body (it holds no `return`, so E4 does not
apply; the `?` stays inside the new function's `anyhow::Result`), then an `extract_module` to
`session_list_entries.rs` under `session_coordinate_handlers/`. Proven with `check --deep` on a
freshly started index daemon, then applied, then the destructure's baseline (61 targets,
622 / 22 / 1). No consent is needed beyond the usual rules: it is an engine move.
