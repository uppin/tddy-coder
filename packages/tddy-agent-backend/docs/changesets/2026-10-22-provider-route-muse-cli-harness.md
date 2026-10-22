# 2026-10-22 — Muse Code CLI harness and its route arm

**Type:** Feature

`#provider-route` 6/9 ([#667](https://github.com/uppin/tddy-coder/pull/667)). `cli_harness_muse.rs`:
the `MUSE_CLI` descriptor (a `const`, registered fourth in `cli_harnesses()`), `muse_cli_models()`
(curated: `muse-spark-1.3` default, `muse-wonder-1.0`), `muse_argv` (`[--model m] [prompt]`;
`--continue` on a resume) and `MUSE_ROUTE_KEY_ENV` (`META_API_KEY`). `resolve_route(MuseCli, _)` is a
real arm: `meta` only, the key as the one secret variable, no env, files or args. The `muse` row of
`context_globs_for_agent` (`AGENTS.md`, `.agents/**`, `.muse/**`). The argv, key variable and `.muse/`
tree are unconfirmed against a real `muse` (`TODO(muse-cli-facts)`, backlog entry
`2026-10-22-muse-code-cli-facts-unconfirmed`). Code issue re-measured: `oversized-file-backend-mod`
880 → 881 production lines (one arm; stack-shared, split deferred). See
[architecture](../architecture.md#cli-harnesses-cli_harness).
