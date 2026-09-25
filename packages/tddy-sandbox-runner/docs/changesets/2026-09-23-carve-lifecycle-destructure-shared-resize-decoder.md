# 2026-09-23 — The runner decodes terminal resizes with `tddy_pty::strip_resize`

**Type:** Refactor

`#carve` 14/15, PR [#524](https://github.com/uppin/tddy-coder/pull/524), DRY #12 (`2c70f1ab`), a
consumer edit the developer accepted (2026-09-23). Cross-package entry:
[2026-09-23-carve-lifecycle-destructure.md](../../../../docs/dev/changesets/2026-09-23-carve-lifecycle-destructure.md).

`runner.rs` drops its own `strip_resize_escape` and imports `tddy_pty::strip_resize` under that
name, so its call sites and tests are unchanged. The crate gains a `tddy-pty` dependency, the
lightest crate that could hold the decoder. No behaviour change.
