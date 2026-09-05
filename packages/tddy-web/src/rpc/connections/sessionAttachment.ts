/**
 * Reading a daemon's attach reply into a transport-neutral hint.
 *
 * `attachmentStateFromResponse` in `components/sessions/useSessionAttachment.ts` does this today by
 * branching on `resp.livekitRoom !== ""` and producing one of two statuses. That branch is the
 * reason every downstream consumer has to know which wire it is on. Here it becomes one pure
 * function producing one hint, and the *provider* decides what the hint costs to honour.
 *
 * Pure, so it is testable without a rendered screen — the same reason `routing/selectedHost.ts`
 * holds the selection rules.
 */

import type { SessionAttachmentHint } from "./session";

/** The fields of `ConnectSessionResponse` / `ResumeSessionResponse` this reads. */
export interface AttachReply {
  readonly livekitRoom: string;
  readonly livekitUrl: string;
  readonly livekitServerIdentity: string;
}

/**
 * The hint for `sessionId`, from what the daemon replied.
 *
 * An empty `livekitRoom` yields a hint with no `room` — not an error and not a lesser session. It
 * means the host serves this session's RPC itself, which is what a desktop app over IPC produces and
 * what `cli_session_manager.rs` already does for a CLI-managed session.
 *
 * Blank-but-present fields are dropped rather than carried as empty strings, so a consumer cannot
 * accidentally treat `""` as a room name — the bug shape that made
 * `SessionsDrawerScreen.tsx:399` fabricate four empty LiveKit fields to satisfy the old union.
 */
export function attachmentHintFromReply(
  sessionId: string,
  reply: AttachReply,
): SessionAttachmentHint {
  // TODO(session-connection): implement
  throw new Error(`attachmentHintFromReply(${sessionId}) is not implemented yet`);
}

/**
 * The capabilities a session reached through `hint` will have.
 *
 * A hint naming a room is carried over LiveKit and can serve tracks and a participant roster; one
 * that does not is plain RPC. This is the function node 4's media and presence gating ultimately
 * reads through, so it is stated once, here, rather than re-derived per surface.
 */
export function capabilitiesForHint(hint: SessionAttachmentHint): ReadonlySet<string> {
  // TODO(session-connection): implement
  throw new Error(`capabilitiesForHint(${hint.sessionId}) is not implemented yet`);
}
