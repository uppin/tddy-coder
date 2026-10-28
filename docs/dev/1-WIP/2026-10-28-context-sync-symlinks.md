# Changeset: context-sync-symlinks

PRD (target): [`docs/ft/daemon/agent-context-sync.md`](../../ft/daemon/agent-context-sync.md)
(amended 2026-10-28 — Design § *Two streaming RPCs* point 3, AC19–AC19c)

## Problem

Found live in split session `01a180c8-ff8c-7e42-8843-70d7ffeff84e` (agent on the Mac, codebase on
`mantas-udoo-ros`): the agent's context dir held `.agents/**` but no `.claude/skills`,
`.claude/commands` or `.claude/agents` — in this repository those three are symlinks into
`.agents/`, and Claude Code reads them only under `.claude/`. The agent started with the project's
rules and none of its skills, commands or subagents.

Cause: `walk_context_files` (`tddy-sandbox/src/context_manifest.rs`) followed a directory link only
when no other spelling of that directory had been visited ("listed once, under whichever spelling
the walk reached first"). `.agents/skills` came first in `read_dir` order, so `.claude/skills` was
dropped, and nothing about the link itself was ever carried — so the consumer could not recreate it.

## State B

- **Links travel as links.** `ContextEntry.symlink_target` / proto `ContextManifestEntry.symlink_target`
  (field 4, empty = ordinary file) name the root-relative target. A link entry carries no bytes:
  `size_bytes = 0`, `sha256 = sha256_hex(target)`, so retargeting is a diff like any edit.
- **An allow-listed link pulls its target in**, even when no glob names the target — one hop only.
  A target is withheld when it escapes the root, is excluded (`CONTEXT_EXCLUDE_GLOBS`) or matches the
  new `CONTEXT_SECRET_GLOBS` (`**/.env`, `**/.env.*`, `**/*.pem`, `**/*.key`, `**/.git`, `**/.git/**`).
- **Reader** (`context_files.rs`) serves a path no glob names only when an allow-listed link reaches
  it, so the manifest and the reader still agree.
- **Consumer** (`context_sync.rs`, `context_dir.rs`) recreates links relative to their parent,
  re-points a retargeted one, removes a dropped link without following it, never writes a file
  through a link, and re-checks every served link (escape / secret → `PERMISSION_DENIED`).
- **Split fetch** (`svc_split_context_from_codebase_host.rs`) maps the proto field and excludes link
  entries from the batch read and the size/hash checks.

## Open decisions

- ⚠️ **A link that *is* an allow-listed directory does not travel.** `.claude/**` matches what is
  inside `.claude`, not `.claude` itself, so `.cursor -> .claude` or `.claude -> shared/claude` is
  not synced — a narrowing for repos that used to get `.cursor -> .claude` followed. Proposed:
  also count a link whose path is the root of a `dir/**` glob.
- ⚠️ **Pulled-in trees have no aggregate bound** beyond the per-file cap: a committed
  `.claude/x -> node_modules` lists all of it.

## Technical debt

- `TODO(agent-context-sync)` in `create_context_link` (`context_manifest.rs`): non-unix hosts return
  `Unsupported` rather than creating a link.

## Package docs to update at wrap

- `packages/tddy-sandbox/docs/architecture.md` — the symlink paragraph: links are carried as link
  entries, one-hop target pull-in, `CONTEXT_SECRET_GLOBS`.
- `packages/tddy-session-files/docs/agent-context-sync.md` — reader link-reach rule; syncer link
  recreation / deletion / receiving-side re-check; `SyncStateEntry.symlink_target`.

## Acceptance tests

Scoped runs, 2026-10-28, all ✅:

- [x] `tddy-sandbox/tests/context_manifest_acceptance.rs` — 24 passed (8 new link tests + 2 secret guards)
- [x] `tddy-session-files/tests/context_files_acceptance.rs` — 29 passed (2 new + 1 guard)
- [x] `tddy-session-lifecycle/tests/context_sync_acceptance.rs` — 26 passed (8 new)
- [x] `tddy-daemon/tests/remote_managed_worktree_cross_host_acceptance.rs` — 11 passed, incl.
  `a_split_sessions_context_dir_recreates_the_repos_claude_skills_link` (LiveKit)
- [x] `-p tddy-sandbox -p tddy-session-files -p tddy-session-split` — every suite 0 failed

## TODO

- [ ] Decide the directory-link question above (and add its test if option 2)
- [ ] Refactor pass over `context_manifest.rs` (~420 changed lines)
- [ ] Update the package docs listed above at wrap

## Files

- `packages/tddy-sandbox/src/{context_manifest.rs,context_dir.rs,lib.rs}`
- `packages/tddy-service/proto/session_files.proto`
- `packages/tddy-session-files/src/{context_files.rs,context_sync.rs}`
- `packages/tddy-session-split/src/svc_split_context_from_codebase_host.rs`
- Tests: the four suites above, plus `tddy-sandbox/tests/context_manifest_unit.rs` (field added)
