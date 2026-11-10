# 2026-11-07 — An ACP agent run as another user through `setpriv` keeps the daemon's `USER`/`LOGNAME`

**Category:** Bug (agent environment)
**Source:** `#agent-tools` 28/28 (`env-allow-list`, #836), pass-2 premise check. Owner: the `#agent-tools`
sweep node.

## What is there

`tddy-session-lifecycle/src/coding_agent_as_user.rs` (`wrap_for_user`, ~line 95) starts a piped-stdio ACP
agent as the session's OS user by wrapping its argv in `setpriv`. It overrides `HOME` and `PATH`
(`pty_user_env_overrides`) but not `USER` or `LOGNAME`. Those are in the agent-env base set
(`tddy-coding-agent-specs/specs/agent-env.yaml`), so once #836 is green the agent keeps the **daemon's**
values: an agent running as `alice` is told it is `tddy`.

The supervisor's relayed path does not have this problem. `ChildEnvironment::build` sets the account's own
`HOME`/`USER`/`LOGNAME` after the request's variables.

## What to decide

Set `USER` and `LOGNAME` to the target account in `wrap_for_user`, beside `HOME`, and add a unit test next to
the existing `HOME`/`PATH` override test. No other behaviour changes.

**Deferred by the user, 2026-11-10** (#agent-tools TODO gate after the sweep #876): asked "continue the stack or defer?", the user answered "Defer". Left open for a later, separately planned node.
