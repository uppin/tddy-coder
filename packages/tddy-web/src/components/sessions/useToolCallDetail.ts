import { useEffect, useSyncExternalStore } from "react";
import type { Client } from "@connectrpc/connect";
import type { ConnectionService } from "../../gen/connection_pb";
import {
  agentActivityRegistry,
  type ToolCallBodyState,
} from "./agentActivityRegistry";

/**
 * Lazily fetches one tool call's body (`raw_input`/`raw_output`) for the detail dialog. The ACP
 * replay stream no longer inlines bodies (PR #345 strips them), so the body is pulled on demand via
 * `ConnectionService.GetAcpToolCallDetail` when a tool-call row is opened.
 *
 * Bodies are cached in the module-level {@link agentActivityRegistry}, keyed by `(sessionId, callId)`,
 * so re-opening the same row reads the cache instead of re-fetching. A failed lookup is stored as an
 * `error` state rather than cached permanently: because the effect refetches from `undefined` *or*
 * `error`, a later re-open retries. An in-flight (`loading`) or resolved (`loaded`) state is left
 * alone.
 *
 * PRD: docs/ft/web/agent-activity-pane.md § 4 Lazy tool bodies — fetch on click.
 */
export function useToolCallDetail(args: {
  sessionId: string;
  callId: string;
  sessionToken: string;
  client: Client<typeof ConnectionService>;
}): ToolCallBodyState {
  const { sessionId, callId, sessionToken, client } = args;

  const state = useSyncExternalStore(
    (listener) => agentActivityRegistry.subscribe(listener),
    () => agentActivityRegistry.getBody(sessionId, callId),
  );

  useEffect(() => {
    const current = agentActivityRegistry.getBody(sessionId, callId);
    // A body that is already in flight or resolved needs no fetch; only an unseen call (undefined)
    // or a previously-failed one (error, the retry case) triggers a lookup.
    if (current?.status === "loading" || current?.status === "loaded") return;

    let cancelled = false;
    agentActivityRegistry.setBody(sessionId, callId, { status: "loading" });
    (async () => {
      try {
        const resp = await client.getAcpToolCallDetail({
          sessionToken,
          sessionId,
          daemonInstanceId: "",
          toolCallId: callId,
        });
        if (cancelled) return;
        agentActivityRegistry.setBody(sessionId, callId, {
          status: "loaded",
          rawInput: resp.rawInput,
          rawOutput: resp.rawOutput,
        });
      } catch (err) {
        if (cancelled) return;
        // No fallback fabrication: a failed lookup surfaces as the error state (not permanently
        // cached — a later re-open retries).
        agentActivityRegistry.setBody(sessionId, callId, {
          status: "error",
          error: err instanceof Error ? err.message : String(err),
        });
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [client, sessionId, callId, sessionToken]);

  // Default to loading when nothing is cached yet: the effect has just started the fetch.
  return state ?? { status: "loading" };
}
