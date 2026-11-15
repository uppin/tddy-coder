# 2026-11-15 — `schema-manifest.json` no longer depends on which packages a build includes

**Type:** Fix

`build.rs` inserts the manifest's keys in sorted order, which `serde_json` writes the same with or without `preserve_order`. See [workflow-schemas.md](../workflow-schemas.md).

Cross-package entry: [2026-11-15-session-display-name.md](../../../../docs/dev/changesets/2026-11-15-session-display-name.md). PR [#910](https://github.com/uppin/tddy-coder/pull/910).
