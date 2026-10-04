# 2026-10-04 — The peer-forward deadline is a daemon setting

**Type:** Feature

`#e2e-leg` 2/5 ([#579](https://github.com/uppin/tddy-coder/pull/579)); cross-package entry:
[2026-10-04-e2e-leg-peer-forward-deadline.md](../../../../docs/dev/changesets/2026-10-04-e2e-leg-peer-forward-deadline.md).

The exec-tool ports (`exec_tool/ports.rs`) and `project.rs` forward through a `CommonRoom`. A type change
at the seam only; no file grew.
