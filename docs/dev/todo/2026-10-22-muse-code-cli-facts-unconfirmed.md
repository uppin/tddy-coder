# 2026-10-22 — Muse Code's argv, key variable and project tree are unconfirmed

**Category:** Deferred from `#provider-route` 6/9 (#667)
**Source:** `docs/dev/1-WIP/2026-10-18-provider-route-muse-cli.md` § Decisions & Trade-offs

The `muse-cli` session type was built without a real `muse` binary (on the development Mac `muse`
resolves only to the Superset wrapper, with no real binary behind it). Four facts come from secondary
sources and are pinned by tests as written, not as observed:

| Fact | Where | Assumed |
|---|---|---|
| Key variable for non-interactive auth | `route.key_env` in `packages/tddy-coding-agent-specs/specs/muse.yaml` | `META_API_KEY` |
| Model flag | `muse_argv` (same file) | `--model <id>` |
| Resume flag | `muse_argv` | `--continue` (the worktree's last session) |
| Project configuration tree | `context_globs_for_agent("muse")` in `packages/tddy-agent-backend/src/backend/mod.rs` | `.muse/**` |

**Why deferred:** nothing to check them against; the stack could not wait for a Muse Code install.

**What would close it:** run `muse --help` (and the resume/auth docs) on a machine with Muse Code,
correct whichever of the four differs, and update `muse_cli_acceptance.rs` and the
[Muse CLI session](../../ft/daemon/muse-cli-session.md) feature doc to match.

## Evidence from a real Muse Code 1.4.12 (2026-11-14)

Muse Code is now installed on the development Mac
(`/opt/homebrew/Caskroom/muse-code/1.4.12-R6909.1/muse-aarch64-macos`). From `muse --help`:

| Fact | Finding |
|---|---|
| `--model <id>` | **Confirmed** (`--model <MODEL>`, "Model id for non-echo providers"). |
| `--continue` | **Not in the help.** Resume is a subcommand: `muse resume --last` (most recent session in this workspace) or `muse resume <uuid\|name>`. A probe could not show whether `--continue` is silently accepted (Muse's parser ignored a bogus flag beside `-V`/`--help`), so `muse.yaml`'s resume argv is likely wrong. `muse resume <uuid>` with the Muse session id read from its log (see [2026-11-14-muse-transcript-not-read.md](2026-11-14-muse-transcript-not-read.md)) would resume exactly this session. |
| `META_API_KEY` | Not checked. The help offers `muse auth`, `muse login` and `muse exec --api-key-stdin`. |
| `.muse/` tree | Muse reads project `.muse/hooks.json` (binary strings). User config lives in `~/.config/muse/`, sessions in `~/.local/share/muse/sessions/`. |

Also wrong in `muse.yaml`: `hooks: unsupported`. See
[2026-11-14-muse-hooks-not-wired.md](2026-11-14-muse-hooks-not-wired.md).
