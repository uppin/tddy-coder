# 2026-11-28 — Fs grows create_new_with_mode

**Type:** Refactoring · PR [#1050](https://github.com/uppin/tddy-coder/pull/1050) (`#fs-port` 6/13)

One additive port method for the activity node: `Fs::create_new_with_mode` — exclusive create
at exactly a mode, never wider, not even briefly — with `DiskFs` + `MemFs` implementations and
conformance tests. No existing signature or behaviour changes. Cross-package entry:
[`docs/dev/changesets/2026-11-28-fs-port-activity-fs.md](../../../../docs/dev/changesets/2026-11-28-fs-port-activity-fs.md).
