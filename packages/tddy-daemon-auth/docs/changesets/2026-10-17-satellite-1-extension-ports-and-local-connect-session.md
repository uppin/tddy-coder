# 2026-10-17 — Boundary suite reads WorkspaceGraph

**Type:** Refactor

`#satellite` 1/13 ([#623](https://github.com/uppin/tddy-coder/pull/623)); cross-package entry:
[2026-10-17-satellite-1-extension-ports-and-local-connect-session.md](../../../../docs/dev/changesets/2026-10-17-satellite-1-extension-ports-and-local-connect-session.md).

`tests/dependency_boundary_unit.rs` reads `WorkspaceGraph` instead of a manifest walker of its own, so
a table-form or workspace-inherited dependency on `tddy-daemon` is seen. The "reaches the kernel"
guard is kept, so a walk that found nothing cannot pass as clean. The test that read the crate's
own `[dev-dependencies]` is now `does_not_declare_the_daemon_as_a_dependency_of_its_own`, which sees
normal, build and optional dependencies only.
