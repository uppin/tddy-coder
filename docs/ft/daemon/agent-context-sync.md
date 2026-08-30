# Agent context sync (per-backend allow-list, continuously synced)

**Status:** 📝 Planned
**Product area:** Daemon
**Date:** 2026-08-29

## Summary

An agent working on a managed codebase reads its cwd for guidance — `CLAUDE.md`, `.claude/`,
`.cursor/`, `.agents/skills/`. Today that cwd is a **context directory** built once at spawn from a
hardcoded, backend-agnostic list, and on the split path it is not built from the repository at all:
it holds only the managed-codebase notice.

This feature gives every backend an **allow-list of globs** naming what that agent actually reads
from the *target repo*, and keeps the context directory **continuously synced** against those globs
for as long as the session lives — on both the co-located path (worktree beside the agent) and the
split path (worktree on another daemon, reached over LiveKit).

It closes `TODO(remote-managed-worktree)` at `packages/tddy-daemon/src/split_session.rs:136` and the
deferral recorded in [remote-managed-worktree.md](remote-managed-worktree.md) § Agent working
directory.

## User Story

As a developer running an agent against a managed codebase, I want the agent to see my repo's own
`CLAUDE.md`, skills and agent config — and to keep seeing them as I edit them mid-session — so the
agent follows my project's rules instead of working blind or from a snapshot taken at spawn.

## Why the current behaviour is not enough

Four distinct defects, each independently user-visible:

1. **The split path syncs nothing.** `build_split_context_dir` (`split_session.rs:139`) writes a
   single `CLAUDE.md` containing only the notice. A split agent never sees the project's guidance.
   The PRD records this and the reason it was deferred: fetching over the peer link "means bounded
   reads over the peer link plus a decision about what a failed fetch means — which is a fallback
   decision" ([remote-managed-worktree.md:334](remote-managed-worktree.md)).

2. **The co-located list is tddy-coder's own layout, not an agent convention.**
   `CONTEXT_DIRS = [".claude", ".agents", "skills", "docs"]` (`context_dir.rs:97`). In an arbitrary
   target repo `docs/` is a thousand files of unrelated prose and `skills/` means nothing. Meanwhile
   `.cursor/` — which a Cursor session actually reads — is **not synced at all**.

3. **It is a one-shot snapshot.** `SandboxContextDir::create` copies at spawn and then
   `make_readonly_recursive` freezes the result at 0444 (`context_dir.rs:210`). A `CLAUDE.md` edited
   an hour into a session is never seen.

4. **The notice is appended, so the agent reads it last.** `content.push_str(&appendix)`
   (`context_dir.rs:123-130`). The rule that matters most — *the codebase is elsewhere, use
   `mcp__tddy-tools__*`* — sits below however many thousand words the project's own file contains.

## Design

### The allow-list belongs to the backend

`CodingBackend` gains one method, defaulting to a name-keyed table so callers holding only an agent
name (which is every caller on the daemon side — the daemon dispatches on `session_type` strings,
not on `CodingBackend`) can reach the same data:

```rust
pub trait CodingBackend: Send + Sync {
    /// Worktree-root-relative globs naming what this agent reads from the target repo.
    fn context_globs(&self) -> &'static [&'static str] {
        context_globs_for_agent(self.name())
    }
}
```

Patterns are **worktree-root-relative** and matched against the target repo, never against
tddy-coder's own layout.

| Agent | Globs |
|---|---|
| *shared base* | `AGENTS.md`, `.agents/**` |
| `claude`, `claude-acp` | base + `CLAUDE.md`, `.claude/**`, `.mcp.json` |
| `cursor` | base + `CLAUDE.md`, `.claude/**`, `.cursor/**`, `.mcp.json` |
| `codex`, `codex-acp` | base + `.codex/**` |
| anything else | base |

`docs/**` and `skills/**` are **deliberately gone**. They are this repo's documentation conventions,
not agent-tool conventions, and on the split path every one of their files would cross the wire.

An unrecognised agent name falls to the shared base. That is a *narrowing* default — it can only
ever sync less than a known backend, never more — so it is not a fallback in the sense
[CLAUDE.md](../../../CLAUDE.md) forbids: no unrecognised name can widen what is readable.

### Two streaming RPCs, gated by the allow-list rather than by git

The existing `StreamReadWorktreeFile` cannot serve this. Its gate, `resolve_listed_worktree_file`
(`worktree_files.rs:223-243`), requires the path to be **listed by git**, deliberately:

> This gate exists to keep `.gitignore`'d paths — a local `.env`, a credential a build wrote, a
> private key — unreadable.

