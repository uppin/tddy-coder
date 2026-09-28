# 2026-09-28 — Assistants on the Models screen carry usage notes for their operator

PRD: the `agent-usage-notes` node of the `subagent-control` stack
([#555](https://github.com/uppin/tddy-coder/pull/555)).

An assistant can now carry **usage notes** — how to use it, its quirks, its budgets, what not to
ask it. The create dialog has a multi-line field for them, the edit dialog pre-fills what is
stored and carries the whole replacement on update (the same rule `replaces` follows), and the
assistant's row on the panel shows its note — truncated, full text on hover — so an operator
choosing an agent reads how to use it before prompting it.

Usage notes are **operator documentation, never machine context**: they are stored with the
assistant and shown to humans, and are deliberately not injected into any system prompt and not
advertised to a main agent at session open.
