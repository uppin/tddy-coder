# 2026-10-17 — Daemon git-over-SSH beyond the first clone cannot ask for a passphrase

**Category:** Future enhancement
**Source:** the clone passphrase prompt (2026-10-17)

- The clone passphrase prompt covers **only** the auto-clone in `ensure_project_available_for_start`.
  Fetch, push, worktree sync and the project coordinate handlers' clone
  (`packages/tddy-daemon-rpc/src/project/coordinate_handlers.rs:127,145,304,322`) run git over SSH
  without an askpass helper, so a passphrase-protected key that is **not** in the agent still hangs or
  fails there with no operator prompt.
- **Why deferred:** chosen in the planning interview to bound the change. Each caller needs its own
  answer to "who is the operator" and "how long may this wait", and several run without a person at the
  UI (background sync), where prompting is wrong and a readable failure is right.
- In practice the first clone leaves the key in the agent, so later operations by the same OS user
  usually succeed — the gap shows after an agent restart or on a host where the first operation was not a
  clone.
- Reuse: `clone_env` composes the askpass environment as a pure function; the other callers can adopt it
  once their operator and deadline questions are answered.
