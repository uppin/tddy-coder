import { RestoredTabOutcome, type RestoredTab } from "tddy-client-web/gen/session_pb";

/**
 * What a Resume tells the operator about the tabs it brought back, or `null` when every tab came
 * back as it was. A tab reopened fresh or dropped is named with the daemon's reason, so a tab that is
 * not what it was never passes silently (`#session-liveness` 4/4).
 */
export function restoredTabsNotice(tabs: readonly RestoredTab[]): string | null {
  const lines = tabs
    .filter((tab) => tab.outcome === RestoredTabOutcome.FRESH || tab.outcome === RestoredTabOutcome.DROPPED)
    .map((tab) => {
      const what = tab.outcome === RestoredTabOutcome.FRESH ? "reopened fresh" : "could not be restored";
      return `Tab ${tab.terminalId} ${what}: ${tab.reason}`;
    });
  return lines.length === 0 ? null : lines.join(" ");
}
