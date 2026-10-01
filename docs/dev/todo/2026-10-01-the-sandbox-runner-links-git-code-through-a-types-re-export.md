# 2026-10-01 — `tddy-sandbox-runner` links `tddy-subagent-worktree` through a re-export

**Category:** Future enhancement — dependency shape
**Source:** `#agent-worktree` 1/4, changeset `2026-09-30-agent-worktree-isolated-edits`

The runner never calls `tddy-subagent-worktree` and never runs git (git runs on the host only), and
`Cargo.toml` has no such line. But `tddy-discovery` re-exports `WorktreeChange` / `FileCounts` /
`LineCounts`, and the runner reaches `tddy-discovery` (via `tddy-tool-engine` →
`tddy-worktree-service` → `tddy-daemon-kernel`, and directly), so `cargo tree -p tddy-sandbox-runner
-i tddy-subagent-worktree` succeeds: the git code is linked into the binary that runs inside every
jail. The runner also restates the directory name (`CONVERSATION_WORKTREES_DIR` in
`packages/tddy-sandbox-runner/src/conversation_root.rs`), and a test in `tddy-daemon-sandbox` pins it
equal to `tddy_subagent_worktree::SUBAGENT_WORKTREES_DIR`.

**Why deferred:** both options below change crate boundaries shared with the rest of the stack, and
the extra link is dead code, not a behaviour problem.

**What closing it takes (pick one):**
- move `WorktreeChange`, `FileCounts`, `LineCounts` into a small types crate that `tddy-discovery` and
  `tddy-subagent-worktree` both depend on, so the runner stops linking git code; or
- let the runner depend on `tddy-subagent-worktree` openly, delete the duplicated
  `CONVERSATION_WORKTREES_DIR` and its pinning test.
