import { useEffect, useMemo, useState } from "react";
import { Code, ConnectError, type Client } from "@connectrpc/connect";
import type { ConnectionService } from "../../gen/connection_pb";
import { agentActivityRegistry } from "../sessions/agentActivityRegistry";

/** Why a body lookup could not be answered. The hosts reserve `NOT_FOUND` for "no such
 *  `tool_call_id` in this session's transcript" and report everything else (host unreachable,
 *  transport failure, internal error) with another code, so the two are kept apart here instead of
 *  collapsing into one opaque failure. `missingId` is answered without asking anyone: a transcript
 *  entry that carries no `tool_call_id` has no addressable bodies at all, which is a different fact
 *  from a lookup that was attempted and failed — and worth stating as such rather than dressing up as
 *  the host's `NOT_FOUND`. */
export type ToolCallDetailErrorKind = "missingId" | "notFound" | "failed";

/** The state of one tool call's body lookup: in flight, resolved (either body may legitimately be
 *  absent — a still-running call has no output yet), or failed. */
export type AcpToolCallDetailState =
  | { readonly status: "loading" }
  | { readonly status: "loaded"; readonly rawInput?: string; readonly rawOutput?: string }
  | { readonly status: "error"; readonly kind: ToolCallDetailErrorKind };

/** A fetched resolution, tagged with the lookup it answers, so a hook whose `toolCallId` (or
 *  `sessionId`) changed reads as loading again from the very first render rather than briefly showing
 *  the previous call's bodies. */
interface Resolution {
  readonly forSessionId: string;
  readonly forToolCallId: string;
  readonly state: AcpToolCallDetailState;
}

const LOADING: AcpToolCallDetailState = { status: "loading" };
const MISSING_ID: AcpToolCallDetailState = { status: "error", kind: "missingId" };

/**
 * Resolves one tool call's `raw_input`/`raw_output` through the unary
 * `ConnectionService.GetAcpToolCallDetail`, on open.
 *
 * `StreamAcpReplay` strips both bodies out of every streamed frame (so the transcript's size tracks
 * the *number* of tool calls, not the volume of their I/O), which leaves the transcript entry with
 * nothing but metadata and its `tool_call_id`. This hook turns that id back into bodies:
 *
 * - an **empty** `toolCallId` — a transcript entry that never carried one — resolves to `missingId`
 *   with **no request**;
 * - a body already cached for this `(sessionId, toolCallId)` resolves with **no request**;
 * - otherwise one unary is issued, and its result is cached — but only when `cacheable`.
 *
 * `cacheable` is false for a call that is still running: its output can still arrive, so caching the
 * partial body would keep the dialog stale for the rest of the session. Such a call is re-fetched on
 * every open, cache read included, so the exclusion holds regardless of what an earlier open wrote.
 *
 * Feature doc: docs/ft/web/agent-activity-pane.md#rendering-an-unanswered-lookup-updated-2026-07-26
 */
export function useAcpToolCallDetail(args: {
  sessionId: string;
  sessionToken: string;
  client: Client<typeof ConnectionService>;
  /** The ACP `tool_call_id` of the call whose bodies to fetch (`ChatMessage.toolCallId`). Empty for a
   *  transcript entry that carried no id: nothing is requested and the state is `missingId`. */
  toolCallId: string;
  /** Whether the resolved bodies are final and may be cached for the session. */
  cacheable: boolean;
}): AcpToolCallDetailState {
  const { sessionId, sessionToken, client, toolCallId, cacheable } = args;
  const [resolution, setResolution] = useState<Resolution | null>(null);

  /**
   * The answer that needs no request, resolved **during render**: an absent id, or a cache hit for
   * this `(sessionId, toolCallId)`. `null` means "must be fetched".
   *
   * Why a render-time `useMemo` and not the effect that used to hold this read: an effect runs after
   * the render commits, so a cached body read there is preceded by one painted frame of `loading` —
   * the skeleton flash on every reopen, which is precisely what caching the bodies is meant to remove.
   * `useMemo` reads the store without updating state during render, and being keyed on the lookup's
   * identity it recomputes the moment `sessionId`, `toolCallId`, or `cacheable` change, so a dialog
   * that stays mounted while the operator switches rows cannot keep the previous row's answer. (A
   * `useState` initializer was rejected for exactly that: it is evaluated once per mount, and this
   * hook outlives a row switch.)
   *
   * The same value gates the effect below, so "rendered from cache" and "issued no request" are one
   * decision rather than two that can disagree.
   */
  const withoutRequest = useMemo<AcpToolCallDetailState | null>(() => {
    if (!toolCallId) return MISSING_ID;
    // `cacheable` gates the cache *read* as much as the write: a running call must re-fetch even if an
    // earlier open of the same row cached its then-partial bodies.
    const cached = cacheable
      ? agentActivityRegistry.get(sessionId)?.toolDetails.get(toolCallId)
      : undefined;
    return cached
      ? { status: "loaded", rawInput: cached.rawInput, rawOutput: cached.rawOutput }
      : null;
  }, [cacheable, sessionId, toolCallId]);

  useEffect(() => {
    if (withoutRequest) return;

    // A dialog closed (or switched to another row) while the lookup is in flight must neither render
    // nor cache what arrives late.
    let cancelled = false;
    (async () => {
      try {
        const detail = await client.getAcpToolCallDetail({
          sessionToken,
          sessionId,
          // An empty *daemon instance* id means "serve locally", the same routing the replay stream
          // requests use.
          daemonInstanceId: "",
          toolCallId,
        });
        if (cancelled) return;
        setResolution({
          forSessionId: sessionId,
          forToolCallId: toolCallId,
          state: { status: "loaded", rawInput: detail.rawInput, rawOutput: detail.rawOutput },
        });
        if (cacheable) {
          agentActivityRegistry.setToolDetail(sessionId, toolCallId, {
            rawInput: detail.rawInput,
            rawOutput: detail.rawOutput,
          });
        }
      } catch (err) {
        if (cancelled) return;
        // The failure is reported as-is — no body is fabricated and nothing is cached, so closing and
        // reopening the row retries the lookup.
        const kind: ToolCallDetailErrorKind =
          ConnectError.from(err).code === Code.NotFound ? "notFound" : "failed";
        setResolution({
          forSessionId: sessionId,
          forToolCallId: toolCallId,
          state: { status: "error", kind },
        });
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [cacheable, client, sessionId, sessionToken, toolCallId, withoutRequest]);

  if (withoutRequest) return withoutRequest;
  // A resolution left over from another lookup reads as `loading`, not as that lookup's bodies.
  return resolution?.forSessionId === sessionId && resolution.forToolCallId === toolCallId
    ? resolution.state
    : LOADING;
}
