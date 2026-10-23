# 2026-10-22 — "no code index" still explains itself as "not a Rust worktree"

**Category:** Wording
**Found in:** `#reshape` 22/24 (`extension-language-server`, PR #681), changeset open question "Wording"
**Owner:** unassigned — a follow-up PR across `tddy-daemon-rpc`, `tddy-service` (proto) and `tddy-web`

## What happens

Since #681 a worktree has a code index when it is a Rust worktree **or** holds a file of an installed extension
language with a configured server (`serves_workspace(root, supports)`). A worktree with neither is answered `NONE` with
`without_an_index::NOT_A_RUST_WORKTREE` (`packages/tddy-daemon-rpc/src/code_session/without_an_index.rs`): "this
worktree has no Cargo.toml at its root, so it is not a Rust worktree and has no code index". True for the worktrees
it is shown for, but it names only Rust, so it does not tell the reader that an extension language would also do.

## Why it was not changed in #681

The words are repeated in places outside the packages #681 owns: the `CODE_INDEX_STATE_NONE` comment in
`packages/tddy-service/proto/code_navigation.proto` and the generated `code_navigation_pb.ts` copies under the
`scripts/generated-code.sh` drift gate, `useCodeIndexProgress.ts`, the `CodeIndexStatusStrip` spec, and the
`tddy-service` / `tddy-daemon` docs. The constant is pinned by `packages/tddy-daemon/tests/code_session_acceptance.rs`
(`NOT_A_RUST_WORKTREE`). Changing one without the others leaves the schema contradicting the message.

## Wanted

One wording — e.g. "this worktree has no Cargo.toml at its root and no file of an installed language with a
configured server, so it has no code index" — in the constant and its test, the proto comment (regenerated), the web
hook's comment and the docs.
