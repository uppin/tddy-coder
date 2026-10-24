# 2026-10-24 — Warp terminal sessions

A fifth CLI session type, **Warp** (`warp-cli`): Warp's interactive Agent CLI (`warp`) in a PTY in a
daemon-cut worktree on `warp-cli/<short id>`, its binary configurable under
`cli_harnesses.warp-cli.binary_path` (or `TDDY_WARP_CLI`). It takes no model — Warp's model is picked
inside Warp with `/model` — and no provider account: it runs on the person's own Warp sign-in, and a
start naming an account is refused by name. Warp prints `warp --resume <TOKEN>` as it exits; the daemon
reads that line off the terminal after every exit and records it in `.session.yaml`
(`harness_resume_token`), and **Resume** relaunches `warp --resume <TOKEN>`, or plain `warp` when no token
was recorded. Sandboxed (`FAILED_PRECONDITION`, "the sandbox runner does not host warp-cli") and
managed-codebase starts are refused. Deleting a session removes its daemon-managed worktree. See
[Warp CLI session](../warp-cli-session.md).
