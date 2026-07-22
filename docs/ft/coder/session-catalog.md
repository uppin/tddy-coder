# Session catalog (DB-backed targets + actions)

> **Related:** extends [session-actions.md](session-actions.md). The action-manifest **discovery**
> and **listing** described there move from per-call YAML globbing to a per-session SQLite catalog.
> Authoring, invocation, async jobs, and the CLI/MCP surfaces are unchanged.

## Status (2026-07-22)

**Producer landed; reads pending.** The catalog store, the `BUILD.yaml` provider, and the
worktree-open populate task are implemented: the coder writes `<session_dir>/catalog.db` on session
open (both coder run paths). **Reads are not yet served from the catalog** — `list-actions` still
uses the per-call YAML glob (`list_action_summaries`). The remaining work — cutting the read path
over to the catalog (a cross-process concern; see below) and triggering populate from the
daemon-managed flow — is tracked under *Future Enhancements* in `docs/dev/TODO.md` and lands in a
later change. The "block until first populate" / cross-process marker behaviour below describes the
target design that the read cutover will complete.

## Purpose

The **session catalog** is a per-session SQLite database that is the **single source of truth** for
what is listable in a session. It unifies two kinds of entries:

- **Action manifests** — the declarative YAML actions from [session-actions.md](session-actions.md)
  (per-repo store `~/.tddy/actions/<repo_key>/` + session overlay `<session_dir>/actions/`).
- **Build targets** ("tddy targets") — **auto-discovered** from `BUILD.yaml`/`build.yaml` across the
  repository (`tddy_build::discover_build_manifests`). These are **not** authored as manifest files;
  they appear in the catalog purely because a scan found them.

Before this feature, action manifests were re-globbed and re-parsed on **every** `list-actions`
call, and build targets were a separate listing that never appeared in the actions catalog. The
catalog makes listing fast (indexed), unified, and consistent.

## On-disk layout

- **Database**: `<session_dir>/catalog.db` (SQLite, WAL mode), beside `changeset.yaml` under
  `{sessions_base}/sessions/{session_id}/` (see [session-layout.md](session-layout.md)).
- YAML manifests and `BUILD.yaml` remain the **authoring inputs** the scan reads; they are not read
  at query time.

## Behaviour

### Populate on worktree-open (async, as a task)

When a session's worktree is opened, an **asynchronous scan** populates the catalog. The scan is a
first-class **`tddy-task`** (`kind = "session_catalog_populate"`) registered in the session
`TaskRegistry`, so it is observable and cancellable like any other task. The scan:

1. Discovers action manifests (per-repo store + session overlay).
2. Discovers `BUILD.yaml` build targets across the repo.
3. Rebuilds the catalog in **one transaction** (full replace — stale entries vanish, renames and
   edits are reflected). On WAL, concurrent readers see either the previous committed catalog or the
   new one, never a partial one.

### Block until first populate

A list query issued before the initial scan has finished **blocks until that populate task reaches a
terminal status**, then reads. Subsequent queries never re-block (terminal status is sticky). There
is no partial/empty fallback: the reader waits for a complete catalog. Auxiliary processes that read
`catalog.db` without owning the task handle bounded-wait on a durable `populated_at` marker written
inside the rebuild transaction, and error explicitly if it never appears.

### Entry shape

Each row stores the full entry as a JSON blob; columns are projected from the JSON purely to serve
indices.

| Field | Action manifest | Build target |
|-------|-----------------|--------------|
| `kind` | `action_manifest` | `build_target` |
| `id` | manifest `id` | target id, e.g. `packages/foo:binary` |
| `package` | parent directory of the manifest path | id prefix before `:`, e.g. `packages/foo` |
| `summary` | manifest `summary` | target `name` |
| `path` | rel path w/o extension (the `--action` handle) | the target id |
| `has_input_schema` / `has_output_schema` | from the manifest | `false` |

### Listing

`list-actions` and the equivalent programmatic/MCP reads return the same
`{ id, summary, has_input_schema, has_output_schema }` shape as before, sorted ascending by `path`,
and support the existing filters: a **package** scope (`path_prefix`), a case-insensitive substring
`query` over id/summary/path, and `limit`/`offset` pagination with a pre-pagination `total`. The
first supported index is **list targets for a package** (`package` equality/prefix).

## Acceptance criteria

1. Opening a worktree scans the repo and exposes a queryable catalog containing both action
   manifests and auto-discovered `BUILD.yaml` targets.
2. Listing targets for a given package returns exactly the entries whose projected `package` matches.
3. `package`/`path_prefix`, substring `query`, and `limit`/`offset` pagination behave exactly as the
   prior YAML-glob path (same sort, same `total` semantics).
4. `packages/foo:binary` projects to package `packages/foo`; manifest path `packages/foo/build`
   projects to `packages/foo`.
5. A re-scan rebuilds the catalog: deleted entries disappear, edited summaries update, new entries
   appear.
6. The populate task reaches `Completed`, and a list query issued before completion blocks until then
   returns the fully populated set.
