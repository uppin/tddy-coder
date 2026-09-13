# Agent context files (tddy-session-files)

The three RPCs that serve a repository's **agent configuration** — whatever the session's backend
reads for guidance — and the syncer that keeps a managed agent's context directory matching it.

Product contract: [agent-context-sync.md](../../../docs/ft/daemon/agent-context-sync.md).

Modules: `context_files` (the gate, the reads, the framing) and `context_sync` (the decision
procedure).

## A separate reader from the Code pane's, deliberately

Policy lives in `context_files`, not in `tddy_worktree_service::worktree_files`. That crate's
`ReadWorktreeFile` gates on git's listing, and that gate is load-bearing: it is what keeps a
`.gitignore`d `.env`, a credential a build wrote or a private key unreadable. But agent configuration
is routinely gitignored — Claude Code writes `.claude/settings.local.json`, and this repo's own
`.gitignore` hides `**/.cursor/mcp.json` and `**/.cursor/hooks.json` — so a context sync built on that
reader would omit precisely the files the agent reads.

The two **never share a gate**. They share only the traversal and containment guards
(`validate_rel_path_shape`, `canonicalize_root`, both public in
`tddy_worktree_service::worktree_files`), and `context_files` replaces the git-listing gate with
`tddy_sandbox::matches_context_globs`.

## The gate is a compiled-in allow-list

Three properties make replacing the git gate safe, and all three are load-bearing:

- **No caller supplies globs.** They are compiled into
  `tddy_core::backend::context_globs_for_agent`, and a request names a table row, never a path set.
  There is no spelling of a request that widens the readable set to reach `.env`.
- **No caller chooses the row.** The serving host derives it from persisted session state, through
  the `SessionContextScopes` port. `ContextManifestRequest.agent` is **advisory**, logged when it
  disagrees. A session recording a paired split agent gets Claude's row — the codebase half of a
  split placement is persisted as `workspace` but stands in for a `claude-cli` agent on another
  host, so `session_type` alone would wrongly reduce it to the shared base.
- **A resolved symlink's target must itself be allow-listed.** The allow-list is applied at *both*
  ends — the name a file is asked for by, and the place its target sits in the tree — which is the
  rule `tddy_sandbox::ContextManifest::of_worktree`'s walk applies to the entries it advertises. The
  two must agree, or `.claude/creds -> ../.env` is served under a spelling every glob matches, out of
  a manifest that correctly never listed it.

One property of the sibling reader is preserved verbatim, and it constrains the order of the checks:
a path the allow-list does not name is refused with the same code **and the same message** whether or
not a file sits there. The gate is therefore asked before anything touches the filesystem.

`CONTEXT_EXCLUDE_GLOBS` narrows the row further.

## The three reads

- **`StreamContextManifest`** — one `ContextManifestEntry` (`rel_path`, `sha256`, `size_bytes`) per
  allow-listed path. It streams rather than returning a repeated field so a large manifest never
  needs the transport's chunk codec. Hashing is streamed through a `BufReader`, and an over-cap file
  is left **out** of the manifest: advertising something the reader would then refuse makes a session
  unstartable.
- **`StreamReadContextFile`** — raw bytes, no encoding applied, framed at `CONTEXT_FILE_FRAME_BYTES`,
  which is *defined as* `tddy_daemon_kernel::HOST_DOCUMENT_FRAME_BYTES` (48 KiB) rather than as the
  same number. The budget is a property of the transport, not of what rides on it, and two constants
  free to drift are two answers to one question. Staying under
  `tddy_livekit::chunking::MAX_CHUNK_FRAME_BYTES` keeps a context read off the chunking codec
  entirely. Over-cap is refused **before the first frame**, never truncated.
- **`StreamReadContextFileBatch`** — several allow-listed files in one call, so a split start costs
  **2** peer round trips rather than 1 + N. Frames carry `rel_path` and `end_of_file`, so every file
  yields at least one frame and a zero-byte file stays distinguishable from a failure. Same gate,
  auth, per-file cap and aggregate cap as the single-file reader; the sizing check is shared
  (`sized_context_file`) so the two cannot drift.

All three run their blocking filesystem work under the host's `context_read_deadline`. A read that
stalls is refused with `DEADLINE_EXCEEDED` naming the configuration key, rather than holding the RPC
open forever — pinned by `context_read_deadline_acceptance.rs`, which parks its one blocking thread
on a channel so the stall is deterministic rather than timed.

## One decision procedure, two sources

A managed-codebase agent reads its working directory for guidance — `CLAUDE.md`, `.claude/`,
`.cursor/`, `.agents/` — and that directory is not the repository. It is built at spawn from the
target repo under the backend's glob allow-list, and it has to keep matching while the session runs:
a `CLAUDE.md` the developer edits an hour in is a rule the agent must start obeying, and one the
developer deletes is a rule it must stop obeying.

`ContextSource` is the seam, with two implementations:

| Source | Where the bytes come from |
|---|---|
| the split half's | the host holding the codebase, over LiveKit — `StreamContextManifest` then `StreamReadContextFileBatch` |
| `LocalWorktreeSource` | the worktree sitting beside the agent, read directly with no RPC at all |

Both hand `ContextSyncer::tick` a `tddy_sandbox::ContextManifest`, and the diff decides the rest. That
is the point: a session whose worktree is local cannot get a different sync than one whose worktree is
a hop away. `LocalWorktreeSource` goes through the same `context_files` reader the remote half is
served by, so the allow-list is enforced once, in one place, for both — and its `max_bytes` is the
host's configured `max_attachment_bytes` passed in, not a constant, because a constant that merely
happened to equal the shipped default would give the two halves different caps the moment an operator
tuned theirs.

`ContextSource` is declared in `context_sync` and re-exported from the crate root rather than being
declared at the root: `split_session`, which stays in `tddy-daemon`, implements the decision procedure
against it, and two declarations of one trait is how the split half and the co-located half stop being
obliged to sync identically.

## The consumer

`split_context_from_codebase_host` (in `tddy-daemon`, beside the split-placement logic it belongs to)
builds a split session's context directory from these reads at start *and* at resume. A failed fetch
is a **refusal, never an empty result** — a split session that cannot read its project's guidance does
not start. It reads through the served surface rather than re-implementing the read, so the deadline,
the gate and the path are one.

## Tests

| Suite | Where | What it pins |
|---|---|---|
| `context_files_acceptance.rs` | this crate | the gate, the caps, byte-exactness, the batch |
| `context_file_frames_unit.rs` | this crate | the framing against the chunk budget |
| `context_read_deadline_acceptance.rs` | this crate | the deadline, with a deterministic stall |
| `context_rpc_session_scope_acceptance.rs` | `tddy-daemon` | row derivation over the real handlers |
| `context_sync_acceptance.rs` | `tddy-daemon` | setup and diff, against the split-context directory builder |
