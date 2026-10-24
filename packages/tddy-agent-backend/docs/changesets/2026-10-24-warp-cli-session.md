# 2026-10-24 — The `warp-cli` harness descriptor and its resume-token reader

**Type:** Feature

`#warp-agent` 1/2, PR [#717](https://github.com/uppin/tddy-coder/pull/717). Cross-package entry:
[2026-10-24-warp-cli-session.md](../../../../docs/dev/changesets/2026-10-24-warp-cli-session.md). Feature: [Warp CLI session](../../../../docs/ft/daemon/warp-cli-session.md).

- `cli_harness_warp.rs`: `WARP_CLI` (fifth in `CLI_HARNESSES`; label Warp, binary `warp` /
  `TDDY_WARP_CLI`, context agent `warp`), `warp_argv` (`warp`, or `warp --resume <token>` on a resume
  with a non-blank `conversation_id`), `warp_cli_models()` (empty), and
  `resume_token_from_output(&[u8]) -> Option<String>` (escape sequences stripped, `\r` normalised, the
  last `warp --resume <TOKEN>` wins).
- `Harness::WarpCli` joins Cursor's `HarnessCannotBeRouted` arm in `route_supported` / `route_body`.
- `context_globs_for_agent` and `SHARED_CONTEXT_GLOBS` moved into `backend/context_globs.rs` (re-exported
  as `backend::context_globs_for_agent`); the `warp` row is `AGENTS.md`, `.agents/**`, `WARP.md`.
- Code issue `oversized-file-backend-mod.md`: 906 → 908 → 850 production lines; partially fixed, open.

Tests: `tests/warp_cli_acceptance.rs`; the registry-order, harness-id and context-glob acceptance suites
extended with Warp.
