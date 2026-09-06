/**
 * What a host has installed and configured: the git identity its commits would carry, and the state
 * of the GitHub CLI there.
 *
 * Six states must stay visually distinct, because each sends an operator somewhere different:
 *
 * | git                       | gh                          |
 * |---------------------------|-----------------------------|
 * | configured (name + email) | authenticated as `<login>`  |
 * | none configured           | installed but logged out    |
 * | probe failed              | not installed / probe failed |
 *
 * The `gh` login is **this host's**, not the tddy session's user in `UserAvatar` and not a
 * `GITHUB_TOKEN` in some environment. Three identities that can disagree, so the row labels which
 * one it is showing.
 */

import type { HostGitIdentity, HostGithubCli } from "../../gen/connection_pb";

export interface HostRowToolingProps {
  instanceId: string;
  git: HostGitIdentity | undefined;
  githubCli: HostGithubCli | undefined;
}

export function HostRowTooling({ instanceId, git, githubCli }: HostRowToolingProps) {
  // TODO(host-identity): implement
  void git;
  void githubCli;
  return <span data-testid={`hosts-row-${instanceId}-tooling`} />;
}
