import type React from "react";
import type { TerminalFeed } from "tddy-client-web/connections/terminal";
import type { TerminalConnectionStatus } from "../connection/terminalConnectionStatus";
import type { ToolShortcutDef } from "../lib/toolShortcuts";
import type { ByteDelta } from "../lib/byteDelta";

export interface TerminalSessionProps {
  /**
   * The session's terminal: its byte stream, its scrollback where the connection can serve one, and
   * the end of it. Opened by the caller off a `SessionConnection` (`openTerminal`), never built
   * here — which is what makes this component the same one on every wire.
   */
  feed: TerminalFeed;

  /**
   * Daemon session token + id for the file drop / upload affordances. When both are set, files
   * dropped on the terminal (or picked via the mobile Attach button) upload to the session dir and
   * their host paths are typed in. See `docs/ft/web/web-terminal.md` § File drop upload.
   */
  sessionToken?: string;
  sessionId?: string;

  /**
   * The session's agent reads this machine's filesystem, so a file dropped from the OS onto this
   * terminal inside Tddy Desktop has its real path typed rather than being uploaded. See
   * `TerminalFileDropZone`'s prop of the same name.
   */
  readsLocalPaths?: boolean;

  /**
   * How the connection carrying {@link feed} is doing, when the caller has something to say about
   * it. Drives the status dot and the raw `livekit-status` readout.
   *
   * Omitted by a caller whose connection state is shown elsewhere — the sessions drawer covers its
   * panes with `SessionConnectionOverlay` — in which case this component claims nothing about it
   * and the readout stays out of the layout.
   */
  connectionStatus?: TerminalConnectionStatus;

  /** Why the connection failed, shown over the terminal while {@link connectionStatus} is `error`. */
  connectionError?: string;

  /** When set, show connection chrome (status dot menu). Interrupt is the TUI Stop pane (SGR mouse → 0x03). */
  connectionOverlay?: {
    onDisconnect: () => void;
    buildId?: string;
    /** When set, the dot menu includes Terminate (daemon session SIGTERM path). */
    onTerminate?: () => void;
  };

  /**
   * Maps to `ConnectionTerminalChrome` `chromeLayout="statusBar"`:
   *
   * | `connectionChromePlacement` | Effect on status bar |
   * |------------------------------|----------------------|
   * | `"floating"` (default)       | Full bar: build id, dot, fullscreen, optional mobile keyboard slot |
   * | `"none"`                     | Compact bar: dot + menu (+ mobile keyboard); no build id / fullscreen (overlay panes) |
   */
  connectionChromePlacement?: "floating" | "none";

  /** Initial terminal font size (session baseline for Ctrl/⌘+0 reset). Default 14. */
  fontSize?: number;
  /** Bounds for zoom / pinch; default min 8, max 32. Overlay may pass a lower min (e.g. 2px). */
  minFontSize?: number;
  maxFontSize?: number;

  /** Fixed logical grid; font scales with the container (floating overlay). */
  fixedViewportGrid?: { cols: number; rows: number };
  /** Passed to `TerminalView` — use `0` for fixed-height overlay panes (default 200px min is too tall). */
  terminalContainerMinHeightPx?: number;
  /** When set, fullscreen targets this node (e.g. a fixed `connected-terminal-container`); otherwise the terminal flex root inside this component. */
  fullscreenTargetRef?: React.RefObject<HTMLElement | null>;

  /** Auto-focus the terminal once it is ready. Defaults to "not on a mobile viewport", where focusing pops the soft keyboard. */
  autoFocus?: boolean;
  /** Prevent the terminal taking focus on pointer/touch. Defaults to "on mobile while the keyboard is closed". */
  preventFocusOnTap?: boolean;
  /** Show the tap-to-type keyboard affordance. Defaults to the mobile viewport, where the soft keyboard is the only way in. */
  showMobileKeyboard?: boolean;

  /** Shortcut presets. When non-empty, renders the `ShortcutDrawer` overlay (desktop and mobile). */
  mobileShortcuts?: ToolShortcutDef[];

  /** Called with a function that returns keyboard focus to this terminal. */
  onRegisterFocus?: (focus: () => void) => void;
  /** Called with a function that types text into the terminal input (no newline). Lets the session
   *  runtime expose this terminal's insert to the inspector's Files tab (click/tap route). */
  onRegisterInsertInput?: (insertInput: (text: string) => void) => void;

  /**
   * Fired once when the remote terminal session ends — the feed said so ({@link TerminalFeed.ended}),
   * i.e. process/session teardown rather than the user choosing Disconnect in the menu.
   */
  onRemoteSessionEnded?: () => void;

  /**
   * Fired per terminal I/O event: once per received output chunk (`bytesIn`) and once per chunk
   * sent (`bytesOut`). The sessions drawer folds these into the per-session runtime's counters so
   * the inspector's I/O byte meter ticks live, even for a backgrounded session.
   */
  onBytes?: (delta: ByteDelta) => void;

  /** Called per output frame with the cumulative byte offset the client has now received up to.
   *  On a replay / catch-up frame (`endOffset > 0`) the offset snaps to the frame's absolute
   *  `endOffset`; a live tail frame (`endOffset === 0`) that follows it advances the offset by its
   *  byte length, while frames that PRECEDE it on the same open (the re-issued mode prologue) are
   *  out-of-band VT state and leave it untouched — see `TerminalStreamOffset`. A parent may use this
   *  to track how far the stream has reached; the host feeds open every stream with a snapshot and
   *  no longer resume by offset. */
  onOffsetUpdate?: (offset: bigint) => void;

  /** Expose the raw streamed text and byte count for E2E assertions (the `terminal-*` suite). */
  showBufferTextForTest?: boolean;
  /** Skip xterm.js rendering and surface the received-chunk count/sample instead (E2E RPC probe). */
  debugMode?: boolean;
  /** Log data flow and lifecycle events to the console. */
  debugLogging?: boolean;
}
