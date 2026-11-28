# Coder-target sessions use the workspace's own `gh` login — SAML-unauthorized for wix-a / wix-private

**Found:** 2026-11-28, session `01a22204-3406-77b2-8845-60a2e59b0968` (coder target
`mantasi/remote-big`). Investigation only.

## Symptom

`gh` in the session: `GraphQL: Resource protected by organization SAML enforcement. You must grant
your OAuth token access to this organization` (wix-a; also blocks wix-private clones).

## Which credential the session actually gets

- **tddy injects no `GH_TOKEN` / `GITHUB_TOKEN` into agent sessions.** The token variables are
  actively stripped for user-run children (`packages/tddy-daemon-kernel/src/spawn_as_user.rs:28-33`,
  `:194-196`), the `coder ssh` client env carries only `CODER_URL`/`CODER_SESSION_TOKEN`
  (`packages/tddy-remote-shell/src/coder.rs:197`), and session identity is resolved per toolcall,
  never from env (`packages/tddy-agent-launch/src/session_acting_identity.rs:8-9`). A tddy-side
  GitHub token is only reachable on request through `GITHUB_TOKEN_METHOD`
  (`packages/tddy-host-service/src/host_session_service.rs:266-281`).
- So `gh` on the workspace uses **the workspace's own login**. The agent's own `gh auth status`
  (agent-activity.jsonl): `Logged in to github.com account mantasindrasius
  (/home/coder/.config/gh/hosts.yml)`, protocol ssh. That OAuth token has not been SSO-authorized
  for wix-a / wix-private.
- Separately, the daemon's vault account `uppin` is broken: daemon log 14:08:04 `session … starts
  without an account identity: the github account uppin has no subject_id recorded, and asking
  github who its token belongs to failed (… /user returned HTTP 401)`. Unrelated to the session's
  `gh`, but it means tddy could not supply a token even if it tried.

## Fix

- **Operator-side (now):** on `remote-big`, `gh auth refresh -h github.com` (or re-login) and
  authorize the token for wix-a and wix-private at the SSO prompt
  (https://github.com/orgs/wix-a/sso). Also re-link the `uppin` GitHub account in Tddy (401).
- **Product (optional, needs decision):** the session's acting identity already exists; a target
  session could be offered that token — e.g. a `gh` credential helper on the target that calls
  `GITHUB_TOKEN_METHOD` over the toolcall socket — instead of whichever login the workspace holds.
  It would still need SSO authorization of the OAuth App token for the orgs, and per
  `docs/ft/daemon/github-identity.md` env injection is deliberately avoided. At minimum, surface
  "gh on this target is logged in as X, not the session's identity Y" at session start.
