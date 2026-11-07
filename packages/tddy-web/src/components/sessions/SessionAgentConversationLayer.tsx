import React, { useCallback, useLayoutEffect, useRef, type RefObject } from "react";
import { createPortal } from "react-dom";
import { safeTestIdPart } from "tddy-client-web";
import type { AgentConversation } from "./agentConversationTabs";
import { SessionAgentConversationPane } from "./SessionAgentConversationPane";
import type { SessionAgentClient } from "./sessionRuntimeProps";

/**
 * Where the bodies of agent conversations live.
 *
 * A conversation body cancels its conversation on the daemon when it unmounts, and only closing the
 * tab may end one. A body that lived inside a tab strip's pane stack would end with the strip: a
 * stopped session's strip is replaced by the live one on Resume, and every strip is replaced as the
 * selection moves. So the bodies are owned by neither strip. They are rendered by one layer — one
 * per screen — that outlives them, and each body is portalled into a **host element** the layer
 * keeps per session. The strip showing a session adopts that session's host into its pane stack
 * ({@link useConversationHostAttachment}); a strip that goes away leaves the host, and the bodies
 * in it, untouched.
 *
 * Dev doc: packages/tddy-web/docs/session-agent-conversation.md § 1
 */

/** The per-session host elements, created on first ask and kept for the life of the layer's owner. */
export function useConversationHosts(): (sessionId: string) => HTMLDivElement {
  const hosts = useRef(new Map<string, HTMLDivElement>());
  return useCallback((sessionId: string) => {
    let host = hosts.current.get(sessionId);
    if (host === undefined) {
      host = document.createElement("div");
      host.setAttribute("data-testid", `sessions-agent-conversations-${safeTestIdPart(sessionId)}`);
      // Stated inline for the reason the panes' own stacking is: the terminal beside it claims
      // `z-index: 2` inline. Above the terminal-control overlay (3) as well — that overlay is about
      // typing into the *terminal*, and a conversation is not it. The host itself takes no pointer
      // events, so while no body is showing the terminals below stay clickable.
      Object.assign(host.style, {
        position: "absolute",
        inset: "0",
        zIndex: "4",
        pointerEvents: "none",
      });
      hosts.current.set(sessionId, host);
    }
    return host;
  }, []);
}

/**
 * Put `host` inside the pane stack while `hosts` holds. A strip that is not the one showing its
 * session passes `hosts = false` and never takes the element; one that unmounts only removes it
 * when it is still the parent, so a successor that already adopted it is not undone.
 */
export function useConversationHostAttachment(
  paneStackRef: RefObject<HTMLElement | null>,
  host: HTMLElement | undefined,
  hosts: boolean,
): void {
  useLayoutEffect(() => {
    const stack = paneStackRef.current;
    if (!hosts || host === undefined || stack === null) return;
    stack.appendChild(host);
    return () => {
      if (host.parentNode === stack) stack.removeChild(host);
    };
  }, [paneStackRef, host, hosts]);
}

export interface SessionAgentConversationLayerProps {
  /** Every session's open conversations. */
  agentConversations: Readonly<Record<string, readonly AgentConversation[]>>;
  /** Each session's focused conversation, when one holds its pane. */
  activeConversations: Readonly<Record<string, string | null>>;
  hostFor: (sessionId: string) => HTMLDivElement;
  sessionToken: string;
  callerLogin: string | null;
  client?: SessionAgentClient | null;
}

/** Renders no DOM of its own: every body is portalled into its session's host. */
export function SessionAgentConversationLayer({
  agentConversations,
  activeConversations,
  hostFor,
  sessionToken,
  callerLogin,
  client,
}: SessionAgentConversationLayerProps): React.ReactElement {
  return (
    <>
      {Object.entries(agentConversations).flatMap(([sessionId, conversations]) =>
        conversations.map((conversation) =>
          createPortal(
            <div
              data-testid={`sessions-agent-pane-${safeTestIdPart(conversation.conversationId)}`}
              className="bg-background"
              style={{
                position: "absolute",
                inset: 0,
                pointerEvents: "auto",
                display:
                  activeConversations[sessionId] === conversation.conversationId
                    ? "block"
                    : "none",
              }}
            >
              <SessionAgentConversationPane
                sessionId={sessionId}
                sessionToken={sessionToken}
                daemonInstanceId={conversation.daemonInstanceId}
                agentId={conversation.agentId}
                conversationId={conversation.conversationId}
                callerLogin={callerLogin}
                client={client ?? undefined}
              />
            </div>,
            hostFor(sessionId),
            `${sessionId}/${conversation.conversationId}`,
          ),
        ),
      )}
    </>
  );
}
