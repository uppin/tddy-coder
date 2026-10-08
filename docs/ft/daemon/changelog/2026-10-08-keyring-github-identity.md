# 2026-10-08 — Sessions act as the account their project is assigned

- **Commits and GitHub calls use the project's account.** A session's commits are authored by that account's
  GitHub login and noreply address, and the agent's GitHub tools use its token. Two projects on one daemon can
  act as two different GitHub users.
- **No environment token.** `GITHUB_TOKEN` and `GH_TOKEN` are never used, in the daemon or in a session.
- **Refusals say why.** Not assigned, not held on this host, ambiguous, unusable, or a locked vault each have
  their own message. The session still starts, under the checkout's identity, and no token is delivered.
- **Chosen once.** The account is fixed when a session starts or resumes; a reassignment applies on the next
  resume. A session started refused stays refused until resumed.
- **Telegram-started sessions** cannot act as the account; the chat is told.

See [Per-project GitHub identity](../github-identity.md).
