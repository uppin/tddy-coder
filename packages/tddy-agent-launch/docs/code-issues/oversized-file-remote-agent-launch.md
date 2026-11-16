# oversized-file: remote_agent_launch.rs

**Location:** `packages/tddy-agent-launch/src/remote_agent_launch.rs`
**Category:** oversized-file
**Detected:** 2026-11-16 by `/pr-wrap` step 3.5 file-length gate on PR #920 (`#remote-shell` 6/7), production lines counted to the first `#[cfg(test)]`
**Metrics:** **451 → 546 production lines** (2026-11-16) · budget 500
**Thresholds breached:** length 546 > 500
**Restructure:** `extract_module` / `move_item` into a submodule — `/code-restructuring` territory
**Status:** Open — unclaimed

## Measurement history

| Run | Production lines | Note |
|---|---:|---|
| 2026-11-16 | 451 → 546 | #920 (`#remote-shell` 6/7): the pid-based startup window (`wait_for_agent_start`, `AgentStart`, `AgentNeverStarted`) and the hook-log creation in `launch_on_target`. **Not split**: successor PR #921 (`feature/remote-shell/coder-hosts`) also modifies this file, and a split here would turn its diff into a conflict (`/pr-wrap` step 3.5, stack-branch rule). Split after the `#remote-shell` stack lands |

## What the tool found

The module holds the whole target launch: the remote command, the PTY spawn, the pid-based
startup window and the hook-log setup. The startup window is the growth that took it over the
budget; the size is the launch's, accumulated over the stack.

## What would close it

Move the startup window (`wait_for_agent_start`, `exits_during_startup`, `AgentStart`) into a
`remote_agent_launch::startup` submodule. Anchor with `tddy-tools restructure anchors`, never
by hand, prove the seam with `restructure check --deep` against a warm index
(`./run-index-daemon`), and confirm the file and the new module are each ≤ 500 production
lines. Do it on a follow-up branch after the `#remote-shell` stack lands, when no stack PR
still edits this file.

## Verified by hand

2026-11-16 (#920 wrap): counted with the step 3.5 gate loop — 451 at
`origin/feature/remote-shell/derived-hosts` (`a76247e46`), 546 at HEAD; confirmed #921 modifies
the same file in its own diff (`git diff origin/feature/remote-shell/remote-hooks..origin/feature/remote-shell/coder-hosts --name-only`).
