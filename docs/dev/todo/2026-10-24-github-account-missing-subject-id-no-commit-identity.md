# 2026-10-24 — A session starts with no commit identity when its GitHub account has no `subject_id`

**Category:** Gap (low)
**Source:** 2026-10-24-project-home-agent-tools (seen while diagnosing main session `01a16dc6-…` on Tddy Desktop)

- Daemon log at the main's start:
  `session 01a16dc6-… starts without an account identity: the github account uppin has no
  \`subject_id\` recorded, so there is no identity to commit under; link the account again to refresh it`
  (`tddy_daemon::connection_service`).
- The start proceeds and the agent commits under the checkout's own identity. The only signal is a
  `WARN` in the daemon log; the operator is not told in the UI, and the remedy ("link the account
  again") is not offered anywhere they would see it.
- **Why deferred:** unrelated to the main's tool wiring this changeset fixes; it affects every
  session whose account row predates `subject_id`.
- **What would close it:** backfill `subject_id` on the next successful token use for the account
  (the GitHub user id is available from any authenticated call), or surface the missing identity on
  the account card with a re-link action. Test: an account row without `subject_id` → after one
  authenticated use the row carries it, and the next session starts with commit pairs.
