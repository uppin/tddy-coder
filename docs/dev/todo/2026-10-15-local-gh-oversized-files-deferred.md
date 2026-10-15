# 2026-10-15 — Files #622 (local gh) grew while already over the 500-line budget

**Category:** Deferred decomposition (developer consent given at `/pr-wrap` step 3.5, 2026-10-15)
**Source:** [2026-10-15-local-gh-github-source.md](../changesets/2026-10-15-local-gh-github-source.md)

| File | Production lines | Record |
|---|---|---|
| `tddy-daemon-rpc/src/pr_stack/ports.rs` | 808 → 819 | `oversized-file-pr-stack-ports.md` |
| `tddy-daemon-rpc/src/project/coordinate_handlers.rs` | 650 → 658 | `oversized-file-project-coordinate-handlers.md` |
| `tddy-host-service/src/host_tooling.rs` | 759 → 775 | `oversized-file-host-tooling.md` (new) |
| `tddy-host-service/src/service.rs` | 934 → 937 | `oversized-file-service.md` |
| `tddy-coder/src/run.rs` | 2,673 → 2,678 | `oversized-file-run.md` |

Growth is 3–16 lines each, all load-bearing for the feature. The seams are in the code-issue records.
Do these as one restructure pass after this PR merges.