Agent config is routinely gitignored. In this repo alone: `.claude/settings.local.json` is ignored,
and `.gitignore:83,87` ignore `**/.cursor/mcp.json` and `**/.cursor/hooks.json`. So this feature adds
a **separate reader** whose gate is the compiled-in allow-list. The two never share a gate; they
share only the traversal/containment guards.

The allow-list is compiled in and derived from the **session's own** `session_type` on the serving
side — **no caller supplies globs, and no caller chooses the row**. That is the first half of what
makes replacing the git gate safe: a caller cannot widen the set to reach `.env`, because a caller
cannot name the set at all.

An earlier draft stopped the argument there, and it was incomplete in two ways that both had to be
closed before this reader was safe:

1. **The request must not choose the row either.** The first implementation took an `agent` name off
   the request and looked the row up from it. Since any row is compiled in, that still cannot name
   `.env` — but it makes the enforced bound the *union* of every backend's row rather than the
   session's own, so a token for one session type reads another agent's `.claude/**`, `.cursor/**`
   and `.mcp.json`. Those routinely carry API tokens in MCP `env` blocks, and they are exactly the
   gitignored files the git gate used to refuse. The serving daemon now derives the row from the
   resolved session; `agent` on the request is advisory and logged only.

2. **The repo can name a path the request cannot.** Restricting *requests* says nothing about what
   the worktree's own contents point at. A containment check that accepts any symlink whose target
   lands inside the root will happily serve `.claude/creds -> ../.env`: the *name* is allow-listed,
   the *bytes* are the secret, and on the split path they cross to another host into a directory the
   agent reads. So a resolved symlink's target must satisfy the allow-list too, not merely
   containment. AC19 covers both the outside-root case and this in-repo sibling case.

The general rule the two share: an allow-list gate is only as strong as the weakest thing permitted
to name a path — the request, the filesystem, or the peer.

```proto
// Every allow-listed path in the worktree, with the hash that says whether it moved.
rpc StreamContextManifest(ContextManifestRequest) returns (stream ContextManifestEntry);

// The bytes of one allow-listed path. Raw, byte-exact, refused over cap before the first frame.
rpc StreamReadContextFile(ReadContextFileRequest) returns (stream ContextFileChunk);
```

`ContextManifestEntry` is `{ rel_path, sha256, size_bytes }`. It **streams** rather than returning a
repeated field so a large manifest never has to be chunked. `ContextFileChunk` mirrors
`WorktreeFileChunk`: raw `bytes` at `HOST_DOCUMENT_FRAME_BYTES` (48 KiB), which is under the 60 KB
`MAX_CHUNK_FRAME_BYTES` budget, so the transport's chunking codec
(`packages/tddy-livekit/src/chunking.rs`) is never engaged. LiveKit itself does not chunk — an
oversized `publish_data` is rejected outright — which is why staying under the budget matters.

`size_bytes` rides the manifest entry so a client refuses an over-cap file *before* spending a read
on it.

### Re-sync is a manifest diff, driven by the broadcast that already exists

The session room's poll loop already measures the worktree every `session_room.poll_interval_ms`
(2 s default) and broadcasts `worktree.activity` when it moves. Both halves ride that signal — no
new timer, no filesystem watcher (there is none anywhere in the repo, and none is added).

On each broadcast the syncer fetches the manifest and diffs it against what it holds:

| Manifest vs. local | Action |
|---|---|
| hash differs, or path is new | stream the bytes, write |
| hash matches | nothing — no transfer |
| path absent from manifest | delete locally |

Steady state is therefore **one round trip that transfers no file content**.

The co-located path runs the identical diff, reading the manifest straight off the local worktree
instead of over RPC. One decision procedure, two sources.

### The notice is prepended, and names what the sync owns

The managed-codebase notice moves to the **top** of `CLAUDE.md` and `AGENTS.md`, above the project's
own content, on both paths. `SANDBOX_REMOTE_APPENDIX` / `sandbox_remote_appendix` are renamed to
`MANAGED_CODEBASE_PREAMBLE` / `managed_codebase_preamble`: "appendix" stops being true.

Where the repo has no `CLAUDE.md`, the context dir still gets one holding just the preamble — as the
split path already does today, and as the co-located path today does **not**.

