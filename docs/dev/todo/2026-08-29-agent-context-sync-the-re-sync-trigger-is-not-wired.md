# 2026-08-29 — Agent context sync — the re-sync trigger is not wired

**Category:** Future enhancement
**Source:** agent-context-sync changeset, 2026-08-29

The feature's headline claim is "continuously synced". **The setup sync landed; the continuous half
did not.** A managed session's context directory is built from the target repo's agent
configuration at start and at resume, and never updated again for the life of the session.

`ContextSyncer::tick` is implemented and tested; nothing calls it on a `worktree.activity`
broadcast. Marked `TODO(agent-context-sync)` at `packages/tddy-daemon/src/context_sync.rs`.

- **Nothing in the daemon constructs a `ContextSyncer` or a `LocalWorktreeSource` at all**, so the
  whole subtree behind `tick` is reachable only from tests: `ContextSyncer::new`/`tick`,
  `LocalWorktreeSource`, `diff_manifests`/`ManifestDiff`, and the entire staleness mechanism
  (`mark_context_stale`/`clear_context_stale`, `with_stale_marker`/`without_stale_marker`). PRD
  AC25–AC30 are proven at the `tick` level and production-unreachable.
- **Blocker 1 — `ContextSource` is synchronous; the split half's transport is async.** Setup gets
  away with it by prefetching every path into `PrefetchedContext`, which is correct there because
  populating an empty directory reads everything anyway. A *tick* must not: prefetching every tick
  transfers the whole set every two seconds, which is exactly what AC26 forbids. Needs an
  `#[async_trait]` source (with `LocalWorktreeSource` wrapping its reads in `spawn_blocking`, as the
  daemon does with filesystem work elsewhere) — `PrefetchedContext` then stays a separate concrete
  type so `populate` can remain synchronous. A `Handle::block_on` inside a `spawn_blocking`'d tick
  is the other option and is a known footgun.
- **Blocker 2 — the natural hook is the session-room poll loop, not a LiveKit subscription.** On
  both halves the daemon that would tick is the one hosting the room and publishing the broadcast,
  so subscribing to its own publication is a detour. Threading a syncer through
  `SessionRoomHosting` touches every `open_measured_by` call site. And on the co-located path the
  directory to write is the jail's `<sandbox_root>/context`, which `tddy-sandbox-app` populates by
  copying a staged tree in — ownership of that directory once the jail is up has to be settled
  before anything writes into it mid-session.
- Two items are dead for unrelated reasons and want an explicit decision rather than looking
  pending: `CodingBackend::context_globs()` has no production caller (every real site uses the free
  function; it exists to satisfy AC1), and `copy_tree_within_root` was orphaned by that changeset
  when the copier moved to globs.
