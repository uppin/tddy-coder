/**
 * Top-right daemon selector (shadcn `Select`). Lists the common-room daemon-role participants and
 * re-targets daemon-level RPC (`useDaemonClient`) at the selected one.
 *
 * A selected daemon that is not listed right now is shown as *waiting* rather than replaced: under
 * the label it was last listed with, beside a pulsing amber dot.
 *
 * PRDs: docs/ft/web/daemon-selector-livekit-rpc.md; docs/ft/web/url-state-routing.md § Host in the URL.
 */

import { SELF_LABEL_SUFFIX, type DaemonHost } from "../../lib/participantRole";
import { findListedDaemon } from "../../routing/selectedHost";
import { useHostDirectorySource } from "../../rpc/hostDirectory/useHostDirectory";
import { LIVEKIT_SOURCE_ID } from "../../rpc/hostDirectory/liveKitSource";
import { useSelectedDaemon } from "../../rpc/selectedDaemon";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../ui/select";

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

/** Accessible name and tooltip of the dot marking a selected daemon that is not listed right now. */
const WAITING_FOR_DAEMON = "Waiting for daemon";

/** The pulsing amber dot beside a waiting daemon's label. No text: the trigger's text is the label. */
function WaitingDot() {
  return (
    <span
      data-testid="daemon-selector-waiting"
      title={WAITING_FOR_DAEMON}
      aria-label={WAITING_FOR_DAEMON}
      role="img"
      className="size-2 shrink-0 animate-pulse rounded-full bg-amber-500"
    />
  );
}

interface DaemonSelectorProps {
  daemons: DaemonHost[];
  selectedInstanceId: string | null;
  servingInstanceId?: string;
  onSelect: (instanceId: string) => void;
  /**
   * The daemon list is empty because the common room could not be joined, not because no daemon is
   * running. Both look identical from `daemons` alone, and the plain "Select daemon" placeholder
   * reads as "pick one" — an invitation the operator cannot act on and that hides the real fault.
   */
  commonRoomUnreachable?: boolean;
  /**
   * The label the selected daemon is known by (`useSelectedDaemon().selectedLabel`). Names the
   * selected daemon while it is waiting — selected but not in `daemons`. `null` falls back to its
   * instance id: a host this tab has never listed has no other name.
   */
  selectedLabel?: string | null;
}

export function DaemonSelector({
  daemons,
  selectedInstanceId,
  servingInstanceId,
  onSelect,
  commonRoomUnreachable = false,
  selectedLabel = null,
}: DaemonSelectorProps) {
  // A selection is kept while its daemon is not listed (see `resolveSelectedDaemonInstanceId`). The
  // Radix trigger can only show the label of an item that exists, so the waiting daemon gets one —
  // disabled, since choosing a daemon that cannot be reached right now does nothing useful.
  const waitingHost: DaemonHost | null =
    selectedInstanceId && !findListedDaemon(daemons, selectedInstanceId)
      ? { instanceId: selectedInstanceId, label: selectedLabel ?? selectedInstanceId }
      : null;
  const options = waitingHost ? [...daemons, waitingHost] : daemons;
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
        {waitingHost && <WaitingDot />}
        <SelectValue
          placeholder={commonRoomUnreachable ? "Common room unreachable" : "Select daemon"}
        />
      </SelectTrigger>
      <SelectContent>
        {options.map((daemon) => (
          <SelectItem
            key={daemon.instanceId}
            value={daemon.instanceId}
            data-testid={`daemon-selector-option-${daemon.instanceId}`}
            disabled={daemon === waitingHost}
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
  const { daemons, selectedInstanceId, selectedLabel, servingInstanceId, selectDaemon } =
    useSelectedDaemon();
  // The common room's own status, not the directory's. The placeholder names the common room, and
  // the merged status cannot answer for it: it is optimistic, so a page that knows its own serving
  // daemon reads `connected` however badly the room is doing. Reading it here would make the
  // message unreachable wherever a serving daemon is named — which is every production page.
  const commonRoom = useHostDirectorySource(LIVEKIT_SOURCE_ID);
  return (
    <DaemonSelector
      daemons={daemons}
      selectedInstanceId={selectedInstanceId}
      servingInstanceId={servingInstanceId}
      onSelect={selectDaemon}
      selectedLabel={selectedLabel}
      commonRoomUnreachable={commonRoom?.status === "error"}
    />
  );
}
