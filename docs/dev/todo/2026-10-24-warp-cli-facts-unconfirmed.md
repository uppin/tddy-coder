# 2026-10-24 — Warp's exit-banner wording is unconfirmed

**Category:** Deferred from `#warp-agent` 1/2 (#717)
**Source:** the `warp-cli` session changeset (#717) § Prerequisites (ℹ PRECEDENT)
**Status:** Open — partially fixed (argv and resume flag verified 2026-10-24)

The `warp-cli` session type was built without a real `warp` binary: the development Mac's Warp.app
bundles only `oz`. Its facts come from Warp's published reference and are pinned by tests as
written, not as observed (`TODO(warp-cli-facts)` in `packages/tddy-agent-backend/src/cli_harness_warp.rs`).

The real **Warp Agent CLI** (`brew install --cask warp-agent-cli`, `warp-tui-stable`
v0.2026.10.21.08.25.stable_02) was installed on the development Mac on 2026-10-24, and the facts
were checked against it and Warp's docs:

| Fact | Where | Status |
|---|---|---|
| Start | `warp_argv` | **Verified** — bare `warp` starts the interactive TUI; `--help` offers no model or prompt flag |
| Resume | `warp_argv` | **Verified** — `--resume <RESUME>` is a real flag ("Resume an Oz/Warp conversation by server token") |
| Exit banner | `resume_token_from_output` | Behavior confirmed by Warp's docs (on exit with a non-empty conversation it prints the `--resume` command); the exact wording is unobserved, and the reader greps for a `warp --resume <TOKEN>` word window |
| Project context | `context_globs_for_agent("warp")` in `packages/tddy-agent-backend/src/backend/context_globs.rs` | Unverified — whether a real `warp` reads `AGENTS.md`, `.agents/**`, `WARP.md` has not been observed |

The banner wording remains the risk: a different wording records no token, and every resume silently
starts a fresh conversation.

**What would close it:** start and quit a conversation on the installed CLI, compare the captured
exit output with `warp_cli_acceptance.rs`'s banner, correct whatever differs, and update the
[Warp CLI session](../../ft/daemon/warp-cli-session.md) feature doc.