The preamble names the synced path set, because the split context dir is deliberately writable (it
is the agent's only scratch space on host A) and **the sync overwrites**: a locally edited file under
an allow-listed path is replaced on the next tick. This matches how `tddy-session-sync` already
treats its `--dest` ("local edits under it are discarded"). Saying so in the preamble is what turns
that from a surprise into a rule.

### The context dir stops being 0444, and the jail denies writes to it instead

`make_readonly_recursive` goes. Continuous re-sync has to write into the live directory
(`<sandbox_root>/context`, which `spawn.rs` copies the staged tree into, permissions and all), and
chmod-dancing around every write is the wrong layer: the *host-side syncer* must write, the *jailed
agent* must not, and a file mode cannot tell them apart.

An earlier draft of this document justified the removal by saying the jail already mounts the
directory read-only. **That was false**, and it is worth recording because the error was
load-bearing: `context_dir` is `sandbox_root/context`, `project_root` is `sandbox_root`, and the
darwin profile emits `(allow file-write* (subpath project_root))` — so the context dir sat *inside*
the writable subpath and the 0444 mode was the only thing holding it. Removing the mode on that
reasoning would have removed the sole protection while claiming to remove a redundant one.

So the protection moves to the layer that can make the distinction. `render_plan`
(`tddy-sandbox-darwin/src/profile.rs`) now emits, **after** the project-root allow:

```
(deny file-write* (subpath "<context_dir>"))
```

Seatbelt is last-match-wins, so the deny governs inside the jail while the host-side syncer — which
is not confined by the profile — still writes freely. The context dir path reaches the profile from
the runner argv (`--context-dir`), which is the only channel that already carries it.

This is *stricter* than the mode it replaces: `make_readonly_recursive` chmod'd files 0444 but left
directory modes alone, so a jailed agent could still create new files in the context dir. The deny
blocks creation too.

The second defence is unchanged: the native filesystem tools are hard-disabled
(`NATIVE_FILESYSTEM_TOOLS`, `split_session.rs`).

### Failure is loud at setup and visible afterwards

**Setup sync failure fails the session start.** A split session that cannot read its target repo's
guidance does not start: `StartSession` returns an error and the worktree already created on the
codebase daemon is torn down, exactly as any other failed step in that sequence does. No partial
context dir, no silent degradation — the deferred fallback decision is resolved as *no fallback*.

**Re-sync failure marks the context stale.** The session keeps running (killing a working session
over a transient link drop would be worse than the staleness), and the preamble gains a line saying
the guidance may have drifted and when it was last confirmed. The next successful re-sync clears it.
The agent is never left silently reading a stale rule it believes is current.

## Acceptance Criteria

### Per-backend allow-list (`tddy-core`)

- **AC1** — `CodingBackend::context_globs()` defaults to `context_globs_for_agent(self.name())`, so
  a backend that overrides nothing still gets its own list.
- **AC2** — `claude` syncs `CLAUDE.md`, `AGENTS.md`, `.claude/**`, `.mcp.json`, `.agents/**`.
- **AC3** — `cursor` syncs everything `claude` does **plus** `.cursor/**`.
- **AC4** — `codex` syncs `AGENTS.md`, `.agents/**`, `.codex/**`, and not `.claude/**`.
- **AC5** — an unrecognised agent name yields the shared base, never the union of every list.
- **AC6** — no backend's list contains `docs/**` or `skills/**`.

### Glob matching and context-dir build (`tddy-sandbox`)

- **AC7** — a repo file matching a glob is copied into the context dir; one matching nothing is not.
- **AC8** — `**` matches at any depth: `.claude/skills/x/references/y.md` matches `.claude/**`.
- **AC9** — a symlink under an allow-listed path resolving **outside** the worktree root is skipped,
  as it is today.
- **AC10** — files in the built context dir are writable; nothing is left at mode 0444.

### Prepended preamble

- **AC11** — `CLAUDE.md` in the context dir **begins** with the managed-codebase preamble, and the
  repo's own content follows it intact.
- **AC12** — the same holds for `AGENTS.md`.
- **AC13** — when the target repo has no `CLAUDE.md`, the context dir still contains one holding the
  preamble alone.
- **AC14** — the preamble names the allow-listed paths as sync-owned and says edits to them are
  replaced.

### Manifest and read RPCs (`tddy-service`, `tddy-daemon`)

- **AC15** — `StreamContextManifest` yields one entry per allow-listed path, each carrying
  `rel_path`, `sha256` and `size_bytes`.
- **AC16** — it includes a **gitignored** allow-listed path, which `StreamReadWorktreeFile` refuses.
  Not `.claude/settings.local.json`, which AC31 excludes for a separate reason.
- **AC17** — it excludes a **tracked** path that matches no glob (`README.md`, `src/main.rs`).
- **AC18** — a `rel_path` containing traversal, or an absolute path, is refused.
- **AC19** — a symlink matching a glob is not served when its target is outside the worktree root,
  **nor when its target is an in-repo path the allow-list does not name** (`.claude/creds -> ../.env`
  must not publish `.env`'s bytes under an allow-listed name).
- **AC20** — `StreamReadContextFile` returns byte-exact content in 48 KiB frames; a zero-byte file
  yields exactly one empty frame.
- **AC21** — a path matching no glob is refused `PERMISSION_DENIED` **identically** whether or not a
  file exists at that name, preserving the existence-map property `resolve_listed_worktree_file`
  protects.
- **AC22** — a file over `max_attachment_bytes` is refused before the first frame, not truncated.

### Setup sync

- **AC23** — a split session's context dir is populated from the codebase daemon **before** the
  agent process spawns.
- **AC24** — a setup fetch that fails makes `StartSession` fail, and the worktree created on the
  codebase daemon is torn down.

### Continuous re-sync

- **AC25** — a `worktree.activity` broadcast triggers a manifest fetch.
- **AC26** — only paths whose `sha256` changed are re-read; an unchanged manifest transfers no file
  content.
- **AC27** — a path that disappears from the manifest is deleted from the context dir.
- **AC28** — a failed re-sync leaves the session running and adds a staleness line to the preamble.
- **AC29** — the next successful re-sync removes that line.
- **AC30** — the co-located path re-syncs on the same broadcast, reading the local worktree rather
  than issuing RPCs.

### Scope of the readable set

These were added after validation found the original set of criteria too weak to pin what the
security argument actually needs.

- **AC31** — `.claude/settings.local.json` is **not** synced by any backend. The daemon owns that
  file on a managed session: `write_claude_hooks_settings` replaces it wholesale after the setup
  sync, so syncing it means writing a file that is guaranteed to be discarded — and, once the
  re-sync trigger is wired, later overwriting the hooks with the repo's copy and silently ending
  status reporting for the session.
- **AC32** — a request naming a different agent than the session's own `session_type` is served the
  **session's** row. The readable bound is the session's row, never the union of every row.
- **AC33** — every glob in every row of the compiled-in table parses. An unparsable pattern narrows
  silently by construction, so nothing else would catch a typo that disabled a whole tree.
- **AC34** — a batched read of several allow-listed files round-trips each byte-exactly, keeps a
  zero-byte file distinguishable from a failure, refuses an unlisted path, and applies the same
  per-file and aggregate size refusals as the single-file read.

## Deployment

⚠️ **Both halves of a split placement must be upgraded together.** A split start and a split resume
each make two mandatory peer calls — `StreamContextManifest`, then `StreamReadContextFileBatch` — and
a failure of either is a refusal, not a degraded start. A codebase daemon running an older build
answers neither method, so **every split start and resume against an un-upgraded peer fails
outright**. Before this feature an ordinary split start made no context calls at all, so there was
nothing to be incompatible about.

This follows from the no-fallback rule rather than sitting in tension with it: the alternative is
starting a session whose agent silently has no project guidance because its peer is old, which is
the exact degradation the rule exists to prevent. It is nonetheless a rollout constraint rather than
a failure an operator can act on in the moment, so it belongs in release notes.

## Current state

**The setup sync is live; the continuous re-sync is not.** A managed session's context directory is
built from the target repo at start and at resume and is not updated again for the life of the
session — `ContextSyncer::tick` exists and is tested, but nothing calls it on a `worktree.activity`
broadcast, so AC25–AC30 are proven at the `tick` level and production-unreachable. The two blockers
are design decisions rather than wiring, and both are recorded in
[docs/dev/TODO.md](../../dev/TODO.md) § Future Enhancements.

Read "continuously synced" in this document as the intended contract, not as shipped behaviour.

## Out of scope

- **Cursor split placement.** Split is `claude-cli` only
  ([remote-managed-worktree.md:99-115](remote-managed-worktree.md)); `cursor_cli_spawn.rs:302`
  discards `managed_codebase`. The per-backend table is built in full, but only `claude`'s entry is
  exercised on the split path until that lands. Every entry is exercised on the co-located path.
- **Syncing *from* the agent dir back to the repo.** One-way, repo → agent, as today.
- **Per-project glob overrides.** The table is compiled in; letting a target repo name its own globs
  would reintroduce the widening risk the compiled-in set exists to prevent.

## Related

- [Remote managed worktree](remote-managed-worktree.md) — amended: § Agent working directory
- [Session worktree sync](session-worktree-sync.md) — the broadcast and streaming precedents reused
- [Session rooms](session-room.md) — the poll loop that produces the trigger
- [Managed codebase workflow](../coder/managed-codebase-workflow.md)
