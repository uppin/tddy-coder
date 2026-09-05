# 2026-08-30 — cross-host planned-PR visibility

**Type:** Bug Fix + Feature

`LinkStackNode` (peer-routed before auth, like `ResolveStackBase`) closes `TODO(cross-host-pr-stack)`: the forward node→branch link was written on the *spawning* daemon's sessions tree, so a child started under an orchestrator on another host left the node branchless forever and `base_ref_for_spawn` refused every descendant. One seam, `link_spawned_branch_without_failing_the_spawn`, serves both claude-cli paths and cursor-cli (which received the node id and dropped it) and logs rather than failing a spawn whose worktree already exists. `spawned_branch_of_session` reads `Changeset.branch` back so the node records the branch that exists, not the requested name a collision suffix may have changed. A claude-cli session's LiveKit bridge now publishes the `session` block — it previously published none — re-sent every 30s so one failed `set_metadata` cannot cost the session its association.
