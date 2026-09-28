# 2026-09-28 — A yielded conversation resumes with the caller's replacement call and result

**Type:** Feature

`#subagent-control` 5/5 (top), [#557](https://github.com/uppin/tddy-coder/pull/557) (base:
`feature/subagent-control/yield-conditions`, node 4). Consumes node 4's yielded outcome — the
`fromMessageId` the caller learned from the yield, the `StopReason::YieldedToCaller` machinery and
its tool-name vocabulary.

Packages: `tddy-discovery` (types, validation, transcript append, the resume path),
`tddy-session-agents` (request parsing + framing), `tddy-service` (`session_agents.proto`),
`tddy-tools` (the MCP `replacement` argument).

## What was delivered

A resume may carry a **`replacement {tool, arguments, result}`** — the caller substitutes its own
call and result for the one that yielded the conversation back:

- Appended **after any rewind and correction**, in that order, as an assistant tool-call message
  plus its tool result, each with a minted id (`call_replacement_{ordinal}` from the transcript's
  rising counter, never colliding with a provider's ids) — and the turn continues over the
  appended history.
- **Append-only**: the original failed call stays in the history; nothing is rewritten or removed.
- **The appended call never dispatches** — the result is the caller's text, recorded verbatim, so
  the transcript carries a call the model made, answered by the operator, with no tool run.
- **Validation before the turn runs, above the rewind** — a malformed replacement is refused with
  the history untouched: the tool must be one the session advertises (derived from the advertised
  definitions, not a hand-kept list), the arguments must pass the tool's own
  `validate_tool_arguments` schema, and the result must be JSON within a bounded size — each
  rejection naming its field.
- An appended replacement invalidates the repeat ledger, like every change that alters the
  conversation from outside the turn loop.

## Code-issue measurements at wrap

- `roster/conversation.rs` — 522 (+10, the replacement framing; the file's record and deferral are
  node 4's), `subagent.rs` — 2,001 (+16, the resume seam), `tddy-session-agents` `service.rs` —
  1,149 (+27, the parse composition). All open, unclaimed, inside the standing deferrals that
  schedule their split after the `subagent-control` stack lands.

## Backlog

No `docs/dev/todo/` entry resolved; the two consented deferral entries from nodes 3/4 stay, with
their reasons.
