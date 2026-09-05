# 2026-08-29 — `subagent_await` travels with `subagent_prompt` in the Claude allowlist

**Type:** Feature

`SUBAGENT_TOOLS` gains `mcp__tddy-tools__subagent_await`, so a jailed agent handed a `responseId` by a deferred prompt can actually redeem it; offered and withheld under exactly the same condition as the prompt tool, since a receipt with no way to cash it is worse than no receipt. Feature [managed-codebase-subagents.md](../../../../docs/ft/coder/managed-codebase-subagents.md) § Long turns. Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-sandbox-recipes)
