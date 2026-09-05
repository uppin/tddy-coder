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
import type {
  TerminalFeed,
  TerminalFrame,
  TerminalOptions,
} from "../../../src/rpc/connections/terminal";
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
  private terminal: ControllableTerminal | null = null;

  /**
   * What every `openTerminal` on this connection was asked for.
   *
   * A pane states the three things a connection cannot know — which terminal, whose access token,
   * and who holds the control lease — so this is where a spec reads what the screen claimed on the
   * operator's behalf. Empty until {@link servingTerminal}.
   */
  readonly terminalOpens: TerminalOptions[] = [];

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

  /**
   * This connection serves a terminal, and the spec drives it.
   *
   * Without it `openTerminal` throws — see `build`. With it, the feed is a real one as far as the
   * component is concerned: bytes the spec delivers are rendered, and bytes the terminal sends are
   * readable.
   */
  servingTerminal(): SessionConnectionBuilder {
    this.terminal = aControllableTerminal();
    return this;
  }

  /** Push one live-tail frame of output at whatever the terminal is. */
  deliverToTerminal(text: string): void {
    if (!this.terminal) {
      throw new Error("this connection was not built with `servingTerminal()` — it has no terminal");
    }
    this.terminal.deliver(text);
  }

  /** Everything the terminal has typed back. */
  get typedIntoTerminal(): Uint8Array[] {
    return this.terminal?.sent ?? [];
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
      openTerminal: (options: TerminalOptions): TerminalFeed => {
        // Deliberately not a silent empty feed. These builders state a session's status and what it
        // can do; a session that was not given a terminal does not own a PTY. A feed that accepted
        // input and never produced a byte would let a spec about terminal output pass while
        // rendering nothing, so a spec that reaches here is told to ask for one.
        if (!this.terminal) {
          throw new Error(
            `session ${this.sessionId} was built by \`aSessionConnection\` without ` +
              `\`servingTerminal()\`, so it serves session RPC and status only — it has no ` +
              `terminal to open`,
          );
        }
        this.terminalOpens.push(options);
        return this.terminal.feed;
      },
    };
  }
}

/** A session connection for `sessionId`. See {@link SessionConnectionBuilder} for the defaults. */
export function aSessionConnection(sessionId: string): SessionConnectionBuilder {
  return new SessionConnectionBuilder(sessionId);
}

interface ControllableTerminal {
  readonly feed: TerminalFeed;
  readonly sent: Uint8Array[];
  deliver(text: string): void;
}

/** A terminal a spec pushes output into and reads input out of — no PTY, no wire. */
function aControllableTerminal(): ControllableTerminal {
  const listeners: Array<(frame: TerminalFrame) => void> = [];
  const sent: Uint8Array[] = [];
  return {
    feed: {
      stream: {
        send: (data) => sent.push(data),
        onMessage: (fn) => listeners.push(fn),
        close: () => {},
      },
    },
    sent,
    deliver: (text: string) => {
      const frame: TerminalFrame = {
        data: new TextEncoder().encode(text),
        endOffset: 0n,
        atOldest: false,
      };
      for (const fn of listeners) fn(frame);
    },
  };
}
