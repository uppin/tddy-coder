# 2026-11-10 — Every session's token usage in the Inspector, with cache, reasoning and cost

`#usage-binding` 2/5 ([#891](https://github.com/uppin/tddy-coder/pull/891)).

- **Every session has a Usage tab with real numbers** — host terminal, jailed terminal and workflow
  sessions, live or ended. The daemon streams it; no LiveKit room is needed.
- **Jailed sessions are read from the jail's home**, not the daemon's or the user's `$HOME`.
- **New columns**: Cache read, Cache write, Reasoning and Cost beside In, Out, Total and Turns. Cost is
  blank when the agent reported none, and the TOTAL cost is marked partial (`*`) when only some
  conversations reported one.
- **Subagents nest under the conversation that spawned it**, at any depth.

See [Session Usage Inspector](../session-usage-inspector.md).
