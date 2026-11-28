# Agent session token expires 24 h after spawn and is never refreshed

**Status:** open · **Found:** 2026-11-28 (investigation of orchestrator `01a20f10-4a30-7df1-8c63-d12aec760d68`)

## Symptom

A long-running orchestrator on LT-R1VXTH2V6H lost every `tddy-tools` MCP tool at once. Every call
after 2026-11-27 21:18:35Z was refused with `UNAUTHENTICATED`, until the session was respawned.

## Evidence

The token is in `~/.tddy/sessions/01a20f10-…/claude-mcp-config.json` (written 2026-11-26 23:18:35 EET,
when the daemon respawned the agent):

```
TDDY_REMOTE_SESSION_TOKEN = v2.{… "iat":1795727915, "exp":1795814315, "kind":"access",
                                "principal":"agent", "session_id":"01a20f10-…"}
iat = 2026-11-26T21:18:35Z   exp = 2026-11-27T21:18:35Z   (exp - iat = 86 400 s)
```

Agent transcript (`~/.claude/projects/…-home/ca4f5f40-….jsonl`):

```
2026-11-27T21:16:26Z  ReadSessionTerminal  -> ok            (last success, 2 min before exp)
2026-11-27T21:23:29Z  work_pr_read         -> {"error":"tool rpc call: UNAUTHENTICATED: daemon
                      LT-R1VXTH2V6H could not verify the session token (invalid or expired there); …"}
… 22 more refusals through 2026-11-28T07:52:58Z, every one from LT-R1VXTH2V6H
```

Daemon log (`~/.tddy/logs/daemon`): 22 lines, `23:23:29.333` (EET) to `09:52:55.584`, all
`exec tool … for session 01a20f10-… refused on daemon …: the session token could not be verified here`.
No other session is refused in that log.

The component that refused is the LT daemon's exec-tool gate,
`packages/tddy-session-agents/src/exec_tool_caller.rs:27` (message at `:40`). The token failed
`exp` in `packages/tddy-github/src/session_token_v2.rs:405`.

## Code path

| Step | Where |
|------|-------|
| TTL: 24 h | `packages/tddy-session-split/src/split_session.rs:23` `SPLIT_AGENT_TOKEN_TTL` |
| Mint (every agent token: local, split and project home) | `packages/tddy-session-split/src/agent_credentials.rs:37-45` |
| Put in the MCP server env at spawn | `packages/tddy-session-split/src/coding_agent_wiring.rs:58` → `claude-mcp-config.json` |
| Re-minted only when the daemon respawns the agent | `packages/tddy-agent-launch/src/cli_agent_steps/resumed_wiring.rs:148` ("minted now — nothing persists the previous process's token") |
| Read once from env by the MCP child, never re-read | `packages/tddy-session-tool-client/src/lib.rs:274,294` |
| Validated, no grace or refresh | `packages/tddy-github/src/session_token_v2.rs:405` |

Nothing refreshes the token while the agent process lives. A daemon restart does not invalidate it,
because the key persists. A resume through the daemon re-mints it. Typing "resume" in the agent's own
chat does not, because the process and its env stay the same. The agent's 30-minute cron kept the
process alive past 24 h, so expiry was certain.

## Root cause

The 24 h TTL was sized to "one agent process's working life" (comment at
`agent_credentials.rs:21`). Orchestrators break that assumption: they run for days. No component
renews the token, and the agent has no tool that works without one.
