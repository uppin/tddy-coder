/**
 * A session connection for a host that answers its own session RPC.
 *
 * This is the case the attach reply names no room for. Today it becomes `connected-grpc` and every
 * consumer branches on it: `SessionRuntime.tsx:176` quietly swaps in the *daemon* client, and the
 * handshake overlay never appears because it is gated on `connected-livekit`. So the configuration
 * that works — `cli_session_manager.rs` serves `terminal.TerminalService` against a PTY handle —
 * reads to the operator as the one that never connected.
 *
 * Here it is an ordinary session connection that happens to route over the host's own transport. It
 * is not LiveKit-shaped and it is not provider-shaped either: any wire whose host already carries
 * session RPC opens one of these, which is why it sits beside the model rather than under
 * `livekit/`.
 */

import type { Client, Transport } from "@connectrpc/connect";
import type { DescService } from "@bufbuild/protobuf";
import { ConnectionService } from "../../gen/connection_pb";
import { capabilitiesForHint } from "./sessionAttachment";
import type { SessionAttachmentHint, SessionConnection } from "./session";
import type { TerminalFeed, TerminalOptions } from "./terminal";
import { openDaemonTerminalFeed, TerminalResumePoint } from "./terminalFeed";
import type { ConnectionStatus, HostConnection } from "./types";

/**
 * `sessionId` on `host`, reached over the host connection itself.
 *
 * Clients and transport are the host's, so identity is stable for exactly as long as the host
 * connection is — the same guarantee `HostConnection.clientFor` already makes, inherited rather than
 * re-implemented. `status` is likewise read through: a session cannot be more reachable than the
 * host serving it.
 *
 * `close()` releases nothing, because nothing was acquired: the host connection outlives this and is
 * shared with every other session on it. It still latches, so a call issued after a session is
 * detached throws instead of quietly succeeding against a host the caller has stopped watching, and
 * the status falls back to `idle` — nothing is being asked of this connection any more, which is a
 * different claim from the host having failed.
 */
export function openHostServedSession(
  host: HostConnection,
  hint: SessionAttachmentHint,
): SessionConnection {
  let live = true;
  const refuseIfClosed = () => {
    if (!live) {
      throw new Error(`session ${hint.sessionId} on host ${host.hostId} is closed`);
    }
  };

  // One resume point per terminal, for the life of this connection: a session has several terminals
  // and each is at its own offset, so re-opening one must not resume it from another's.
  const resumePoints = new Map<string, TerminalResumePoint>();
  const resumePointFor = (terminalId: string): TerminalResumePoint => {
    const existing = resumePoints.get(terminalId);
    if (existing) return existing;
    const fresh = new TerminalResumePoint();
    resumePoints.set(terminalId, fresh);
    return fresh;
  };

  return {
    hostId: host.hostId,
    sessionId: hint.sessionId,
    get status(): ConnectionStatus {
      return live ? host.status : "idle";
    },
    get error(): string | null {
      return live ? host.error : null;
    },
    capabilities: capabilitiesForHint(hint),
    clientFor<S extends DescService>(service: S): Client<S> {
      refuseIfClosed();
      return host.clientFor(service);
    },
    transport(): Transport {
      refuseIfClosed();
      return host.transport();
    },
    close(): void {
      live = false;
    },
    /**
     * The terminal over this host's own `ConnectionService` — the wire this session's RPC already
     * travels, and the one holding its capture ring, so scrollback comes with it.
     */
    openTerminal(options: TerminalOptions): TerminalFeed {
      refuseIfClosed();
      return openDaemonTerminalFeed({
        client: host.clientFor(ConnectionService),
        sessionId: hint.sessionId,
        resume: resumePointFor(options.terminalId ?? ""),
        options,
      });
    },
  };
}
