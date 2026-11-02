import React, { useEffect, useRef, useState } from "react";
import { Code, ConnectError } from "@connectrpc/connect";
import { GhosttyTerminal, type GhosttyTerminalHandle } from "../GhosttyTerminal";
import { HISTORY_FILTER, HISTORY_THEME } from "../GhosttyTerminalSession";
import {
  createForwardHistoryFetcher,
  TerminalHistoryForwardLoader,
} from "../../lib/terminalHistoryLoader";
import { rpcFailureText } from "../../rpc/rpcFailure";
import { useIsMobile } from "../../hooks/useIsMobile";
import { Toast } from "../ui/toast";
import { InlineError } from "../ui/inline-error";
import type { TerminalClient } from "./sessionRuntimeProps";
import { AGENT_TERMINAL_ID } from "./useSessionTerminals";

/**
 * The Agent tab of a session whose agent is not running: what the agent terminal last printed,
 * read-only and in the terminal's history colour scheme (`HISTORY_THEME`, `HISTORY_FILTER`), with a
 * toast that says the session is not running and offers Resume.
 *
 * The output comes from `GetTerminalHistory` for the `main` terminal, which the daemon answers from
 * the session's recording once the process has exited. Keystrokes go nowhere. A session with no
 * recording says "No recorded output".
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-02-dormant-agent-tab.md (`#session-tabs 6/7`)
 */
export interface DormantAgentTerminalProps {
  sessionId: string;
  sessionToken: string;
  /** The owning daemon's terminal client — where the recorded history is read. */
  terminalClient?: TerminalClient | null;
  /** Resume the session. A rejection is shown in the toast and the greyed view stays. */
  onResume: () => Promise<void>;
}

/** A stopped session's whole recording stays scrollable, as the history view's does. */
const RECORDING_SCROLLBACK = 50000;

const NO_TERMINAL_CLIENT =
  "This session's terminal service is not connected, so its recorded output cannot be read.";

export function DormantAgentTerminal({
  sessionId,
  sessionToken,
  terminalClient = null,
  onResume,
}: DormantAgentTerminalProps): React.ReactElement {
  const terminalRef = useRef<GhosttyTerminalHandle>(null);
  // A touch device keeps the keyboard down over a view that takes no input, as the live terminal
  // does while its keyboard is closed. A pointer device may focus it, so a click lands in it.
  const isMobile = useIsMobile();
  // The emulator takes writes only once it has opened, so the recording is read from then on.
  const [opened, setOpened] = useState(false);
  const [bufferText, setBufferText] = useState("");
  const [noRecording, setNoRecording] = useState(false);
  const [historyError, setHistoryError] = useState<string | null>(null);
  const [resuming, setResuming] = useState(false);
  const [resumeError, setResumeError] = useState<string | null>(null);

  useEffect(() => {
    if (!opened || terminalClient === null) return;
    let stale = false;
    const fetchChunk = createForwardHistoryFetcher(terminalClient, {
      sessionToken,
      sessionId,
      terminalId: AGENT_TERMINAL_ID,
    });
    // A stopped session has no live stream to anchor on, so the fill runs to the recording's end,
    // which the daemon reports with `atEnd`.
    const loader = new TerminalHistoryForwardLoader(null);
    void (async () => {
      let wrote = false;
      try {
        while (!loader.done) {
          const chunk = await loader.loadNext(fetchChunk);
          if (stale) return;
          if (chunk === null) break;
          if (chunk.data.length > 0) {
            terminalRef.current?.write(chunk.data);
            wrote = true;
            setBufferText(terminalRef.current?.getBufferText?.() ?? "");
          }
        }
        if (!wrote) setNoRecording(true);
      } catch (err) {
        if (stale) return;
        // `NOT_FOUND` is the daemon's word that there is no recording for this terminal. Any other
        // failure is shown as what it is, not as an empty recording.
        if (ConnectError.from(err).code === Code.NotFound) setNoRecording(true);
        else setHistoryError(rpcFailureText(err));
      }
    })();
    return () => {
      stale = true;
    };
  }, [opened, terminalClient, sessionId, sessionToken]);

  const resume = async () => {
    setResuming(true);
    setResumeError(null);
    try {
      await onResume();
    } catch (err) {
      setResumeError(rpcFailureText(err));
    } finally {
      setResuming(false);
    }
  };

  return (
    <div
      data-testid="dormant-agent-terminal"
      data-color-scheme="history"
      className="relative flex-1 min-h-0 w-full overflow-hidden"
    >
      <div className="absolute inset-0" style={{ filter: HISTORY_FILTER }}>
        <GhosttyTerminal
          ref={terminalRef}
          theme={HISTORY_THEME}
          scrollback={RECORDING_SCROLLBACK}
          testId="ghostty-terminal-dormant"
          containerMinHeightPx={0}
          preventFocusOnTap={isMobile}
          onReady={() => setOpened(true)}
        />
      </div>
      {/* The text the terminal holds, read by the terminal specs as `GhosttyTerminalSession` exposes its own. */}
      <div data-testid="terminal-buffer-text" style={{ display: "none" }} aria-hidden>
        {bufferText}
      </div>
      {noRecording && (
        <div
          data-testid="dormant-agent-terminal-empty"
          className="absolute inset-0 flex items-center justify-center p-4 text-center text-sm text-muted-foreground"
        >
          No recorded output
        </div>
      )}
      <Toast
        testId="dormant-agent-toast"
        message="This session is not running"
        action={{ label: "Resume", onClick: () => void resume(), busy: resuming }}
        error={resumeError}
      />
      <InlineError
        testId="dormant-agent-terminal-error"
        message={terminalClient === null ? NO_TERMINAL_CLIENT : historyError}
        className="absolute inset-x-3 bottom-3 text-center"
      />
    </div>
  );
}
