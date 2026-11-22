# 2026-11-22 — A Coder-target resume takes ~2 minutes and shows no progress

**Category:** Future enhancement (found by `fix/coder-target-resume`, #998)
**Source:** `packages/tddy-agent-launch/src/opencode_cli_spawn/resume.rs` (`resume_cli_harness_session`), `start_steps.rs` (`place_slot_files_and_pointer`), `remote_agent_launch.rs`

A resume of a session whose agent runs on a Coder workspace took about 2 minutes on 2026-11-22
(the original start took about as long): it re-renders and re-places the hooks, settings, system
prompt and session pointer over some 25 `coder ssh` round-trips, each paying a connection set-up. It
does not redo the clone. The web UI shows nothing in that window, so an operator who clicked Resume
sees no effect — which is how a double click launched two agents before #998 refused the second.

**Why deferred:** #998 fixed the duplicate launch and logs the resume's start and end; making the
resume fast, or visible, is a separate change touching the remote launch steps another branch was
editing at the time (`investigate/coder-target-back-channel`).

**What would close it:** either batch the resume's remote steps into fewer `coder ssh` calls (skip
re-placing files whose content is unchanged since the start), or report start progress to the UI the
way a start's progress is reported, so a long resume reads as in flight; ideally both.
