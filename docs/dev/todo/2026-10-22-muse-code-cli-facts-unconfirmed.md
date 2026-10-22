# 2026-10-22 — Muse Code's argv, key variable and project tree are unconfirmed

**Category:** Deferred from `#provider-route` 6/9 (#667)
**Source:** `docs/dev/1-WIP/2026-10-18-provider-route-muse-cli.md` § Decisions & Trade-offs

The `muse-cli` session type was built without a real `muse` binary (on the development Mac `muse`
resolves only to the Superset wrapper, with no real binary behind it). Four facts come from secondary
sources and are pinned by tests as written, not as observed:

| Fact | Where | Assumed |
|---|---|---|
| Key variable for non-interactive auth | `MUSE_ROUTE_KEY_ENV` in `packages/tddy-agent-backend/src/cli_harness_muse.rs` | `META_API_KEY` |
| Model flag | `muse_argv` (same file) | `--model <id>` |
| Resume flag | `muse_argv` | `--continue` (the worktree's last session) |
| Project configuration tree | `context_globs_for_agent("muse")` in `packages/tddy-agent-backend/src/backend/mod.rs` | `.muse/**` |

**Why deferred:** nothing to check them against; the stack could not wait for a Muse Code install.

**What would close it:** run `muse --help` (and the resume/auth docs) on a machine with Muse Code,
correct whichever of the four differs, and update `muse_cli_acceptance.rs` and the
[Muse CLI session](../../ft/daemon/muse-cli-session.md) feature doc to match.
