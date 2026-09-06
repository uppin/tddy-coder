/**
 * The ssh-agent a host has, and the keys it is holding.
 *
 * Four states, kept distinct because each points somewhere different:
 *
 * | State | What an operator does about it |
 * |---|---|
 * | agent holding keys | nothing — this host can reach its remotes |
 * | agent, no keys | add a key (`#hosts-screen 6/8`) |
 * | no agent reachable | start an agent, or the daemon cannot see its socket |
 * | probe failed | look at the daemon log; we do not know |
 *
 * ⚠ A key's `comment` is free text set when the key was generated — commonly `user@host`. It is
 * **not** a file path, and the agent does not know which file a key came from, so the row must never
 * present it as a location.
 */

import type { HostSshAgent } from "../../gen/connection_pb";

export interface HostRowSshAgentProps {
  instanceId: string;
  sshAgent: HostSshAgent | undefined;
}

export function HostRowSshAgent({ instanceId, sshAgent }: HostRowSshAgentProps) {
  // TODO(agent-keys): implement
  void sshAgent;
  return <span data-testid={`hosts-row-${instanceId}-ssh-agent`} />;
}
