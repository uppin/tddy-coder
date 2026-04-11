# Changelog and changeset index merge hygiene

Indexes and product changelogs are high-traffic files: many branches append release notes. Follow these rules so parallel work merges cleanly and Git’s **union** merge driver (see repo `.gitattributes`) stays effective.

## Goals

- **Different branches touch different lines** (or different new files).
- **No mass edits** to historical entries in the same PR as unrelated feature work.

## Cross-package index — `docs/dev/changesets.md`

- **One bullet = one physical line** (no soft-wrapped continuation lines). Put long detail in feature docs or an optional shard file (below).
- **Reverse chronological order**: add each wrapped changeset as a **new bullet immediately under** the intro block (same pattern as package indexes).
- **Do not rewrite** existing bullets to “clean up” wording or WIP references while other feature branches might still be open; do doc-only cleanup in a **separate** PR if possible.
- **Bullet shape**: `- **YYYY-MM-DD** [Type] **Title** — Summary with links. (packages)`

### Optional long-form shard — `docs/dev/changesets.d/`

For cross-package work with a long narrative, add **`docs/dev/changesets.d/YYYY-MM-DD-short-slug.md`** (one file per wrapped changeset). Keep **`docs/dev/changesets.md`** to **one new index line** that links to that file plus the usual links. New files almost never conflict.

See [changesets.d/README.md](../changesets.d/README.md).

## Package indexes — `packages/*/docs/changesets.md`

Same rules as the cross-package index: **single-line bullets**, **prepend only**, **no retroactive rewrites** in shared merge windows.

## Product changelogs — `docs/ft/{coder,daemon,web}/changelog.md`

- **Newest first**: each release is a **`## YYYY-MM-DD — Title`** section **below** the opening paragraph.
- **Distinct titles**: if two releases land on the same calendar day, use different slugs in the heading (e.g. `## 2026-04-06 — Codex OAuth relay` vs `## 2026-04-06 — GitHub PR MCP tools`) so two branches are less likely to edit the **same** heading line.
- **Prefer single-line bullets** under each `##`; move long prose into feature docs and link.
- **Do not edit** older `##` sections for unrelated follow-ups—add a new `##` at the top.

## Wrap workflow (`/wrap-context-docs`)

When wrapping, add the new index line or `##` section **without** reformatting or reordering older entries. Transfer detailed State B into permanent feature/package docs first; the index is an audit trail, not the full story.

## What union merge does not fix

If two branches **change the same line** (same bullet or same `##` line), Git still conflicts. Append-only discipline and optional shards avoid that.
