# 2026-11-07 — `debug` typed by `@types/debug` (#web-split 10/17, PR #847)

The hand-written `src/debug.d.ts` is replaced by the `@types/debug` devDependency, so every package
that compiles `src/lib/debugMask.ts` sees its types (it was the one `Web lint` error
`tddy-rpc-playground-web`, `tddy-terminal-web` and `tddy-session-create-web` inherited).
