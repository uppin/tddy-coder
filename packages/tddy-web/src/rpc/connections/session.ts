/**
 * A session-bound connection, and the hint that opens one.
 *
 * Attaching to a session used to mean joining a **second** LiveKit room, minting an observer
 * identity, generating and refreshing a browser token, and addressing the session process at its own
 * participant identity `daemon-<instance>-<session>`. Where the attach reply named no room, none of
 * that happened and session RPC quietly fell back to the daemon's own client — a second, degraded
 * path (`connected-grpc`) that every consumer had to branch on.
 *
 * A `SessionConnection` is both of those, named once. What opening one costs — a room, a channel,
 * nothing at all — is the provider's business.
 *
 * Technical: `packages/tddy-web/docs/session-connections.md`.
 */

import type { Client, Transport } from "@connectrpc/connect";
import type { DescService } from "@bufbuild/protobuf";
import type { ConnectionCapability, ConnectionStatus } from "./types";
import type { TerminalFeed, TerminalOptions } from "./terminal";

/**
 * What the daemon's attach reply said about reaching this session, in transport-neutral terms.
 *
 * The reply is a proto message carrying LiveKit fields. Passing the hint whole to the provider keeps
 * the reading of those fields in exactly one place, instead of spreading `livekitRoom !== ""` across
 * the app the way `attachmentStateFromResponse` does today.
 *
 * A hint with no `room` is not an error and not a lesser session — it is a session on a host that
 * serves session RPC itself (`cli_session_manager.rs` hosts `terminal.TerminalService` against a PTY
 * handle), which is exactly what a desktop app over IPC will produce.
 */
export interface SessionAttachmentHint {
  readonly sessionId: string;

  /** The room the session publishes into, when it publishes into one. */
  readonly room?: string;

  /** Where that room lives. */
  readonly url?: string;

  /** The identity the session's own process serves RPC on, when it serves it separately. */
  readonly serverIdentity?: string;
}

/**
 * A live connection to one session on one host.
 *
 * Opened with `HostConnection.openSession`, and **closed by whoever opened it**. Unlike a host
 * connection, whose lifetime is the page's, a session connection ends when the session is detached —
 * which on a multi-connection transport means a real resource is released, not merely forgotten.
 */
export interface SessionConnection {
  readonly hostId: string;
  readonly sessionId: string;

  readonly status: ConnectionStatus;

  /** Why the connection is unusable, when {@link status} is `"error"`; `null` otherwise. */
  readonly error: string | null;

  /**
   * What this session connection can do — the media and presence surfaces are gated on it.
   *
   * A session carried over LiveKit has `{"rpc", "media", "presence"}`; one served by the host
   * daemon itself has `{"rpc"}`. Note this is the *session's* answer, not the host's: the same host
   * can serve a media-capable session and a plain one.
   */
  readonly capabilities: ReadonlySet<ConnectionCapability>;

  /**
   * A client for `service` on this session, memoised per connection.
   *
   * The identity guarantee `SessionClientCache` gives today, kept: the same instance while the
   * routing holds, a fresh one when it genuinely changes. `useAcpReplay` keys an effect on the
   * client and cancels an in-flight snapshot pull if it changes for no reason.
   */
  clientFor<S extends DescService>(service: S): Client<S>;

  transport(): Transport;

  /**
   * Release this connection.
   *
   * Idempotent. After it, `clientFor` and `transport` throw rather than returning something that
   * routes nowhere — a call issued on a detached session has no answer coming, and saying so beats
   * leaving it unsettled.
   */
  close(): void;

  /**
   * The terminal byte stream for this session, and its history fetcher where the transport can
   * serve one.
   *
   * Added by node 5 (`terminal-convergence`), which is what lets one terminal component be fed by
   * any wire. A connection that cannot serve history omits it and the terminal degrades to
   * live-tail — the LiveKit path's behaviour today, so nothing regresses.
   *
   * `options` carries the three things a connection provably cannot know — which terminal, whose
   * access token, and who holds the control lease — see {@link TerminalOptions}. Everything else
   * (the host, the session, the wire) the connection already is.
   */
  openTerminal(options: TerminalOptions): TerminalFeed;
}

/**
 * The one connected state a session attachment can be in.
 *
 * This replaces the pair `connected-livekit` / `connected-grpc`. Those forced every consumer to
 * re-derive "can I do X here" from "which wire am I on", and left one of the two paths with no
 * handshake overlay at all — `SessionRuntime.tsx:130` gates it on `connected-livekit`. Worse,
 * `SessionsDrawerScreen.tsx:399` had to fabricate a state carrying four empty LiveKit fields just to
 * satisfy the union.
 *
 * With one status, "which wire" stops being a question anyone asks: the connection's capabilities
 * answer what the two statuses were being used to approximate.
 */
export type SessionAttachmentState =
  | { readonly status: "idle" }
  | { readonly status: "connecting"; readonly sessionId: string }
  | { readonly status: "connected"; readonly connection: SessionConnection }
  | { readonly status: "error"; readonly error: string };
