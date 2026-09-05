/**
 * Session-connection fixtures for component specs.
 *
 * A screen no longer takes a session's LiveKit coordinates and a status string — it takes the
 * `SessionConnection` its host opened, and asks that what it can do (`capabilities`) and how it is
 * doing (`status`). These builders state one of those in the terms a spec cares about: a session
 * carried over its own room, or one its host serves itself.
 *
 * Model: `src/rpc/connections/session.ts`.
 */

import { createClient, type Client, type Transport } from "@connectrpc/connect";
import type { DescService } from "@bufbuild/protobuf";
import type {
  SessionAttachmentHint,
  SessionConnection,
} from "../../../src/rpc/connections/session";
import type { TerminalFeed } from "../../../src/rpc/connections/terminal";
import type { ConnectionCapability, ConnectionStatus } from "../../../src/rpc/connections/types";

const A_HOST = "local";

/**
 * Builds one session connection. Defaults to a connected session its host serves itself — the
 * plainest thing an attachment can be — so a spec states only what it is actually about.
 */
export class SessionConnectionBuilder {
  private capabilities = new Set<ConnectionCapability>(["rpc"]);
  private status: ConnectionStatus = "connected";
  private failure: string | null = null;
  private transport: Transport | null = null;
  private hint: SessionAttachmentHint;

  constructor(private readonly sessionId: string) {
    this.hint = { sessionId };
  }

  /** A session published into `room` — tracks and a roster come with the wire that carries it. */
  carriedByRoom(
    room: string,
    where: { url: string; serverIdentity: string },
  ): SessionConnectionBuilder {
    this.capabilities = new Set<ConnectionCapability>(["rpc", "media", "presence"]);
    this.hint = { sessionId: this.sessionId, room, ...where };
    return this;
  }

  /** The connection's calls land on `transport` — normally an in-memory backend's. */
  servingOver(transport: Transport): SessionConnectionBuilder {
    this.transport = transport;
    return this;
  }

  /** The handshake has not landed yet. */
  stillConnecting(): SessionConnectionBuilder {
    this.status = "connecting";
    return this;
  }

  /** The connection failed, and says why. */
  failedWith(error: string): SessionConnectionBuilder {
    this.status = "error";
    this.failure = error;
    return this;
  }

  /** The routing this connection was opened with — what a screen's chat panel joins for itself. */
  buildHint(): SessionAttachmentHint {
    return this.hint;
  }

  build(): SessionConnection {
    const clients = new Map<DescService, Client<DescService>>();
    let live = true;
    const wire = () => {
      if (!this.transport) {
        throw new Error(
          `session ${this.sessionId} was built without \`servingOver(transport)\`, so it has ` +
            `nowhere to send this call — give the builder the backend's transport`,
        );
      }
      if (!live) throw new Error(`session ${this.sessionId} is closed`);
      return this.transport;
    };

    return {
      hostId: A_HOST,
      sessionId: this.sessionId,
      status: this.status,
      error: this.failure,
      capabilities: this.capabilities,
      clientFor: <S extends DescService>(service: S): Client<S> => {
        const cached = clients.get(service);
        if (cached) return cached as Client<S>;
        const built = createClient(service, wire());
        clients.set(service, built as Client<DescService>);
        return built;
      },
      transport: () => wire(),
      close: () => {
        live = false;
      },
      openTerminal: (): TerminalFeed => {
        // Deliberately not a silent empty feed. These builders state a session's status and what it
        // can do; none of them owns a PTY. A feed that accepted input and never produced a byte
        // would let a spec about terminal output pass while rendering nothing, so a spec that
        // reaches here is told to drive the terminal through a fixture that actually serves one.
        throw new Error(
          `session ${this.sessionId} was built by \`aSessionConnection\`, which serves session RPC ` +
            `and status only — it has no terminal to open`,
        );
      },
    };
  }
}

/** A session connection for `sessionId`. See {@link SessionConnectionBuilder} for the defaults. */
export function aSessionConnection(sessionId: string): SessionConnectionBuilder {
  return new SessionConnectionBuilder(sessionId);
}
