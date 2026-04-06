/**
 * Pure helpers for multiple concurrent terminal attachments (Connection screen).
 */

import { parseTerminalSessionIdFromPathname } from "../../routing/appRoutes";

function logMultiSessionDebug(message: string, data: Record<string, unknown>): void {
  console.debug(`[tddy][multiSessionState] ${message}`, data);
}

export type LiveKitConnectionParams = {
  livekitUrl: string;
  roomName: string;
  identity: string;
  serverIdentity: string;
  debugLogging: boolean;
};

/** Map: sessionId → LiveKit params for that attachment (insertion order = connect order). */
export type SessionAttachmentMap = Map<string, LiveKitConnectionParams>;

/**
 * Adds or updates an attachment for `sessionId`, preserving all other sessions.
 */
export function addSessionAttachment(
  prev: SessionAttachmentMap,
  sessionId: string,
  params: LiveKitConnectionParams,
): SessionAttachmentMap {
  logMultiSessionDebug("addSessionAttachment", { sessionId, prevSize: prev.size });
  const next = new Map(prev);
  next.set(sessionId, params);
  return next;
}

export function removeSessionAttachment(prev: SessionAttachmentMap, sessionId: string): SessionAttachmentMap {
  logMultiSessionDebug("removeSessionAttachment", { sessionId, prevSize: prev.size });
  const next = new Map(prev);
  next.delete(sessionId);
  return next;
}

/** `data-testid` for the root wrapper of a mounted terminal attachment (Cypress). */
export function connectionAttachedTerminalTestId(sessionId: string): string {
  return `connection-attached-terminal-${sessionId}`;
}

/**
 * Focused session for routing: `/terminal/:id` selects that session when it is attached.
 * If the path does not match an attachment, falls back to the sole attachment (if any) or the first
 * connected session in map order when multiple are attached.
 */
export function focusedSessionIdFromPathname(
  pathname: string,
  attachments: SessionAttachmentMap,
): string | null {
  logMultiSessionDebug("focusedSessionIdFromPathname", {
    pathname,
    attachmentCount: attachments.size,
  });
  if (attachments.size === 0) {
    return null;
  }
  const pathId = parseTerminalSessionIdFromPathname(pathname);
  if (pathId && attachments.has(pathId)) {
    return pathId;
  }
  if (attachments.size === 1) {
    const onlyKey = attachments.keys().next().value;
    return onlyKey ?? null;
  }
  return attachments.keys().next().value ?? null;
}
