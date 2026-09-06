# 2026-08-17 — `provisions_the_project_on_a_daemon_that_has_never_seen_it` — AC37 is not implemented — resolved 2026-08-18

**Category:** Known failing test
**Status:** Resolved
**Source:** session-agent-roster changeset, 2026-08-17

- AC37 is now implemented. The facilitating daemon serves `remote_git.RemoteGitService` on its
  common-room `daemon-{A}` participant; the owning daemon clones with
  `git clone {facilitating_instance_id}:{project_id}` using
  `GIT_SSH_COMMAND=tddy-remote-git-repo --daemon-url {facilitating_url} --session-token {token}`,
  the same transport `tddy-session-sync` uses. `provision_agent_clone` forwards the facilitating
  URL in `AgentClonePlacement.facilitating_daemon_url`; `start_hosted_agent_clone` sets it only
  for facilitator clones (shared-filesystem clones keep reading the local project repo). The
  acceptance test `provisions_the_project_on_a_daemon_that_has_never_seen_it` passes
  deterministically as part of the 20/20 `session_agent_remote_acceptance` suite.
- The same work closes the second gap it named: a clone's mirror fetches the WIP ref over the
  same `tddy-remote-git-repo` transport, so cross-host commit-following works without the two
  daemons already sharing the repository.
