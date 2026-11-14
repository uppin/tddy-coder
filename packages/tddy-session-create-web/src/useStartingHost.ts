/**
 * The host a start runs on, and its label (`#orchestrators` 12/12, B12): the codebase host on a
 * split start, the agent's host otherwise. The steps of the start and the passphrase prompt are
 * the codebase host's on a split session, the session's own host's otherwise.
 */
export function useStartingHost(input: {
  isSplitCodebase: boolean;
  codebaseDaemonInstanceId: string;
  agentHostInstanceId: string;
  daemons: ReadonlyArray<{ instanceId: string; label?: string }>;
}): { startingHostId: string; startingHostLabel: string } {
  const startingHostId = input.isSplitCodebase
    ? input.codebaseDaemonInstanceId
    : input.agentHostInstanceId;
  const startingHostLabel =
    input.daemons.find((daemon) => daemon.instanceId === startingHostId)?.label ?? startingHostId;
  return { startingHostId, startingHostLabel };
}
