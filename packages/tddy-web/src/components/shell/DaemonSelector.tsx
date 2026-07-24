/**
 * Top-right daemon selector (shadcn `Select`). Lists the common-room daemon-role participants and
 * re-targets daemon-level RPC (`useDaemonClient`) at the selected one.
 *
 * PRD: docs/ft/web/daemon-selector-livekit-rpc.md.
 */

import type { DaemonHost } from "../../lib/participantRole";
import { useSelectedDaemon } from "../../rpc/selectedDaemon";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../ui/select";

const SELF_LABEL_SUFFIX = " (this daemon)";

/**
 * Every daemon's own advertisement self-labels itself "{id} (this daemon)" from its own
 * perspective (see the PRD's "daemon identity subtlety") — that suffix only means something for
 * the entry matching `servingInstanceId`; strip it from every other daemon's label.
 */
function displayLabel(daemon: DaemonHost, servingInstanceId?: string): string {
  if (daemon.instanceId === servingInstanceId) return daemon.label;
  return daemon.label.endsWith(SELF_LABEL_SUFFIX)
    ? daemon.label.slice(0, -SELF_LABEL_SUFFIX.length)
    : daemon.label;
}

export function DaemonSelector({
  daemons,
  selectedInstanceId,
  servingInstanceId,
  onSelect,
}: {
  daemons: DaemonHost[];
  selectedInstanceId: string | null;
  servingInstanceId?: string;
  onSelect: (instanceId: string) => void;
}) {
  return (
    <Select
      value={selectedInstanceId ?? undefined}
      onValueChange={onSelect}
      disabled={daemons.length === 0}
    >
      <SelectTrigger
        data-testid="daemon-selector-trigger"
        className="h-7 gap-1 px-2 text-xs [&_svg:not([class*='size-'])]:size-3.5"
      >
        <SelectValue placeholder="Select daemon" />
      </SelectTrigger>
      <SelectContent>
        {daemons.map((daemon) => (
          <SelectItem
            key={daemon.instanceId}
            value={daemon.instanceId}
            data-testid={`daemon-selector-option-${daemon.instanceId}`}
          >
            {displayLabel(daemon, servingInstanceId)}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

/** Connected wrapper reading the shared `SelectedDaemonProvider` context — what screens render. */
export function DaemonSelectorConnected() {
  const { daemons, selectedInstanceId, servingInstanceId, selectDaemon } = useSelectedDaemon();
  return (
    <DaemonSelector
      daemons={daemons}
      selectedInstanceId={selectedInstanceId}
      servingInstanceId={servingInstanceId}
      onSelect={selectDaemon}
    />
  );
}
