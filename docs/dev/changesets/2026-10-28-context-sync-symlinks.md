# Context sync carries symlinks as links

**Date:** 2026-10-28 · **Type:** Fix · **Packages:** tddy-sandbox, tddy-session-files,
tddy-session-split, tddy-service (proto), tddy-session-lifecycle and tddy-daemon (tests) · PR
[#784](https://github.com/uppin/tddy-coder/pull/784)

A split session's agent ran with `.agents/**` but no `.claude/skills`, `.claude/commands` or
`.claude/agents`: in this repository those are symlinks into `.agents/`, and the manifest walk listed
an aliased directory only under whichever spelling `read_dir` reached first, carrying nothing about
the link itself. Claude Code reads those trees only under `.claude/`.

The manifest now carries a symlink as a link (`symlink_target`, proto field 4) and the consumer
recreates it. An allow-listed link — a glob names it, or it is a `dir/**` root — pulls its target in
one hop, gated on git not ignoring it (failing closed), with `CONTEXT_SECRET_GLOBS` checked at every
depth and a read-as exclusion across all links. A link at `CLAUDE.md`/`AGENTS.md` is followed, so
the managed-codebase preamble always leads it. The receiving side refuses an entry beneath a served
link and every write verifies its parent resolves to its logical path — closing a nested-link escape
found in review before merge. The co-located source answers a tick's reads from its manifest's walk.

Feature: [agent-context-sync.md](../../ft/daemon/agent-context-sync.md) (Design 3–5, AC19–AC19d).

**Restructure** (file-length gate, production lines; master → peak → final):
`tddy-sandbox/src/context_manifest.rs` 542 → 1296 → 430, into `context_manifest/{links,walk,git_ignore}.rs`
(381 / 435 / 115); `tddy-session-files/src/context_sync.rs` 634 → 869 → 398, into
`context_sync/{manifest_gate,context_writes,sources}.rs` (143 / 267 / 129); `tddy-session-files/src/context_files.rs`
479 → 578 → 457, into `context_files/worktree_reach.rs` (177). Engine-driven `move_item`s;
`restructure verify` accounted for every statement.

**Deferred:** [unbounded pulled-in trees](../todo/2026-10-28-context-sync-link-pulled-in-trees-are-unbounded.md),
[private worktree exclude](../todo/2026-10-28-context-sync-git-gate-misses-private-worktree-exclude.md),
[submodule paths fail a whole stage](../todo/2026-10-28-context-sync-git-gate-fails-a-whole-stage-on-one-submodule-path.md),
[case-folded directories on macOS](../todo/2026-10-28-context-write-guard-refuses-case-folded-directories-on-macos.md),
[check-then-use races](../todo/2026-10-28-context-reader-resolve-then-read-race.md); the
`TODO(agent-context-sync)` for non-unix link creation; function-length extractions the engine could
not perform (`synchronize_directory`, `collect_allow_listed`, `resolve_allow_listed_context_file`).
