# 2026-11-11 — OpenCode usage from its SQLite store, with cost

**Type:** Feature

`#usage-binding 5/5`, PR [#894](https://github.com/uppin/tddy-coder/pull/894), stack
[#895](https://github.com/uppin/tddy-coder/stack/895). Feature:
[session-token-accounting.md](../../ft/coder/session-token-accounting.md).

## What changed

- **`tddy-usage`** — a generic, read-only `sqlite` format (`SqliteSource`, `RootMatch`): table and
  columns declared in the binding and validated as identifiers at load, the newest root row matching a
  column template as the main conversation, children by parent column recursively (depth 16),
  counters as column expressions, cost, model and name columns. Adds `rusqlite 0.32` (bundled), the
  version `tddy-semantic-index` already uses; no new crate version in `Cargo.lock`.
- **`tddy-coding-agent-specs`** — `opencode.yaml` binds `session_v2`.

## Deviations from the plan

- **A missing store or root row gives a zero main row** (as a missing Claude transcript does), not
  "no rows"; a store that cannot be read now gives no rows so the caller keeps its snapshot.
- **Open flags, not a `mode=ro` URI** — the same read-only guarantee without URI quoting of the path.
- **`turns` is 0** (`session_v2` has none).
- **The reasoning overlap is unverified** — the local store held no sessions:
  [todo](../todo/2026-11-11-opencode-reasoning-overlap-unverified.md).
- **The fixtures are synthetic rows** in a store built with the real `session_v2` DDL.

## Wrap fixes

- The recursive walk takes a `Walk` struct instead of an `#[allow(clippy::too_many_arguments)]`.
- A row without a usable id is skipped with a logged warning rather than silently.

## Code issues

- No code-issue record added; none existed in `tddy-usage` or `tddy-coding-agent-specs`.
- No file passes 500 production lines.
