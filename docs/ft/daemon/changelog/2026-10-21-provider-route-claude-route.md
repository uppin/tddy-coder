# 2026-10-21 — Claude Code CLI sessions on a provider account

A claude-cli session can run Claude Code on one of the operator's provider accounts: the session's
Claude Code talks to the account's Anthropic Messages endpoint with its key, every model alias is
pinned to the session's model, and the choice is recorded so a resume uses the same account. A
start or resume that cannot read the account — a locked vault, an account the vault does not hold,
an account that does not serve Anthropic Messages, no model — is refused, never run on Anthropic.
`ListProviderRoutes` tells a client which accounts a harness can run on and why not; Cursor never
can. Sandboxed sessions on an account are refused for now. See
[Claude Code CLI session § Running on a provider account](../claude-cli-session.md#running-on-a-provider-account)
and [provider accounts § Running a harness on an account](../provider-accounts.md#running-a-harness-on-an-account).
