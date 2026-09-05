# 2026-08-30 — cross-host planned-PR visibility

**Type:** Bug Fix + Feature

new `session_participant_metadata` module owns the `session` participant block's shape (13 fields, every key always emitted) now that two crates publish it and the LiveKit merge is shallow, so a partial object would erase the other publisher's keys; a serialize failure returns an empty string rather than `{"session": null}`, which would erase them too. `session_chain`'s `pr_stack_node_for_spawn` / `resolve_chain_base_ref` take a `stack_node_id` and prefer it over the branch-derived lookup — a supplied-but-unknown id resolves to nothing rather than falling back, since falling back reinstates the guess the id removes.
