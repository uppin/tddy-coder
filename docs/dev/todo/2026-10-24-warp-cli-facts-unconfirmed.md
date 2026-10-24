# 2026-10-24 — Warp's argv and exit banner are unconfirmed

**Category:** Deferred from `#warp-agent` 1/2 (#717)
**Source:** the `warp-cli` session changeset (#717) § Prerequisites (ℹ PRECEDENT)

The `warp-cli` session type was built without a real `warp` binary: the development Mac's Warp.app
bundles only `oz`. Its facts come from Warp's published reference and are pinned by tests as
written, not as observed (`TODO(warp-cli-facts)` in `packages/tddy-agent-backend/src/cli_harness_warp.rs`):

| Fact | Where | Assumed |
|---|---|---|
| Start | `warp_argv` | `warp`, with no model or prompt flag |
| Resume | `warp_argv` | `warp --resume <TOKEN>` |
| Exit banner | `resume_token_from_output` | a line holding `warp --resume <TOKEN>`, TOKEN the next whitespace-delimited word |
| Project context | `context_globs_for_agent("warp")` in `packages/tddy-agent-backend/src/backend/context_globs.rs` | `AGENTS.md`, `.agents/**`, `WARP.md` |

The banner is the riskiest: a different wording records no token, and every resume silently starts
a fresh conversation.

**Why deferred:** nothing to check them against.

**What would close it:** run `warp --help`, start and quit a conversation on a machine with the Warp
CLI installed, compare the captured exit output with `warp_cli_acceptance.rs`'s banner, correct
whatever differs, and update the [Warp CLI session](../../ft/daemon/warp-cli-session.md) feature doc.
