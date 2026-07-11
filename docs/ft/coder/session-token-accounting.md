# Session Token Accounting

**Product area:** coder / sandbox session
**Status:** WIP (changeset `docs/dev/1-WIP/2026-07-11-changeset-session-token-accounting.md`)

## Problem

A `tddy-sandbox-app` session runs several distinct conversations against language models:
the main `claude` agent, plus any number of **subagents** (`fastcontext` and other
YAML-defined specialized agents) — some of them backed by **local models via Ollama**.
Today the session keeps no account of how many tokens each of these conversations spends,
and there is no way to enumerate the conversations a session opened. Operators running a
session have no visibility into where the token budget went.

## Outcome

When a session ends, `tddy-sandbox-app` prints a per-conversation token summary to stderr:
one row per conversation (main agent + each subagent), showing the agent name, conversation
id, model, cumulative input/output/total tokens, and turn count, plus a session TOTAL row.
The same accounting is exposed at the RPC layer so it can be consumed programmatically.

## Requirements

1. **List all conversations.** A session can enumerate every conversation it opened,
   including subagents, via an RPC surface (a `subagent_list` MCP tool). Enumeration returns,
   per conversation: agent name, conversation id, model, input/output/total tokens, turns.

2. **Account subagent tokens.** Every subagent conversation accumulates token usage across
   all of its prompt turns, taken from the model's own `usage` report. This works uniformly
   for OpenAI-compatible endpoints and **Ollama** local models (both report `usage` on
   `/v1/chat/completions`). A model that omits `usage` counts as zero — it never fails a turn.

3. **Account the main agent.** The main `claude` agent's tokens are summed from its own
   transcript JSONL (the session's `--session-id` makes the transcript path deterministic).
   When no transcript exists, the main agent is reported with zero tokens and the model from
   the session's CLI args, never an error.

4. **End-of-session summary.** `tddy-sandbox-app` prints the per-conversation breakdown plus
   a TOTAL row when the session ends. Missing subagent accounting → subagents are omitted;
   the main agent is always attempted.

5. **Tokens only.** Accounting records tokens (input / output / total) and turn count. No
   monetary cost estimation in this cut.

## Scope

- **In scope:** RPC/backend layer + the `tddy-sandbox-app` CLI stderr summary.
- **Out of scope:** web dashboard / TUI visualization; USD cost estimation.

## Acceptance criteria

1. A session that opened two subagent conversations can list both, each with its exact
   cumulative input/output/total tokens and turn count.
2. `tddy-sandbox-app` renders a per-agent breakdown, including the main agent and a TOTAL row.
3. The main agent's tokens are summed exactly from a Claude transcript JSONL.
