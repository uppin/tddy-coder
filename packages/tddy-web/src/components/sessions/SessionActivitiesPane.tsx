import { useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import { ConnectionService } from "../../gen/connection_pb";
import { useHttpClient } from "../../rpc/transportProvider";
import { AgentChatView, TRANSCRIPT_ROOT_STYLE } from "../chat/AgentChat";
import type { ChatMessage } from "../chat/useAgentChat";
import { useAcpReplay } from "../chat/useAcpReplay";
import { AgentActivityDetailDialog } from "./AgentActivityDetailDialog";

interface SessionActivitiesPaneProps {
  sessionId: string;
  sessionToken: string;
  /** Explicit client override — session-scoped routing where available. Falls back to the shared
   *  HTTP client from the transport context. A dormant session has no LiveKit room of its own, so
   *  in practice this is the owning daemon's client and the replay comes off disk. */
  client?: Client<typeof ConnectionService>;
}

/**
 * The **Activities view**: an inactive session's main-pane surface, replacing the terminal it has no
 * live process for. It renders the same recorded ACP transcript as the top-bar Agent Activity
 * overlay — `StreamAcpReplay` frames projected into a read-only `AgentChatView`, tool entries opening
 * the shared detail dialog — but as the pane itself rather than as a popover.
 *
 * That difference is why the snapshot is pulled **eagerly** here: the overlay defers because the
 * operator may never open it, whereas this IS the view, so deferring would only add a blank frame.
 * Both surfaces share `useAcpReplay` and its registry cache, so a transcript read in one costs
 * nothing in the other.
 *
 * PRD: docs/ft/web/inactive-session-activities.md § Activities view.
 */
export function SessionActivitiesPane({
  sessionId,
  sessionToken,
  client,
}: SessionActivitiesPaneProps) {
  // `useHttpClient` is called unconditionally (hook rules); the explicit prop wins when present.
  const httpClient = useHttpClient(ConnectionService);
  const resolvedClient = client ?? httpClient;

  const chat = useAcpReplay({ sessionId, sessionToken, client: resolvedClient });
  const { hasActivity, countLoaded, unreadCount, markSeen, loadSnapshot } = chat;
  const { atOldest, loadingOlder, loadOlder } = chat;

  const [detail, setDetail] = useState<ChatMessage | null>(null);

  // Pull the transcript as soon as the view mounts, and keep the unread baseline level with what is
  // on screen — entries arriving while the operator is reading them are not unread, and the overlay
  // shares that baseline through the same registry.
  useEffect(() => {
    loadSnapshot();
    if (unreadCount > 0) markSeen();
  }, [unreadCount, loadSnapshot, markSeen]);

  // Switching to another session drops a detail dialog carried over from the previous one.
  useEffect(() => {
    setDetail(null);
  }, [sessionId]);

  return (
    <div
      data-testid="sessions-activities-pane"
      className="flex-1 min-h-0 flex flex-col relative overflow-hidden"
      // The transcript inside can only scroll if every element above it bounds its height, this pane
      // included — see `TRANSCRIPT_ROOT_STYLE` (PRD § Inline layout contract).
      style={TRANSCRIPT_ROOT_STYLE}
    >
      {/* "Recorded no activity" is a claim about the session, so it waits for the count feed to
          actually say so. Until then the pane shows nothing rather than a statement it cannot yet
          support — and a feed that fails never answers, so a failed read never turns into a false
          "nothing happened here" either (PRD § Edge cases). */}
      {hasActivity ? (
        // Keyed by session so the transcript's scroll state belongs to the session it was measured
        // against. The pane itself is not remounted on a switch, so without this a reader scrolled
        // up in one session would land in the next one still detached, have a phantom prepend
        // compensated against the previous transcript's height, and — since entry keys are scoped by
        // position, which two sessions can share — see a jump-to-latest count from the wrong
        // session. The cached transcript is held in the module-level registry, so re-keying re-reads
        // nothing.
        <AgentChatView
          key={sessionId}
          room={null}
          readOnly
          chat={chat}
          onToolClick={setDetail}
          onLoadOlder={loadOlder}
          hasOlder={!atOldest}
          loadingOlder={loadingOlder}
        />
      ) : countLoaded ? (
        <div
          data-testid="sessions-activities-empty"
          className="flex items-center justify-center h-full text-muted-foreground text-sm"
        >
          This session recorded no activity
        </div>
      ) : null}

      {detail && (
        <AgentActivityDetailDialog
          message={detail}
          sessionId={sessionId}
          sessionToken={sessionToken}
          client={resolvedClient}
          onClose={() => setDetail(null)}
        />
      )}
    </div>
  );
}
