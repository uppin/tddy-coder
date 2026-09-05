/**
 * One session terminal, opened on the session's own connection.
 *
 * The pane states the three things a connection provably cannot know — which terminal, whose access
 * token, and who holds the control lease (see `TerminalOptions`) — and the connection answers with
 * a feed. Everything else about how those bytes travel is the connection's business, which is what
 * lets one terminal component render a session carried over a room and a session its host serves
 * itself without knowing which it is looking at.
 *
 * PRD: `docs/dev/1-WIP/2026-09-05-optional-livekit-terminal-convergence-prd.md`.
 */

import { useEffect, useRef, useState, type RefObject } from "react";
import { measureTerminalGridFromRect } from "../../lib/terminalGridMeasure";
import type { SessionConnection } from "../../rpc/connections/session";
import type { TerminalFeed } from "../../rpc/connections/terminal";

export interface SessionTerminalFeedDeps {
  /** The session's connection, or `null` while there is none to open a terminal on. */
  readonly connection: SessionConnection | null;

  /** The operator's access token, which the daemon resolves before serving any terminal RPC. */
  readonly sessionToken: string;

  /** Which of the session's terminals this pane shows. Empty (the default) is the Agent terminal. */
  readonly terminalId?: string;

  /**
   * The control lease held by this screen, read **per send**: a second screen claiming the terminal
   * replaces it, and a token snapshotted when the feed opened goes stale the moment control changes
   * hands. Held in a ref so a re-claim does not reopen the stream.
   */
  readonly controlToken: () => string;

  /**
   * The pane the terminal renders into, measured once at open time so the daemon can resize the PTY
   * *before* it replays. Omitted, or not laid out yet, means no grid is stated — which is honest,
   * and better than stating a size measured off nothing.
   */
  readonly containerRef?: RefObject<HTMLElement | null>;
}

/**
 * Open `terminalId` on `connection`, and close it when the pane goes away or the connection is
 * replaced.
 *
 * `null` until there is a connection: a terminal cannot be opened on a session that is not attached,
 * and rendering one against a feed that is not there is what the `stream &&` guards in the previous
 * terminals were for.
 */
export function useSessionTerminalFeed({
  connection,
  sessionToken,
  terminalId = "",
  controlToken,
  containerRef,
}: SessionTerminalFeedDeps): TerminalFeed | null {
  const [feed, setFeed] = useState<TerminalFeed | null>(null);
  const controlTokenRef = useRef(controlToken);
  controlTokenRef.current = controlToken;
  const containerRefHolder = useRef(containerRef);
  containerRefHolder.current = containerRef;

  useEffect(() => {
    if (!connection) {
      setFeed(null);
      return;
    }
    const grid = measureTerminalGridFromRect(
      containerRefHolder.current?.current?.getBoundingClientRect(),
    );
    const opened = connection.openTerminal({
      terminalId,
      sessionToken,
      controlToken: () => controlTokenRef.current(),
      ...(grid.cols > 0 && grid.rows > 0 ? { initialGrid: { cols: grid.cols, rows: grid.rows } } : {}),
    });
    setFeed(opened);
    return () => {
      opened.stream.close();
      setFeed(null);
    };
  }, [connection, sessionToken, terminalId]);

  return feed;
}
