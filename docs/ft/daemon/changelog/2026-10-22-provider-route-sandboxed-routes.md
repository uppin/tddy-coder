# 2026-10-22 — Sandboxed sessions on a provider account; sandboxed OpenCode and Muse Code

A sandboxed **Claude CLI** session can now run on a provider account. Inside the jail, the account's
key reaches Claude Code through the jail's secret files, never its environment map or argv. The
host's OAuth token is not injected beside it. **OpenCode** and **Muse Code** sessions can be
sandboxed, and they start and resume in a jail. They need a provider account to do so (the jail
holds none of the person's own setup), and they take no first prompt. A resume relaunches the jail on
the recorded account. See [Claude Code CLI session](../claude-cli-session.md#running-on-a-provider-account),
[OpenCode CLI session § Sandbox mode](../opencode-cli-session.md#sandbox-mode) and
[Muse Code CLI session § Sandbox mode](../muse-cli-session.md#sandbox-mode).
