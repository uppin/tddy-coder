# Changeset: Semantic session search (SQLite + Connect-RPC + web control)

**Date**: 2026-04-05  
**Status**: Complete  
**Type**: Feature

## Affected packages

- `tddy-core`
- `tddy-service` / `packages/tddy-service/proto/connection.proto`
- `tddy-daemon`
- `tddy-web`
- Product docs under `docs/ft/web/`

## Summary

Local session search uses a SQLite index under the Tddy data root, deterministic embeddings in `tddy-core`, `SearchSessions` on `ConnectionService`, generated TS clients, and a debounced `SessionSearchInput` in `tddy-web`. Feature documentation lives in `docs/ft/web/semantic-session-search.md`; package and cross-package changelog entries record the deliverable.

## Scope

- [x] SQLite schema + migration hook (`PRAGMA user_version`)
- [x] Text extraction and merge rules from `Changeset`
- [x] `SearchSessions` RPC + daemon handler
- [x] Web search control (debounced) + component test
- [x] Product and dev documentation updates

## Technical notes (State B)

- Index file: `{data_root}/session_search_index.sqlite3`
- Embedding model id: `tddy-hash-trick-v1-dim256` (dimension 256)
- Invalid session tokens on `SearchSessions` return **unauthenticated** Connect errors; whitespace-only queries return empty hit lists.

## References

- `docs/ft/web/semantic-session-search.md`
- `packages/tddy-daemon/docs/connection-service.md`
- `packages/tddy-core/src/session_semantic_search.rs`
