# 2026-07-26 — tddy-sandbox-recipes — host-independent sandbox path tests

**Category:** Future enhancement
**Source:** acp-tool-detail-explicit-states changeset, 2026-07-26

- **`cursor_agent_prerequisite_reads` asserts a machine-specific path** —
  `packages/tddy-sandbox-recipes/src/cursor_cli.rs:509` asserts `/Users` appears among the path-traversal
  ancestor grants. That only holds where `HOME` sits under `/Users` (macOS); on Linux `HOME=/var/tddy`, the
  ancestors end at `/var`, and the test fails permanently. Introduced by c018a176 (#303) and already on
  `master`, so **no Linux developer can get a clean `cargo test --workspace`** — which trains everyone to
  ignore workspace failures.
  - **Tests should stub their environment** rather than read the ambient one: have
    `cursor_agent_prerequisite_reads` take the home/share roots (or resolve them through an injectable
    provider) so a test can pass a `tempfile::tempdir()` root and assert the *shape* of the ancestor chain —
    "every ancestor from the install dir up to the filesystem root is granted" — instead of a literal
    prefix belonging to one OS.
  - **Production code should be testable by design**: the same seam removes the hidden `HOME` dependency
    from the recipe, which is the actual reason the assertion had to name a real directory.
  - Audit sibling recipes for the same pattern before fixing just this one assertion.
