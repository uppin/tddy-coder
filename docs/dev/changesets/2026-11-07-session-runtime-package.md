# 2026-11-07 — Session runtime and terminals in `tddy-session-web` (#web-split 16/17, PR #860)

**Type:** Architecture change

Moves the session runtime into `tddy-session-web`: `SessionRuntime` and its child and dormant
variants, the runtime registry, `GrpcSessionTerminal` and the terminal tab strip, tmux sessions,
attach claims and the attachment hooks, with 15 specs that now run on the clients and media port
doubles instead of a LiveKit connection provider.
Deviations from the plan: `SessionMainPane` stays in `tddy-web` for the inspector node (#861) — it
composes the inspector, agent layers and code pane; the one assertion about the provider's token mint
moved out with LiveKit (covered by `tddy-livekit-web`'s `sessionConnection.test.ts`).
At wrap: `bun.lock` regenerated (identical to the green's hand edit); typecheck 4 → 1, the remaining
error is `tddy-tauri-web`'s (`Web lint` excludes the package on that record); one of
`tddy-terminal-web`'s recorded errors fixed in passing (15 left).
Package detail: [tddy-session-web](../../packages/tddy-session-web/docs/changesets/2026-11-07-session-runtime-package.md),
[tddy-web](../../packages/tddy-web/docs/changesets/2026-11-07-session-runtime-package.md).
Product description: [session-package.md](../ft/web/session-package.md).
