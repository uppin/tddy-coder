# 2026-08-14 — `tddy-rust-typescript-tests/gen/` is badly stale and nothing detects it

**Category:** Future enhancement
**Source:** remote-managed-worktree changeset, 2026-08-14

Running `bun run generate` in that package produces **12 files that were never checked in**
(`actions_pb`, `bsp_pb`, `tasks_pb`, `vm_pb`, `vnc_pb`, `sandbox_pb`, the `grpc/reflection` and
`tddy/acp` trees, …) and rewrites three that were, including a 5182-line diff to `connection_pb.ts`.
So the committed set is a curated subset frozen at some past point, and the checked-in files have
drifted behind the proto they are generated from — `auth_pb.ts` is missing `RefreshSession`, added
some time ago.

Nothing catches this: no CI step regenerates and diffs, and the package's own `bun test` needs a
built web bundle, so it does not run in an ordinary check either. A proto change can therefore land
with this package silently describing a different wire contract than the daemon serves — which is
precisely what an interop test package exists to prevent.

Left untouched by the remote-managed-worktree changeset deliberately: regenerating it there would
have added ~5 000 lines of unrelated churn to a feature PR. Worth either regenerating and committing
the whole set in a change of its own, adding a CI drift check, or deleting the directory if the
package is no longer exercised.
