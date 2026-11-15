# 2026-11-15 — tddy-daemon-rpc: answering an agent that takes no structured answer is refused

`AnswerSessionQuestion` on a target whose agent declares no `ask_user_question` is refused before the orchestrator and fit checks, naming the product and pointing at `ClaimSessionTerminal` + `SendSessionKeys`. Sessions no agent declares fall through to the existing checks. PR [#911](https://github.com/uppin/tddy-coder/pull/911). See [2026-11-15-muse-hooks-wired](../../../../docs/dev/changesets/2026-11-15-muse-hooks-wired.md).
