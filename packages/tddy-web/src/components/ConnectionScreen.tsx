import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { create } from "@bufbuild/protobuf";
import { GripVertical, Minus, Trash2 } from "lucide-react";
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import {
  AgentInfoSchema,
  ConnectionService,
  DeleteSessionRequestSchema,
  Signal,
  type AgentInfo,
  type ProjectEntry,
  type SessionEntry,
  type ToolInfo,
  type EligibleDaemonEntry,
} from "../gen/connection_pb";
import {
  buildAgentSelectOptionsFromRpc,
  coalesceBackendAgentSelection,
} from "./connection/agentOptions";
import { GhosttyTerminalLiveKit } from "./GhosttyTerminalLiveKit";
import { ConnectionTerminalChrome } from "./connection/ConnectionTerminalChrome";
import { ParticipantList } from "./ParticipantList";
import { useAuth } from "../hooks/useAuth";
import { useCommonRoom } from "../hooks/useCommonRoom";
import { useRoomParticipants } from "../hooks/useRoomParticipants";
import { GitHubLoginButton } from "./GitHubLoginButton";
import { UserAvatar } from "./UserAvatar";
import { BUILD_ID } from "../buildId";
import { useVisualViewport } from "../hooks/useVisualViewport";
import { TokenService } from "../gen/token_pb";
import {
  formatSessionCreatedAt,
  sessionIdFirstSegment,
  sessionPidDisplay,
} from "../utils/sessionDisplay";
import {
  isSessionOrphan,
  projectForUnscopedSession,
  sortedSessionsForProjectTable,
} from "../utils/sessionProjectTable";
import {
  computeHeaderCheckboxState,
  toggleRowInTableSelection,
  toggleSelectAllForTable,
} from "../utils/sessionSelection";
import { sortSessionsForDisplay } from "../utils/sessionSort";
import { SessionWorkflowStatusCells } from "./SessionWorkflowStatusCells";
import { SessionMoreActionsMenu } from "./session/SessionMoreActionsMenu";
import { SessionWorkflowFilesModal } from "./session/SessionWorkflowFilesModal";
import { DaemonNavMenu } from "./shell/DaemonNavMenu";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  isSessionListPath,
  parseTerminalSessionIdFromPathname,
  terminalPathForSessionId,
} from "../routing/appRoutes";
import {
  applyDedicatedTerminalBackToMini,
  clampTerminalOverlayPaneSize,
  TERMINAL_OVERLAY_COLS,
  TERMINAL_OVERLAY_FONT_MIN_PX,
  TERMINAL_OVERLAY_PANE_HEIGHT_PX,
  TERMINAL_OVERLAY_PANE_WIDTH_PX,
  TERMINAL_OVERLAY_ROWS,
  type TerminalPresentation,
  nextPresentationFromAttach,
} from "./connection/terminalPresentation";
import {
  addSessionAttachment,
  connectionAttachedTerminalTestId,
  focusedSessionIdFromPathname,
  removeSessionAttachment,
  type SessionAttachmentMap,
} from "./connection/multiSessionState";
import { presenceIdentityForUser } from "../lib/presenceIdentity";
import { DEFAULT_TERMINAL_FONT_MAX, DEFAULT_TERMINAL_FONT_MIN } from "../lib/terminalZoom";

/** Host dropdown: local daemon first, then peers; stable order by `instanceId` (matches daemon list policy). */
function sortEligibleDaemonsForDisplay(daemons: EligibleDaemonEntry[]): EligibleDaemonEntry[] {
  return [...daemons].sort((a, b) => {
    if (a.isLocal !== b.isLocal) {
      return a.isLocal ? -1 : 1;
    }
    return a.instanceId.localeCompare(b.instanceId);
  });
}

/** Full viewport width shell (session tables are not max-width capped). */
const screenShellClassName =
  "min-h-svh w-full min-w-0 box-border px-4 py-6 sm:px-6 font-sans text-foreground";

/** Floating pane chrome (header row above terminal grid); used for position clamping. */
const OVERLAY_HEADER_APPROX_PX = 36;
const OVERLAY_PANE_EDGE_MARGIN = 16;

function defaultOverlayPanePosition(
  paneWidth: number,
  paneInnerHeight: number,
  viewportWidth: number,
  viewportHeight: number,
): { left: number; top: number } {
  const m = OVERLAY_PANE_EDGE_MARGIN;
  const outerH = OVERLAY_HEADER_APPROX_PX + paneInnerHeight;
  return {
    left: Math.max(m, viewportWidth - paneWidth - m),
    top: Math.max(m, viewportHeight - outerH - m),
  };
}

function clampOverlayPanePosition(
  left: number,
  top: number,
  paneWidth: number,
  paneInnerHeight: number,
  viewportWidth: number,
  viewportHeight: number,
): { left: number; top: number } {
  const m = OVERLAY_PANE_EDGE_MARGIN;
  const outerH = OVERLAY_HEADER_APPROX_PX + paneInnerHeight;
  const maxLeft = Math.max(m, viewportWidth - paneWidth - m);
  const maxTop = Math.max(m, viewportHeight - outerH - m);
  return {
    left: Math.min(maxLeft, Math.max(m, left)),
    top: Math.min(maxTop, Math.max(m, top)),
  };
}

const inputStyle = {
  display: "block",
  width: "100%",
  marginBottom: 12,
  padding: 8,
  fontSize: 14,
  boxSizing: "border-box" as const,
};

const labelStyle = { display: "block", marginBottom: 4, fontWeight: 500 };

function createConnectionClient() {
  const transport = createConnectTransport({
    baseUrl: typeof window !== "undefined" ? `${window.location.origin}/rpc` : "",
    useBinaryFormat: true,
  });
  return createClient(ConnectionService, transport);
}

function createTokenClient() {
  const transport = createConnectTransport({
    baseUrl: typeof window !== "undefined" ? `${window.location.origin}/rpc` : "",
    useBinaryFormat: true,
  });
  return createClient(TokenService, transport);
}

type ProjectSessionForm = {
  toolPath: string;
  agent: string;
  /** Workflow recipe: `tdd`, `tdd-small`, `bugfix`, `free-prompting`, or `grill-me` (matches `WorkflowRecipe::name()`). */
  recipe: string;
  debugLogging: boolean;
  daemonInstanceId: string;
};

function defaultProjectSessionForm(
  tools: ToolInfo[],
  agents: AgentInfo[],
  daemons: EligibleDaemonEntry[],
): ProjectSessionForm {
  const localDaemon = daemons.find((d) => d.isLocal);
  const agentOptions = buildAgentSelectOptionsFromRpc(
    agents.map((a) => ({ id: a.id, label: a.label })),
  );
  return {
    toolPath: tools[0]?.path ?? "",
    agent: coalesceBackendAgentSelection(agentOptions, undefined),
    recipe: "free-prompting",
    debugLogging: false,
    daemonInstanceId: localDaemon?.instanceId ?? daemons[0]?.instanceId ?? "",
  };
}

const sessionControlSelectClassName =
  "box-border w-full min-w-[9rem] max-w-[16rem] rounded-md border border-input bg-background px-2 py-1.5 text-sm text-foreground shadow-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring";

/** Tool, backend, host, and browser-terminal debug for one project—per session / connection, not stored on the project. */
function ProjectSessionOptions({
  projectId,
  tools,
  agents,
  daemons,
  form,
  onChange,
  startSessionButton,
}: {
  projectId: string;
  tools: ToolInfo[];
  agents: AgentInfo[];
  daemons: EligibleDaemonEntry[];
  form: ProjectSessionForm;
  onChange: (patch: Partial<ProjectSessionForm>) => void;
  startSessionButton: ReactNode;
}) {
  const toolId = `tool-select-${projectId}`;
  const backendId = `backend-select-${projectId}`;
  const hostId = `host-select-${projectId}`;
  const recipeId = `recipe-select-${projectId}`;
  const debugId = `debug-logging-${projectId}`;
  const backendOptions = useMemo(
    () => buildAgentSelectOptionsFromRpc(agents.map((a) => ({ id: a.id, label: a.label }))),
    [agents],
  );
  return (
    <>
      <p className="mb-2 mt-2 text-xs text-muted-foreground">
        Tool, backend, workflow recipe, host, and debug apply only to <strong>Start New Session</strong> and to{" "}
        <strong>Connect / Resume</strong> in this project—not saved on the project.
      </p>
      <div className="flex min-w-0 flex-nowrap items-end gap-3 overflow-x-auto pb-1 pt-1 [scrollbar-width:thin]">
        <div className="flex min-w-[9rem] shrink-0 flex-col gap-1">
          <label className="text-sm font-medium leading-none" htmlFor={hostId}>
            Host (this session)
          </label>
          <select
            id={hostId}
            data-testid={hostId}
            value={form.daemonInstanceId}
            onChange={(e) => onChange({ daemonInstanceId: e.target.value })}
            className={sessionControlSelectClassName}
          >
            {daemons.map((d) => (
              <option key={d.instanceId} value={d.instanceId}>
                {d.label || d.instanceId}
              </option>
            ))}
          </select>
        </div>
        <div className="flex min-w-[9rem] shrink-0 flex-col gap-1">
          <label className="text-sm font-medium leading-none" htmlFor={toolId}>
            Tool (this session)
          </label>
          <select
            id={toolId}
            data-testid={toolId}
            value={form.toolPath}
            onChange={(e) => onChange({ toolPath: e.target.value })}
            className={sessionControlSelectClassName}
          >
            {tools.map((t) => (
              <option key={t.path} value={t.path}>
                {t.label || t.path}
              </option>
            ))}
          </select>
        </div>
        <div className="flex min-w-[9rem] shrink-0 flex-col gap-1">
          <label className="text-sm font-medium leading-none" htmlFor={backendId}>
            Backend (this session)
          </label>
          <select
            id={backendId}
            data-testid={backendId}
            value={form.agent}
            onChange={(e) => onChange({ agent: e.target.value })}
            className={sessionControlSelectClassName}
          >
            {backendOptions.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
        </div>
        <div className="flex min-w-[10rem] shrink-0 flex-col gap-1">
          <label className="text-sm font-medium leading-none" htmlFor={recipeId}>
            Workflow recipe (this session)
          </label>
          <select
            id={recipeId}
            data-testid={recipeId}
            value={form.recipe}
            onChange={(e) => onChange({ recipe: e.target.value })}
            className={sessionControlSelectClassName}
          >
            <option value="tdd">TDD (plan → implement)</option>
            <option value="tdd-small">TDD small (plan → red → green)</option>
            <option value="bugfix">Bugfix (reproduce → fix)</option>
            <option value="free-prompting">Free prompting (open-ended)</option>
            <option value="grill-me">Grill me (Grill → Create plan)</option>
          </select>
        </div>
        <label
          className="flex shrink-0 cursor-pointer items-center gap-2 pb-2 text-sm leading-tight"
          htmlFor={debugId}
        >
          <input
            id={debugId}
            data-testid={debugId}
            type="checkbox"
            className="size-4 shrink-0 rounded border border-input accent-primary"
            checked={form.debugLogging}
            onChange={(e) => onChange({ debugLogging: e.target.checked })}
          />
          <span className="max-w-[11rem] sm:max-w-none sm:whitespace-nowrap">
            Debug logging (browser terminal, this connection)
          </span>
        </label>
        <div className="shrink-0 pb-0.5">{startSessionButton}</div>
      </div>
    </>
  );
}

function SignalDropdown({
  sessionId,
  onSignal,
}: {
  sessionId: string;
  onSignal: (sessionId: string, signal: Signal) => void;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  const handleClick = (signal: Signal) => {
    setOpen(false);
    onSignal(sessionId, signal);
  };

  return (
    <div ref={ref} className="relative ml-1 inline-block">
      <Button
        type="button"
        variant="outline"
        size="sm"
        data-testid={`signal-dropdown-${sessionId}`}
        onClick={() => setOpen((o) => !o)}
      >
        Signal ▾
      </Button>
      {open && (
        <div
          data-testid={`signal-menu-${sessionId}`}
          className="absolute top-full left-0 z-[1000] min-w-[180px] overflow-hidden rounded-md border border-border bg-popover p-1 text-popover-foreground shadow-md"
        >
          <Button
            type="button"
            variant="ghost"
            size="sm"
            data-testid={`signal-sigint-${sessionId}`}
            className="h-auto w-full justify-start rounded-sm px-3 py-2 font-normal"
            onClick={() => handleClick(Signal.SIGINT)}
          >
            Interrupt (SIGINT)
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            data-testid={`signal-sigterm-${sessionId}`}
            className="h-auto w-full justify-start rounded-sm px-3 py-2 font-normal"
            onClick={() => handleClick(Signal.SIGTERM)}
          >
            Terminate (SIGTERM)
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            data-testid={`signal-sigkill-${sessionId}`}
            className="h-auto w-full justify-start rounded-sm px-3 py-2 font-normal text-destructive hover:text-destructive"
            onClick={() => handleClick(Signal.SIGKILL)}
          >
            Kill (SIGKILL)
          </Button>
        </div>
      )}
    </div>
  );
}

/** Trash control — same `data-testid` for active and inactive rows. */
function SessionDeleteButton({
  sessionId,
  onDelete,
}: {
  sessionId: string;
  onDelete: (sessionId: string) => void | Promise<void>;
}) {
  return (
    <Button
      type="button"
      variant="destructive"
      size="icon-sm"
      aria-label="Delete session"
      title="Delete session"
      data-testid={`delete-session-${sessionId}`}
      onClick={() => void onDelete(sessionId)}
    >
      <Trash2 />
    </Button>
  );
}

/** Resume + Delete for inactive session rows (project and orphan tables share stable `data-testid`s). */
function InactiveSessionActions({
  sessionId,
  onResume,
  onDelete,
}: {
  sessionId: string;
  onResume: (sessionId: string) => void;
  onDelete: (sessionId: string) => void | Promise<void>;
}) {
  return (
    <span className="inline-flex flex-wrap items-center gap-2">
      <Button
        type="button"
        variant="secondary"
        size="sm"
        data-testid={`resume-${sessionId}`}
        onClick={() => onResume(sessionId)}
      >
        Resume
      </Button>
      <SessionDeleteButton sessionId={sessionId} onDelete={onDelete} />
    </span>
  );
}

/** Per-table header "select all" — `indeterminate` set from `computeHeaderCheckboxState`. */
function SessionTableSelectAllCheckbox({
  selectedCount,
  totalRows,
  dataTestId,
  ariaLabel,
  onToggle,
}: {
  selectedCount: number;
  totalRows: number;
  dataTestId: string;
  ariaLabel: string;
  onToggle: () => void;
}) {
  const ref = useRef<HTMLInputElement>(null);
  const { checked, indeterminate } = computeHeaderCheckboxState(selectedCount, totalRows);
  useEffect(() => {
    const el = ref.current;
    if (el) {
      el.indeterminate = indeterminate;
    }
  }, [checked, indeterminate]);

  return (
    <input
      ref={ref}
      type="checkbox"
      data-testid={dataTestId}
      aria-label={ariaLabel}
      checked={checked}
      onChange={() => onToggle()}
      className="size-4 shrink-0 rounded border border-input accent-primary"
    />
  );
}

function ConnectedTerminal({
  livekitUrl,
  roomName,
  identity,
  serverIdentity,
  debugLogging,
  onDisconnect,
  onTerminate,
  onRemoteSessionEnded,
  terminalLayout = "fullscreen",
  onExpandTerminal,
  onBackToMini,
  onMinimizePane,
  paneSessionLabel,
}: {
  livekitUrl: string;
  roomName: string;
  identity: string;
  serverIdentity: string;
  debugLogging?: boolean;
  onDisconnect: () => void;
  onTerminate?: () => void | Promise<void>;
  /** When daemon/coder session ends (LiveKit or stream), parent returns to Connection screen. */
  onRemoteSessionEnded?: () => void;
  /** fullscreen = dedicated route; overlay | mini = floating pane (80×24 terminal grid; header separate). */
  terminalLayout?: "fullscreen" | "overlay" | "mini";
  onExpandTerminal?: () => void;
  /** Fullscreen only: shrink to mini without disconnecting. */
  onBackToMini?: () => void;
  /** Floating pane only: minimize control (session row uses Hide / Open). */
  onMinimizePane?: () => void;
  /** Short session id (same as session table first segment / two UUID groups). */
  paneSessionLabel: string;
}) {
  type OverlayResizeCorner = "nw" | "ne" | "sw" | "se";
  const tokenClient = useMemo(() => createTokenClient(), []);
  const fullscreenTargetRef = useRef<HTMLDivElement>(null);
  const [initialToken, setInitialToken] = useState<string | null>(null);
  const [ttlSeconds, setTtlSeconds] = useState<bigint | null>(null);
  const [error, setError] = useState<string | null>(null);
  const { height: viewportHeight, isKeyboardOpen } = useVisualViewport();
  const isMobile =
    typeof window !== "undefined" &&
    (("ontouchstart" in window) || window.innerWidth < 768);

  const [compactPaneWidth, setCompactPaneWidth] = useState(TERMINAL_OVERLAY_PANE_WIDTH_PX);
  const [compactPaneHeight, setCompactPaneHeight] = useState(TERMINAL_OVERLAY_PANE_HEIGHT_PX);
  const [paneLiveKitStatus, setPaneLiveKitStatus] = useState<"connecting" | "connected" | "error">(
    "connecting",
  );
  const overlayResizeRef = useRef<{
    pointerId: number;
    corner: OverlayResizeCorner;
    startW: number;
    startH: number;
    originX: number;
    originY: number;
  } | null>(null);

  const [overlayPanePosition, setOverlayPanePosition] = useState(() =>
    typeof window !== "undefined"
      ? defaultOverlayPanePosition(
          TERMINAL_OVERLAY_PANE_WIDTH_PX,
          TERMINAL_OVERLAY_PANE_HEIGHT_PX,
          window.innerWidth,
          window.innerHeight,
        )
      : { left: OVERLAY_PANE_EDGE_MARGIN, top: OVERLAY_PANE_EDGE_MARGIN },
  );
  const overlayDragRef = useRef<{
    pointerId: number;
    startX: number;
    startY: number;
    originLeft: number;
    originTop: number;
  } | null>(null);

  const overlayPaneMaxSize = useCallback(() => {
    if (typeof window === "undefined") {
      return { maxW: 3840, maxH: 2160 };
    }
    return {
      maxW: window.innerWidth - 32,
      maxH: window.innerHeight - 96,
    };
  }, []);

  useEffect(() => {
    tokenClient
      .generateToken({ room: roomName, identity })
      .then((res) => {
        setInitialToken(res.token);
        setTtlSeconds(res.ttlSeconds);
      })
      .catch((e) => {
        setError(
          e instanceof Error
            ? e.message
            : "Token fetch failed. Ensure tddy-daemon is running with LiveKit."
        );
      });
  }, [tokenClient, roomName, identity]);

  const getToken = useMemo(
    () => async () => {
      const res = await tokenClient.refreshToken({ room: roomName, identity });
      return { token: res.token, ttlSeconds: res.ttlSeconds };
    },
    [tokenClient, roomName, identity]
  );

  const isCompact = terminalLayout === "overlay" || terminalLayout === "mini";

  useEffect(() => {
    if (!isCompact || typeof window === "undefined") return;
    const vh = viewportHeight > 0 ? viewportHeight : window.innerHeight;
    setOverlayPanePosition((p) =>
      clampOverlayPanePosition(
        p.left,
        p.top,
        compactPaneWidth,
        compactPaneHeight,
        window.innerWidth,
        vh,
      ),
    );
  }, [isCompact, compactPaneWidth, compactPaneHeight, viewportHeight]);

  useEffect(() => {
    if (!isCompact || typeof window === "undefined") return;
    const onWinResize = () => {
      const vh = viewportHeight > 0 ? viewportHeight : window.innerHeight;
      setOverlayPanePosition((p) =>
        clampOverlayPanePosition(
          p.left,
          p.top,
          compactPaneWidth,
          compactPaneHeight,
          window.innerWidth,
          vh,
        ),
      );
    };
    window.addEventListener("resize", onWinResize);
    return () => window.removeEventListener("resize", onWinResize);
  }, [isCompact, compactPaneWidth, compactPaneHeight, viewportHeight]);

  const onOverlayDragPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!isCompact || !onExpandTerminal) return;
    if ((e.target as HTMLElement).closest("[data-terminal-header-actions]")) return;
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    overlayDragRef.current = {
      pointerId: e.pointerId,
      startX: e.clientX,
      startY: e.clientY,
      originLeft: overlayPanePosition.left,
      originTop: overlayPanePosition.top,
    };
  };

  const onOverlayDragPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    const drag = overlayDragRef.current;
    if (!drag || drag.pointerId !== e.pointerId) return;
    if (typeof window === "undefined") return;
    const vh = viewportHeight > 0 ? viewportHeight : window.innerHeight;
    const nextLeft = drag.originLeft + (e.clientX - drag.startX);
    const nextTop = drag.originTop + (e.clientY - drag.startY);
    setOverlayPanePosition(
      clampOverlayPanePosition(
        nextLeft,
        nextTop,
        compactPaneWidth,
        compactPaneHeight,
        window.innerWidth,
        vh,
      ),
    );
  };

  const onOverlayDragPointerUp = (e: React.PointerEvent<HTMLDivElement>) => {
    const drag = overlayDragRef.current;
    if (!drag || drag.pointerId !== e.pointerId) return;
    overlayDragRef.current = null;
    try {
      (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
    } catch {
      /* already released */
    }
  };

  const onOverlayResizePointerDown = (e: React.PointerEvent) => {
    if (!isCompact) return;
    const corner = (e.currentTarget as HTMLElement).dataset
      .resizeCorner as OverlayResizeCorner | undefined;
    if (!corner || !["nw", "ne", "sw", "se"].includes(corner)) return;
    e.preventDefault();
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    overlayResizeRef.current = {
      pointerId: e.pointerId,
      corner,
      startW: compactPaneWidth,
      startH: compactPaneHeight,
      originX: e.clientX,
      originY: e.clientY,
    };
  };

  const onOverlayResizePointerMove = (e: React.PointerEvent) => {
    const drag = overlayResizeRef.current;
    if (!drag || drag.pointerId !== e.pointerId) return;
    const dx = e.clientX - drag.originX;
    const dy = e.clientY - drag.originY;
    let w = drag.startW;
    let h = drag.startH;
    switch (drag.corner) {
      case "se":
        w = drag.startW + dx;
        h = drag.startH + dy;
        break;
      case "sw":
        w = drag.startW - dx;
        h = drag.startH + dy;
        break;
      case "ne":
        w = drag.startW + dx;
        h = drag.startH - dy;
        break;
      case "nw":
        w = drag.startW - dx;
        h = drag.startH - dy;
        break;
      default:
        break;
    }
    const { maxW, maxH } = overlayPaneMaxSize();
    const { width, height } = clampTerminalOverlayPaneSize(w, h, maxW, maxH);
    setCompactPaneWidth(width);
    setCompactPaneHeight(height);
  };

  const onOverlayResizePointerUp = (e: React.PointerEvent) => {
    const drag = overlayResizeRef.current;
    if (!drag || drag.pointerId !== e.pointerId) return;
    overlayResizeRef.current = null;
    try {
      (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
    } catch {
      /* already released */
    }
  };

  const fullscreenContainerStyle = {
    position: "fixed" as const,
    top: 0,
    left: 0,
    right: 0,
    height: viewportHeight,
    margin: 0,
    overflow: "hidden" as const,
    display: "flex" as const,
    flexDirection: "column" as const,
  };

  const compactContainerBaseStyle = {
    position: "fixed" as const,
    zIndex: 50,
    boxShadow: "0 8px 32px rgba(0,0,0,0.45)",
    borderRadius: 8,
    overflow: "hidden" as const,
    display: "flex" as const,
    flexDirection: "column" as const,
    backgroundColor: "#0a0a0a",
  };

  const terminalPaneInnerStyle = isCompact
    ? {
        position: "relative" as const,
        zIndex: 0,
        width: "100%",
        height: compactPaneHeight,
        minHeight: 0,
        flexShrink: 0,
      }
    : { flex: 1, minHeight: 0, position: "relative" as const };

  const outerStyle = isCompact
    ? {
        ...compactContainerBaseStyle,
        width: compactPaneWidth,
        left: overlayPanePosition.left,
        top: overlayPanePosition.top,
        bottom: "auto" as const,
        right: "auto" as const,
      }
    : fullscreenContainerStyle;

  const overlayResizeHandles = isCompact
    ? (
        [
          { corner: "nw" as const, cursor: "nw-resize", label: "Resize from top left", testSuffix: "nw" },
          { corner: "ne" as const, cursor: "ne-resize", label: "Resize from top right", testSuffix: "ne" },
          { corner: "sw" as const, cursor: "sw-resize", label: "Resize from bottom left", testSuffix: "sw" },
          { corner: "se" as const, cursor: "se-resize", label: "Resize from bottom right", testSuffix: "se" },
        ] as const
      ).map(({ corner, cursor, label, testSuffix }) => (
        <button
          key={corner}
          type="button"
          data-testid={`terminal-overlay-resize-handle-${testSuffix}`}
          data-resize-corner={corner}
          aria-label={label}
          onPointerDown={onOverlayResizePointerDown}
          onPointerMove={onOverlayResizePointerMove}
          onPointerUp={onOverlayResizePointerUp}
          onPointerCancel={onOverlayResizePointerUp}
          style={{
            position: "absolute",
            ...(corner === "nw" || corner === "sw" ? { left: 0 } : { right: 0 }),
            ...(corner === "nw" || corner === "ne" ? { top: 0 } : { bottom: 0 }),
            width: 14,
            height: 14,
            cursor,
            touchAction: "none",
            zIndex: 130,
            border: "none",
            padding: 0,
            margin: 0,
            background:
              corner === "se"
                ? "linear-gradient(135deg, transparent 50%, rgba(255,255,255,0.2) 50%, rgba(255,255,255,0.2) 100%)"
                : corner === "sw"
                  ? "linear-gradient(225deg, transparent 50%, rgba(255,255,255,0.2) 50%, rgba(255,255,255,0.2) 100%)"
                  : corner === "ne"
                    ? "linear-gradient(45deg, transparent 50%, rgba(255,255,255,0.2) 50%, rgba(255,255,255,0.2) 100%)"
                    : "linear-gradient(315deg, transparent 50%, rgba(255,255,255,0.2) 50%, rgba(255,255,255,0.2) 100%)",
            borderRadius: 4,
          }}
        />
      ))
    : null;

  if (error) {
    return (
      <div style={{ padding: 24 }}>
        <div data-testid="livekit-error">{error}</div>
      </div>
    );
  }
  if (!initialToken || ttlSeconds === null) {
    return (
      <div
        ref={fullscreenTargetRef}
        data-testid="connected-terminal-container"
        style={outerStyle}
        className={isCompact ? "relative" : undefined}
      >
        {isCompact && onExpandTerminal ? (
          <div
            data-testid="terminal-overlay-drag-header"
            className="flex shrink-0 cursor-grab select-none items-center gap-1 border-b border-border bg-muted px-2 py-1 text-[10px] text-foreground active:cursor-grabbing"
            style={{ touchAction: "none", position: "relative", zIndex: 40 }}
            onPointerDown={onOverlayDragPointerDown}
            onPointerMove={onOverlayDragPointerMove}
            onPointerUp={onOverlayDragPointerUp}
            onPointerCancel={onOverlayDragPointerUp}
          >
            <GripVertical className="size-3 shrink-0 text-muted-foreground" aria-hidden />
            <span className="min-w-0 flex-1 truncate font-mono">{paneSessionLabel}</span>
            <div data-terminal-header-actions className="flex shrink-0 items-center gap-0.5">
              <ConnectionTerminalChrome
                chromeLayout="paneHeader"
                overlayStatus="connecting"
                onDisconnect={onDisconnect}
                onTerminate={onTerminate}
                fullscreenTargetRef={fullscreenTargetRef}
              />
              {onMinimizePane ? (
                <button
                  type="button"
                  className="shrink-0 rounded border border-input bg-background p-0.5 text-foreground"
                  data-testid="terminal-overlay-minimize"
                  aria-label="Minimize terminal"
                  onClick={onMinimizePane}
                >
                  <Minus className="size-3" aria-hidden />
                </button>
              ) : null}
              <button
                type="button"
                className="shrink-0 rounded border border-input bg-background px-1.5 py-0.5"
                data-testid="terminal-reconnect-expand"
                onClick={onExpandTerminal}
              >
                Expand
              </button>
            </div>
          </div>
        ) : null}
        <div style={terminalPaneInnerStyle}>
          <ConnectionTerminalChrome
            overlayStatus="connecting"
            buildId={BUILD_ID}
            onDisconnect={onDisconnect}
            onTerminate={onTerminate}
            fullscreenTargetRef={fullscreenTargetRef}
          />
        </div>
        {overlayResizeHandles}
      </div>
    );
  }

  return (
    <div
      ref={fullscreenTargetRef}
      data-testid="connected-terminal-container"
      style={outerStyle}
      className={isCompact ? "relative" : undefined}
    >
      {!isCompact && onBackToMini ? (
        <button
          type="button"
          data-testid="terminal-back-to-mini"
          className="absolute left-2 top-2 z-[120] rounded border border-input bg-background/90 px-2 py-1 text-xs text-foreground shadow"
          onClick={onBackToMini}
        >
          Back
        </button>
      ) : null}
      {isCompact && onExpandTerminal ? (
        <div
          data-testid="terminal-overlay-drag-header"
          className="flex shrink-0 cursor-grab select-none items-center gap-1 border-b border-border bg-muted px-2 py-1 text-[10px] text-foreground active:cursor-grabbing"
          style={{ touchAction: "none", position: "relative", zIndex: 40 }}
          onPointerDown={onOverlayDragPointerDown}
          onPointerMove={onOverlayDragPointerMove}
          onPointerUp={onOverlayDragPointerUp}
          onPointerCancel={onOverlayDragPointerUp}
        >
          <GripVertical className="size-3 shrink-0 text-muted-foreground" aria-hidden />
          <span className="min-w-0 flex-1 truncate font-mono">{paneSessionLabel}</span>
          <div data-terminal-header-actions className="flex shrink-0 items-center gap-0.5">
            <ConnectionTerminalChrome
              chromeLayout="paneHeader"
              overlayStatus={paneLiveKitStatus}
              onDisconnect={onDisconnect}
              onTerminate={onTerminate}
              fullscreenTargetRef={fullscreenTargetRef}
            />
            {onMinimizePane ? (
              <button
                type="button"
                className="shrink-0 rounded border border-input bg-background p-0.5 text-foreground"
                data-testid="terminal-overlay-minimize"
                aria-label="Minimize terminal"
                onClick={onMinimizePane}
              >
                <Minus className="size-3" aria-hidden />
              </button>
            ) : null}
            <button
              type="button"
              className="shrink-0 rounded border border-input bg-background px-1.5 py-0.5"
              data-testid="terminal-reconnect-expand"
              onClick={onExpandTerminal}
            >
              Expand
            </button>
          </div>
        </div>
      ) : null}
      <div style={terminalPaneInnerStyle}>
        <GhosttyTerminalLiveKit
          url={livekitUrl}
          token={initialToken}
          getToken={getToken}
          ttlSeconds={ttlSeconds}
          roomName={roomName}
          serverIdentity={serverIdentity}
          debugMode={false}
          debugLogging={debugLogging ?? false}
          autoFocus={!isMobile && !isCompact}
          preventFocusOnTap={isMobile && !isKeyboardOpen}
          showMobileKeyboard={isMobile}
          connectionOverlay={{ onDisconnect, buildId: BUILD_ID, onTerminate }}
          connectionChromePlacement={isCompact ? "none" : "floating"}
          onConnectionStatusChange={isCompact ? setPaneLiveKitStatus : undefined}
          fullscreenTargetRef={fullscreenTargetRef}
          fontSize={14}
          minFontSize={isCompact ? TERMINAL_OVERLAY_FONT_MIN_PX : DEFAULT_TERMINAL_FONT_MIN}
          maxFontSize={DEFAULT_TERMINAL_FONT_MAX}
          terminalContainerMinHeightPx={isCompact ? 0 : undefined}
          fixedViewportGrid={
            isCompact
              ? { cols: TERMINAL_OVERLAY_COLS, rows: TERMINAL_OVERLAY_ROWS }
              : undefined
          }
          onRemoteSessionEnded={onRemoteSessionEnded}
        />
      </div>
      {overlayResizeHandles}
    </div>
  );
}

export function ConnectionScreen({
  livekitUrl,
  commonRoom,
  allowedAgentsFromConfig,
  onNavigate,
}: {
  livekitUrl?: string;
  commonRoom?: string;
  /** From GET /api/config (daemon `allowed_agents`); preferred over RPC until ListAgents hydrates. */
  allowedAgentsFromConfig?: { id: string; label: string }[];
  /** Client-side navigation (daemon shell: Sessions ↔ Worktrees). */
  onNavigate?: (path: string) => void;
} = {}) {
  const { user, isAuthenticated, isLoading, login, logout, sessionToken } = useAuth();
  const [tools, setTools] = useState<ToolInfo[]>([]);
  const [agents, setAgents] = useState<AgentInfo[]>([]);
  const [daemons, setDaemons] = useState<EligibleDaemonEntry[]>([]);
  const [sessions, setSessions] = useState<SessionEntry[]>([]);
  const [projects, setProjects] = useState<ProjectEntry[]>([]);
  const [projectForms, setProjectForms] = useState<Record<string, ProjectSessionForm>>({});
  const [orphanSessionDebug, setOrphanSessionDebug] = useState(false);
  /** Per project / orphan table: selected session ids for bulk actions (independent per table). */
  const [tableSessionSelections, setTableSessionSelections] = useState<Record<string, Set<string>>>(
    {},
  );
  const [workflowFilesSessionId, setWorkflowFilesSessionId] = useState<string | null>(null);
  const [createProjectOpen, setCreateProjectOpen] = useState(false);
  const [newProjectName, setNewProjectName] = useState("");
  const [newProjectGitUrl, setNewProjectGitUrl] = useState("");
  const [newProjectUserRelativePath, setNewProjectUserRelativePath] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const effectiveAgents: AgentInfo[] = useMemo(() => {
    if (allowedAgentsFromConfig && allowedAgentsFromConfig.length > 0) {
      return allowedAgentsFromConfig.map((a) =>
        create(AgentInfoSchema, { id: a.id, label: a.label }),
      );
    }
    return agents;
  }, [allowedAgentsFromConfig, agents]);
  const [sessionAttachments, setSessionAttachments] = useState<SessionAttachmentMap>(
    () => new Map(),
  );
  const [routePath, setRoutePath] = useState(
    () => (typeof window !== "undefined" ? window.location.pathname : "/"),
  );
  const [sessionsListHydrated, setSessionsListHydrated] = useState(false);
  const [terminalRouteUnknown, setTerminalRouteUnknown] = useState(false);
  const [terminalPresentation, setTerminalPresentation] = useState<TerminalPresentation>("hidden");
  const [terminalOverlayMinimized, setTerminalOverlayMinimized] = useState(false);
  const terminalDeepLinkSeqRef = useRef(0);
  const sessionsEverLoadedRef = useRef(false);
  const client = useMemo(() => createConnectionClient(), []);

  const navigatePath = useCallback(
    (path: string, mode: "push" | "replace") => {
      if (typeof window === "undefined") return;
      if (mode === "push" && onNavigate) {
        onNavigate(path);
      } else {
        if (mode === "push") {
          window.history.pushState(null, "", path);
        } else {
          window.history.replaceState(null, "", path);
        }
      }
      setRoutePath(path);
    },
    [onNavigate],
  );

  const focusedSessionId = useMemo(
    () => focusedSessionIdFromPathname(routePath, sessionAttachments),
    [routePath, sessionAttachments],
  );

  const removeAttachmentForSession = useCallback(
    (sessionId: string, reason: string) => {
      console.info("[ConnectionScreen] removeAttachmentForSession", { sessionId, reason });
      setSessionAttachments((prev) => {
        const next = removeSessionAttachment(prev, sessionId);
        queueMicrotask(() => {
          if (next.size === 0) {
            setTerminalPresentation("hidden");
            navigatePath("/", "replace");
          } else {
            const p = typeof window !== "undefined" ? window.location.pathname : "/";
            const focus = focusedSessionIdFromPathname(p, next);
            if (focus) {
              navigatePath(terminalPathForSessionId(focus), "replace");
            }
          }
        });
        return next;
      });
    },
    [navigatePath],
  );

  useEffect(() => {
    if (sessionAttachments.size === 0) {
      setTerminalOverlayMinimized(false);
    }
  }, [sessionAttachments.size]);

  useEffect(() => {
    const onPopState = () => {
      const p = window.location.pathname;
      setRoutePath(p);
      if (isSessionListPath(p)) {
        setSessionAttachments(new Map());
        setTerminalRouteUnknown(false);
        setTerminalPresentation("hidden");
      }
    };
    window.addEventListener("popstate", onPopState);
    return () => window.removeEventListener("popstate", onPopState);
  }, []);

  const presenceReady =
    Boolean(commonRoom?.trim() && livekitUrl?.trim()) &&
    isAuthenticated &&
    !isLoading &&
    Boolean(user);

  const presenceIdentity = useMemo(
    () => (user ? presenceIdentityForUser(user.login) : undefined),
    [user?.login],
  );

  const { room: presenceRoom, status: presenceStatus, error: presenceError } = useCommonRoom(
    presenceReady ? livekitUrl : undefined,
    presenceReady ? commonRoom : undefined,
    presenceReady ? presenceIdentity : undefined
  );

  const participants = useRoomParticipants(presenceReady ? presenceRoom : null);

  const hasActiveSession = useMemo(
    () => sessions.some((s) => s.isActive),
    [sessions]
  );

  const loadSessions = useCallback(() => {
    if (!sessionToken) return;
    client
      .listSessions({ sessionToken })
      .then((res) => {
        setSessions(res.sessions);
        sessionsEverLoadedRef.current = true;
        setSessionsListHydrated(true);
      })
      .catch((e) => {
        setSessions([]);
        setSessionsListHydrated(true);
        if (!sessionsEverLoadedRef.current) {
          setError(e instanceof Error ? e.message : "Failed to list sessions");
        }
      });
  }, [client, sessionToken]);

  useEffect(() => {
    if (!sessionToken || !isAuthenticated) {
      setLoading(false);
      return;
    }
    Promise.all([client.listTools({}), client.listAgents({})])
      .then(([toolsRes, agentsRes]) => {
        setTools(toolsRes.tools);
        setAgents(agentsRes.agents);
        console.debug("[ConnectionScreen] ListTools + ListAgents loaded", {
          tools: toolsRes.tools.length,
          agents: agentsRes.agents.length,
        });
      })
      .catch((e) => {
        setTools([]);
        setAgents([]);
        setError(e instanceof Error ? e.message : "Failed to list tools or agents");
      })
      .finally(() => setLoading(false));

    client
      .listEligibleDaemons({ sessionToken })
      .then((res) => {
        const sorted = sortEligibleDaemonsForDisplay(res.daemons);
        console.debug("[ConnectionScreen] ListEligibleDaemons", {
          count: sorted.length,
          order: sorted.map((d) => ({ id: d.instanceId, isLocal: d.isLocal })),
        });
        setDaemons(sorted);
      })
      .catch(() => setDaemons([]));

    const loadProjects = () => {
      client
        .listProjects({ sessionToken })
        .then((res) => setProjects(res.projects))
        .catch(() => setProjects([]));
    };
    loadSessions();
    loadProjects();
    const projectInterval = setInterval(loadProjects, 5000);
    return () => clearInterval(projectInterval);
  }, [client, sessionToken, isAuthenticated, loadSessions]);

  useEffect(() => {
    if (!sessionToken || !isAuthenticated) {
      return;
    }
    const sessionPollMs = hasActiveSession ? 2000 : 5000;
    const sessionInterval = setInterval(loadSessions, sessionPollMs);
    return () => clearInterval(sessionInterval);
  }, [sessionToken, isAuthenticated, hasActiveSession, loadSessions]);

  useEffect(() => {
    setProjectForms((prev) => {
      const next = { ...prev };
      const def = defaultProjectSessionForm(tools, effectiveAgents, daemons);
      const agentOptions = buildAgentSelectOptionsFromRpc(
        effectiveAgents.map((a) => ({ id: a.id, label: a.label })),
      );
      for (const p of projects) {
        const existing = next[p.projectId];
        if (!existing) {
          next[p.projectId] = { ...def };
        } else {
          const toolStillValid = tools.some((t) => t.path === existing.toolPath);
          if (!toolStillValid && tools[0]) {
            next[p.projectId] = { ...existing, toolPath: tools[0].path };
          }
          const agentStillValid = effectiveAgents.some((a) => a.id === existing.agent);
          if (!agentStillValid) {
            next[p.projectId] = {
              ...next[p.projectId],
              agent: coalesceBackendAgentSelection(agentOptions, existing.agent),
            };
          }
          if (!existing.daemonInstanceId && def.daemonInstanceId) {
            next[p.projectId] = { ...next[p.projectId], daemonInstanceId: def.daemonInstanceId };
          }
          if (!existing.recipe?.trim()) {
            next[p.projectId] = { ...next[p.projectId], recipe: def.recipe };
          }
        }
      }
      return next;
    });
  }, [projects, tools, effectiveAgents, daemons]);

  const updateProjectForm = (projectId: string, patch: Partial<ProjectSessionForm>) => {
    setProjectForms((prev) => ({
      ...prev,
      [projectId]: {
        ...(prev[projectId] ?? defaultProjectSessionForm(tools, effectiveAgents, daemons)),
        ...patch,
      },
    }));
  };

  const knownProjectIds = useMemo(
    () => new Set(projects.map((p) => p.projectId)),
    [projects]
  );
  const orphanSessions = useMemo(
    () => sortSessionsForDisplay(sessions.filter((s) => isSessionOrphan(s, projects))),
    [sessions, projects]
  );

  const orphanTableKey = "orphan";
  const orphanSelectedSet = tableSessionSelections[orphanTableKey] ?? new Set<string>();
  const orphanAllSessionIds = useMemo(
    () => orphanSessions.map((s) => s.sessionId),
    [orphanSessions],
  );

  const debugForSessionId = useCallback(
    (sessionId: string): boolean => {
      const sess = sessions.find((s) => s.sessionId === sessionId);
      if (!sess) return false;
      if (knownProjectIds.has(sess.projectId)) {
        return projectForms[sess.projectId]?.debugLogging ?? false;
      }
      if (sess.projectId.trim() === "") {
        const matched = projectForUnscopedSession(sess, projects);
        if (matched) {
          return projectForms[matched.projectId]?.debugLogging ?? false;
        }
      }
      return orphanSessionDebug;
    },
    [sessions, projectForms, knownProjectIds, projects, orphanSessionDebug],
  );

  useEffect(() => {
    if (!sessionsListHydrated || !isAuthenticated || !sessionToken) {
      return;
    }
    const id = parseTerminalSessionIdFromPathname(routePath);
    if (!id) {
      setTerminalRouteUnknown(false);
      return;
    }
    if (sessionAttachments.has(id)) {
      setTerminalRouteUnknown(false);
      return;
    }
    const known = sessions.some((s) => s.sessionId === id);
    setTerminalRouteUnknown(!known);
  }, [routePath, sessions, sessionsListHydrated, isAuthenticated, sessionToken, sessionAttachments]);

  useEffect(() => {
    if (!sessionsListHydrated || !isAuthenticated || !sessionToken || terminalRouteUnknown) {
      return;
    }
    const id = parseTerminalSessionIdFromPathname(routePath);
    if (!id || sessionAttachments.has(id)) {
      return;
    }
    if (!sessions.some((s) => s.sessionId === id)) {
      return;
    }
    const seq = ++terminalDeepLinkSeqRef.current;
    let cancelled = false;
    void (async () => {
      setError(null);
      try {
        const res = await client.connectSession({ sessionToken, sessionId: id });
        if (cancelled || seq !== terminalDeepLinkSeqRef.current) return;
        setSessionAttachments((prev) =>
          addSessionAttachment(prev, id, {
            livekitUrl: res.livekitUrl,
            roomName: res.livekitRoom,
            identity: `browser-${id}-${Date.now()}`,
            serverIdentity: res.livekitServerIdentity,
            debugLogging: debugForSessionId(id),
          }),
        );
        navigatePath(terminalPathForSessionId(id), "replace");
        const attachNew = nextPresentationFromAttach(terminalPresentation, "new");
        setTerminalPresentation(attachNew.presentation);
        if (attachNew.shouldPushTerminalRoute) {
          const target = terminalPathForSessionId(id);
          if (typeof window !== "undefined" && window.location.pathname !== target) {
            navigatePath(target, "push");
          }
        }
      } catch {
        try {
          const res = await client.resumeSession({ sessionToken, sessionId: id });
          if (cancelled || seq !== terminalDeepLinkSeqRef.current) return;
          setSessionAttachments((prev) =>
            addSessionAttachment(prev, res.sessionId, {
              livekitUrl: res.livekitUrl,
              roomName: res.livekitRoom,
              identity: `browser-${res.sessionId}-${Date.now()}`,
              serverIdentity: res.livekitServerIdentity,
              debugLogging: debugForSessionId(id),
            }),
          );
          navigatePath(terminalPathForSessionId(res.sessionId), "replace");
          const attachRe = nextPresentationFromAttach(terminalPresentation, "reconnect");
          setTerminalPresentation(attachRe.presentation);
        } catch (e) {
          if (!cancelled && seq === terminalDeepLinkSeqRef.current) {
            setError(e instanceof Error ? e.message : "Failed to open session");
          }
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [
    routePath,
    sessions,
    sessionsListHydrated,
    isAuthenticated,
    sessionToken,
    sessionAttachments,
    terminalRouteUnknown,
    client,
    debugForSessionId,
    navigatePath,
    terminalPresentation,
  ]);

  const shrinkTerminalPresentationToMini = useCallback(() => {
    setTerminalPresentation(
      applyDedicatedTerminalBackToMini({
        connectSessionCalls: 0,
        resumeSessionCalls: 0,
        disconnectCalls: 0,
      }).presentation,
    );
  }, []);

  useEffect(() => {
    setSessionAttachments((prev) => {
      if (prev.size === 0) return prev;
      let next: SessionAttachmentMap | null = null;
      for (const sessionId of prev.keys()) {
        const row = sessions.find((s) => s.sessionId === sessionId);
        if (row && !row.isActive) {
          if (!next) next = new Map(prev);
          next.delete(sessionId);
          console.info("[ConnectionScreen] prune attachment: session inactive in ListSessions", {
            sessionId,
          });
        }
      }
      if (!next) return prev;
      const pruned = next;
      queueMicrotask(() => {
        if (pruned.size === 0) {
          setTerminalPresentation("hidden");
          navigatePath("/", "replace");
        } else {
          const p = typeof window !== "undefined" ? window.location.pathname : "/";
          const focus = focusedSessionIdFromPathname(p, pruned);
          if (focus) {
            navigatePath(terminalPathForSessionId(focus), "replace");
          }
        }
      });
      return pruned;
    });
  }, [sessions, navigatePath]);

  const handleStartSession = async (projectId: string) => {
    const form = projectForms[projectId] ?? defaultProjectSessionForm(tools, effectiveAgents, daemons);
    if (!sessionToken || !form.toolPath || !projectId.trim() || !form.agent) return;
    setError(null);
    try {
      const res = await client.startSession({
        sessionToken,
        toolPath: form.toolPath,
        projectId: projectId.trim(),
        agent: form.agent,
        daemonInstanceId: form.daemonInstanceId,
        recipe: form.recipe,
      });
      console.info("[ConnectionScreen] startSession: new attachment", { sessionId: res.sessionId });
      setSessionAttachments((prev) =>
        addSessionAttachment(prev, res.sessionId, {
          livekitUrl: res.livekitUrl,
          roomName: res.livekitRoom,
          identity: `browser-${res.sessionId}-${Date.now()}`,
          serverIdentity: res.livekitServerIdentity,
          debugLogging: form.debugLogging,
        }),
      );
      const attach = nextPresentationFromAttach(terminalPresentation, "new");
      setTerminalPresentation(attach.presentation);
      if (attach.shouldPushTerminalRoute) {
        navigatePath(terminalPathForSessionId(res.sessionId), "push");
      } else {
        navigatePath(terminalPathForSessionId(res.sessionId), "replace");
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to start session");
    }
  };

  const handleCreateProject = async () => {
    if (!sessionToken || !newProjectName.trim() || !newProjectGitUrl.trim()) return;
    setError(null);
    try {
      await client.createProject({
        sessionToken,
        name: newProjectName.trim(),
        gitUrl: newProjectGitUrl.trim(),
        userRelativePath: newProjectUserRelativePath.trim(),
      });
      const res = await client.listProjects({ sessionToken });
      setProjects(res.projects);
      setNewProjectName("");
      setNewProjectGitUrl("");
      setNewProjectUserRelativePath("");
      setCreateProjectOpen(false);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to create project");
    }
  };

  const handleConnectSession = async (sessionId: string) => {
    if (!sessionToken) return;
    if (sessionAttachments.has(sessionId)) {
      console.debug("[ConnectionScreen] connectSession: already attached, focusing", { sessionId });
      navigatePath(terminalPathForSessionId(sessionId), "replace");
      return;
    }
    setError(null);
    try {
      const res = await client.connectSession({ sessionToken, sessionId });
      console.info("[ConnectionScreen] connectSession: new attachment", { sessionId });
      setSessionAttachments((prev) =>
        addSessionAttachment(prev, sessionId, {
          livekitUrl: res.livekitUrl,
          roomName: res.livekitRoom,
          identity: `browser-${sessionId}-${Date.now()}`,
          serverIdentity: res.livekitServerIdentity,
          debugLogging: debugForSessionId(sessionId),
        }),
      );
      const attach = nextPresentationFromAttach(terminalPresentation, "new");
      setTerminalPresentation(attach.presentation);
      if (attach.shouldPushTerminalRoute) {
        navigatePath(terminalPathForSessionId(sessionId), "push");
      } else {
        navigatePath(terminalPathForSessionId(sessionId), "replace");
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to connect to session");
    }
  };

  const handleResumeSession = async (sessionId: string) => {
    if (!sessionToken) return;
    if (sessionAttachments.has(sessionId)) {
      console.debug("[ConnectionScreen] resumeSession: already attached, focusing", { sessionId });
      navigatePath(terminalPathForSessionId(sessionId), "replace");
      return;
    }
    setError(null);
    try {
      const res = await client.resumeSession({ sessionToken, sessionId });
      // Resume makes the session active; list data may still say inactive until the next ListSessions.
      setSessions((prev) =>
        prev.map((s) =>
          s.sessionId === res.sessionId ? { ...s, isActive: true, status: "active" } : s
        )
      );
      console.info("[ConnectionScreen] resumeSession: new attachment", { sessionId: res.sessionId });
      setSessionAttachments((prev) =>
        addSessionAttachment(prev, res.sessionId, {
          livekitUrl: res.livekitUrl,
          roomName: res.livekitRoom,
          identity: `browser-${res.sessionId}-${Date.now()}`,
          serverIdentity: res.livekitServerIdentity,
          debugLogging: debugForSessionId(sessionId),
        }),
      );
      const attach = nextPresentationFromAttach(terminalPresentation, "reconnect");
      setTerminalPresentation(attach.presentation);
      if (attach.shouldPushTerminalRoute) {
        navigatePath(terminalPathForSessionId(res.sessionId), "push");
      } else {
        navigatePath(terminalPathForSessionId(res.sessionId), "replace");
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to resume session");
    }
  };

  const handleSignalSession = async (sessionId: string, signal: Signal) => {
    if (!sessionToken) return;
    setError(null);
    try {
      await client.signalSession({ sessionToken, sessionId, signal });
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to send signal");
    }
  };

  const handleDeleteSession = async (sessionId: string) => {
    if (!sessionToken) return;
    if (
      !window.confirm(
        "Delete this session? If the tool process is still running, it will be stopped first, then on-disk session data will be removed. This cannot be undone."
      )
    ) {
      return;
    }
    setError(null);
    try {
      await client.deleteSession(
        create(DeleteSessionRequestSchema, { sessionToken, sessionId }),
      );
      const res = await client.listSessions({ sessionToken });
      setSessions(res.sessions);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to delete session");
    }
  };

  const handleBulkDeleteSelectedSessions = useCallback(
    async (tableKey: string, selectedIds: string[]) => {
      if (!sessionToken || selectedIds.length === 0) {
        if (import.meta.env.DEV) {
          console.debug("[ConnectionScreen] bulk delete skipped", {
            tableKey,
            hasToken: Boolean(sessionToken),
            count: selectedIds.length,
          });
        }
        return;
      }
      const count = selectedIds.length;
      const msg = `Delete ${count} selected session${count === 1 ? "" : "s"}? If the tool process is still running, it will be stopped first, then on-disk session data will be removed. This cannot be undone.`;
      if (import.meta.env.DEV) {
        console.info("[ConnectionScreen] bulk delete confirm prompt", { tableKey, count });
      }
      if (!window.confirm(msg)) {
        if (import.meta.env.DEV) {
          console.debug("[ConnectionScreen] bulk delete cancelled");
        }
        return;
      }
      setError(null);
      try {
        for (let i = 0; i < selectedIds.length; i++) {
          const sessionId = selectedIds[i]!;
          if (import.meta.env.DEV) {
            console.debug("[ConnectionScreen] bulk deleteSession", {
              tableKey,
              index: i + 1,
              total: selectedIds.length,
            });
          }
          await client.deleteSession(
            create(DeleteSessionRequestSchema, { sessionToken, sessionId }),
          );
        }
        const res = await client.listSessions({ sessionToken });
        if (import.meta.env.DEV) {
          console.info("[ConnectionScreen] bulk delete listSessions refresh", {
            tableKey,
            listed: res.sessions.length,
          });
        }
        setSessions(res.sessions);
        setTableSessionSelections((prev) => ({ ...prev, [tableKey]: new Set() }));
      } catch (e) {
        const message = e instanceof Error ? e.message : "Failed to delete selected sessions";
        if (import.meta.env.DEV) {
          console.info("[ConnectionScreen] bulk delete failed", { tableKey, message });
        }
        setError(message);
        try {
          const res = await client.listSessions({ sessionToken });
          setSessions(res.sessions);
          const existingIds = new Set(res.sessions.map((s) => s.sessionId));
          setTableSessionSelections((prev) => {
            const current = prev[tableKey] ?? new Set<string>();
            const pruned = new Set([...current].filter((id) => existingIds.has(id)));
            return { ...prev, [tableKey]: pruned };
          });
        } catch {
          /* ignore secondary failure */
        }
      }
    },
    [client, sessionToken],
  );

  const primaryFloatingSessionAction = (sessionId: string) => {
    const docked =
      sessionAttachments.has(sessionId) &&
      focusedSessionId === sessionId &&
      (terminalPresentation === "overlay" || terminalPresentation === "mini");
    if (docked && terminalOverlayMinimized) {
      return {
        label: "Open" as const,
        onClick: () => setTerminalOverlayMinimized(false),
      };
    }
    if (docked && !terminalOverlayMinimized) {
      return {
        label: "Hide" as const,
        onClick: () => setTerminalOverlayMinimized(true),
      };
    }
    return {
      label: "Connect" as const,
      onClick: () => void handleConnectSession(sessionId),
    };
  };

  if (terminalPresentation === "full" && focusedSessionId) {
    const focusedAttachment = sessionAttachments.get(focusedSessionId);
    if (focusedAttachment) {
      return (
        <ConnectedTerminal
          livekitUrl={focusedAttachment.livekitUrl}
          roomName={focusedAttachment.roomName}
          identity={focusedAttachment.identity}
          serverIdentity={focusedAttachment.serverIdentity}
          debugLogging={focusedAttachment.debugLogging}
          terminalLayout="fullscreen"
          paneSessionLabel={sessionIdFirstSegment(focusedSessionId)}
          onBackToMini={shrinkTerminalPresentationToMini}
          onDisconnect={() => removeAttachmentForSession(focusedSessionId, "userDisconnectFullscreen")}
          onTerminate={() => void handleSignalSession(focusedSessionId, Signal.SIGTERM)}
          onRemoteSessionEnded={() =>
            removeAttachmentForSession(focusedSessionId, "remoteSessionEndedFullscreen")
          }
        />
      );
    }
  }

  if (!isAuthenticated) {
    return (
      <div className={screenShellClassName}>
        <h1>tddy-web</h1>
        <p className="mb-4 text-sm text-muted-foreground">
          Sign in with GitHub to access the terminal.
        </p>
        <GitHubLoginButton onClick={login} />
      </div>
    );
  }

  return (
    <div className={screenShellClassName}>
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div className="flex min-w-0 flex-wrap items-center gap-3">
          {onNavigate ? <DaemonNavMenu onNavigate={onNavigate} /> : null}
          <h1 className="text-2xl font-semibold">tddy-web</h1>
        </div>
        {user ? <UserAvatar user={user} onLogout={logout} /> : null}
      </div>
      <h2 className="mt-6 text-lg font-medium">Start or connect to a session</h2>

      {terminalRouteUnknown && (
        <div
          data-testid="terminal-route-unknown-session"
          className="mb-4 rounded-md border border-destructive/40 bg-destructive/5 p-4"
        >
          <p className="mb-3 text-sm text-foreground">Session not found or no longer available.</p>
          <Button
            type="button"
            variant="secondary"
            data-testid="terminal-route-unknown-session-home"
            onClick={() => {
              navigatePath("/", "replace");
              setTerminalRouteUnknown(false);
            }}
          >
            Back to sessions
          </Button>
        </div>
      )}

      {presenceReady && (
        <div
          data-testid="connected-participants-panel"
          style={{
            marginTop: 16,
            marginBottom: 16,
            border: "1px solid #ddd",
            borderRadius: 4,
            padding: 12,
          }}
        >
          <h3 style={{ marginTop: 0, fontSize: 16 }}>Connected participants</h3>
          <ParticipantList
            participants={participants}
            roomStatus={presenceStatus}
            connectionError={presenceError}
          />
        </div>
      )}

      <div className="my-4">
        <Button
          type="button"
          variant="outline"
          data-testid="toggle-create-project"
          onClick={() => setCreateProjectOpen((o) => !o)}
        >
          {createProjectOpen ? "Hide" : "Create project"}
        </Button>
      </div>

      {createProjectOpen && (
        <div
          data-testid="create-project-form"
          style={{
            border: "1px solid #ccc",
            borderRadius: 4,
            padding: 12,
            marginBottom: 16,
          }}
        >
          <label style={labelStyle} htmlFor="new-project-name">
            Project name
          </label>
          <input
            id="new-project-name"
            data-testid="new-project-name"
            type="text"
            placeholder="my-app"
            value={newProjectName}
            onChange={(e) => setNewProjectName(e.target.value)}
            style={inputStyle}
          />
          <label style={labelStyle} htmlFor="new-project-git-url">
            Git URL
          </label>
          <input
            id="new-project-git-url"
            data-testid="new-project-git-url"
            type="text"
            placeholder="https://github.com/org/repo.git"
            value={newProjectGitUrl}
            onChange={(e) => setNewProjectGitUrl(e.target.value)}
            style={inputStyle}
          />
          <label style={labelStyle} htmlFor="new-project-user-relative-path">
            Path under home (optional)
          </label>
          <input
            id="new-project-user-relative-path"
            data-testid="new-project-user-relative-path"
            type="text"
            placeholder="e.g. Code/my-app or ~/Code/my-app — leave empty for default clone path"
            value={newProjectUserRelativePath}
            onChange={(e) => setNewProjectUserRelativePath(e.target.value)}
            style={inputStyle}
          />
          <Button
            type="button"
            data-testid="create-project-submit"
            onClick={handleCreateProject}
            disabled={!newProjectName.trim() || !newProjectGitUrl.trim()}
          >
            Create
          </Button>
        </div>
      )}

      {error && (
        <div data-testid="connection-error" style={{ color: "#c00", marginTop: 12 }}>
          {error}
        </div>
      )}

      <h3 style={{ marginTop: 24, fontSize: 16 }}>Projects</h3>
      {projects.length === 0 ? (
        <p style={{ fontSize: 14, color: "#666" }}>No projects yet. Create one above.</p>
      ) : (
        projects.map((p) => {
          const projectSessions = sortedSessionsForProjectTable(sessions, p, projects);
          const tableKey = p.projectId;
          const selectedSet = tableSessionSelections[tableKey] ?? new Set<string>();
          const allProjectSessionIds = projectSessions.map((s) => s.sessionId);
          return (
            <details
              key={p.projectId}
              data-testid={`project-accordion-${p.projectId}`}
              style={{ marginBottom: 12, border: "1px solid #ddd", borderRadius: 4, padding: 8 }}
              open
            >
              <summary style={{ cursor: "pointer", fontWeight: 600 }}>
                {p.name}{" "}
                <span style={{ fontWeight: 400, fontSize: 12, color: "#666" }}> {p.gitUrl}</span>
              </summary>
              <p style={{ fontSize: 12, color: "#555", marginTop: 8 }}>{p.mainRepoPath}</p>
              <ProjectSessionOptions
                projectId={p.projectId}
                tools={tools}
                agents={effectiveAgents}
                daemons={daemons}
                form={
                  projectForms[p.projectId] ?? defaultProjectSessionForm(tools, effectiveAgents, daemons)
                }
                onChange={(patch) => updateProjectForm(p.projectId, patch)}
                startSessionButton={
                  <Button
                    type="button"
                    data-testid={`start-session-${p.projectId}`}
                    onClick={() => handleStartSession(p.projectId)}
                    disabled={
                      loading ||
                      !(projectForms[p.projectId] ?? defaultProjectSessionForm(tools, effectiveAgents, daemons))
                        .toolPath ||
                      !(projectForms[p.projectId] ?? defaultProjectSessionForm(tools, effectiveAgents, daemons))
                        .agent
                    }
                  >
                    Start New Session
                  </Button>
                }
              />
              {projectSessions.length === 0 ? (
                <p style={{ fontSize: 14, color: "#666" }}>No sessions for this project.</p>
              ) : (
                <>
                  <div
                    className="mt-3 w-full min-w-0"
                    data-testid={`sessions-table-${p.projectId}`}
                  >
                    <div className="flex justify-end">
                      <Button
                        type="button"
                        variant="destructive"
                        size="sm"
                        disabled={selectedSet.size === 0}
                        data-testid={`bulk-delete-selected-${tableKey}`}
                        onClick={() =>
                          void handleBulkDeleteSelectedSessions(tableKey, Array.from(selectedSet))
                        }
                      >
                        Delete selected
                      </Button>
                    </div>
                    <Table className="mt-3 w-full min-w-0">
                  <TableHeader>
                    <TableRow>
                      <TableHead className="w-10">
                        <SessionTableSelectAllCheckbox
                          selectedCount={selectedSet.size}
                          totalRows={projectSessions.length}
                          dataTestId={`session-table-select-all-${tableKey}`}
                          ariaLabel="Select all sessions in this table"
                          onToggle={() => {
                            const next = toggleSelectAllForTable(allProjectSessionIds, selectedSet);
                            setTableSessionSelections((prev) => ({ ...prev, [tableKey]: next }));
                          }}
                        />
                      </TableHead>
                      <TableHead>ID</TableHead>
                      <TableHead>Date</TableHead>
                      <TableHead>Status</TableHead>
                      <TableHead>Host</TableHead>
                      <TableHead>PID</TableHead>
                      <TableHead>Goal</TableHead>
                      <TableHead>Workflow</TableHead>
                      <TableHead>Elapsed</TableHead>
                      <TableHead>Agent</TableHead>
                      <TableHead>Model</TableHead>
                      <TableHead>Actions</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {projectSessions.map((s) => {
                      const sessionAction = primaryFloatingSessionAction(s.sessionId);
                      const pendingElicitation = s.pendingElicitation === true;
                      return (
                      <TableRow
                        key={s.sessionId}
                        data-pending-elicitation={pendingElicitation ? "true" : "false"}
                      >
                        <TableCell className="w-10 align-middle">
                          <input
                            type="checkbox"
                            data-testid={`session-row-select-${s.sessionId}`}
                            aria-label={`Select session ${s.sessionId}`}
                            checked={selectedSet.has(s.sessionId)}
                            onChange={() => {
                              const next = toggleRowInTableSelection(selectedSet, s.sessionId);
                              setTableSessionSelections((prev) => ({ ...prev, [tableKey]: next }));
                            }}
                            className="size-4 shrink-0 rounded border border-input accent-primary"
                          />
                        </TableCell>
                        <TableCell>
                          <span className="inline-flex flex-wrap items-center gap-2">
                            <span>{sessionIdFirstSegment(s.sessionId)}</span>
                            {pendingElicitation ? (
                              <span
                                role="status"
                                className="inline-flex shrink-0 items-center rounded border border-amber-300 bg-amber-50 px-1.5 py-0.5 text-xs font-medium text-amber-950"
                                aria-label="Session needs your input or approval"
                                data-testid={`elicitation-indicator-${s.sessionId}`}
                              >
                                Input needed
                              </span>
                            ) : null}
                          </span>
                        </TableCell>
                        <TableCell>{formatSessionCreatedAt(s.createdAt)}</TableCell>
                        <TableCell>{s.status}</TableCell>
                        <TableCell>{s.daemonInstanceId || "—"}</TableCell>
                        <TableCell>{sessionPidDisplay(s.isActive, s.pid)}</TableCell>
                        <SessionWorkflowStatusCells session={s} />
                        <TableCell>
                          <span className="inline-flex flex-wrap items-center gap-2">
                            {s.isActive ? (
                              <>
                                <Button
                                  type="button"
                                  size="sm"
                                  data-testid={`connect-${s.sessionId}`}
                                  onClick={() => sessionAction.onClick()}
                                >
                                  {sessionAction.label}
                                </Button>
                                <SignalDropdown
                                  sessionId={s.sessionId}
                                  onSignal={handleSignalSession}
                                />
                                <SessionDeleteButton
                                  sessionId={s.sessionId}
                                  onDelete={handleDeleteSession}
                                />
                              </>
                            ) : (
                              <InactiveSessionActions
                                sessionId={s.sessionId}
                                onResume={handleResumeSession}
                                onDelete={handleDeleteSession}
                              />
                            )}
                            <SessionMoreActionsMenu
                              sessionId={s.sessionId}
                              onShowFiles={() => setWorkflowFilesSessionId(s.sessionId)}
                            />
                          </span>
                        </TableCell>
                      </TableRow>
                      );
                    })}
                  </TableBody>
                </Table>
                  </div>
                </>
              )}
            </details>
          );
        })
      )}

      {orphanSessions.length > 0 && (
        <>
          <h3 style={{ marginTop: 24, fontSize: 16 }}>Other sessions</h3>
          <p style={{ fontSize: 13, color: "#666" }}>Sessions not associated with a listed project.</p>
          <label
            style={{ ...labelStyle, display: "flex", alignItems: "center", gap: 8, marginTop: 8 }}
            htmlFor="orphan-session-debug"
          >
            <input
              id="orphan-session-debug"
              data-testid="orphan-session-debug"
              type="checkbox"
              checked={orphanSessionDebug}
              onChange={(e) => setOrphanSessionDebug(e.target.checked)}
            />
            Debug logging (browser terminal, Connect / Resume below)
          </label>
          <div className="mt-3 w-full min-w-0" data-testid="sessions-table-orphan">
            <div className="flex justify-end">
              <Button
                type="button"
                variant="destructive"
                size="sm"
                disabled={orphanSelectedSet.size === 0}
                data-testid={`bulk-delete-selected-${orphanTableKey}`}
                onClick={() =>
                  void handleBulkDeleteSelectedSessions(orphanTableKey, Array.from(orphanSelectedSet))
                }
              >
                Delete selected
              </Button>
            </div>
            <Table className="mt-3 w-full min-w-0">
            <TableHeader>
              <TableRow>
                <TableHead className="w-10">
                  <SessionTableSelectAllCheckbox
                    selectedCount={orphanSelectedSet.size}
                    totalRows={orphanSessions.length}
                    dataTestId={`session-table-select-all-${orphanTableKey}`}
                    ariaLabel="Select all sessions in the other sessions table"
                    onToggle={() => {
                      const next = toggleSelectAllForTable(orphanAllSessionIds, orphanSelectedSet);
                      setTableSessionSelections((prev) => ({ ...prev, [orphanTableKey]: next }));
                    }}
                  />
                </TableHead>
                <TableHead>ID</TableHead>
                <TableHead>Date</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Host</TableHead>
                <TableHead>PID</TableHead>
                <TableHead>Goal</TableHead>
                <TableHead>Workflow</TableHead>
                <TableHead>Elapsed</TableHead>
                <TableHead>Agent</TableHead>
                <TableHead>Model</TableHead>
                <TableHead>Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {orphanSessions.map((s) => {
                const orphanSessionAction = primaryFloatingSessionAction(s.sessionId);
                const pendingElicitation = s.pendingElicitation === true;
                return (
                <TableRow
                  key={s.sessionId}
                  data-pending-elicitation={pendingElicitation ? "true" : "false"}
                >
                  <TableCell className="w-10 align-middle">
                    <input
                      type="checkbox"
                      data-testid={`session-row-select-${s.sessionId}`}
                      aria-label={`Select session ${s.sessionId}`}
                      checked={orphanSelectedSet.has(s.sessionId)}
                      onChange={() => {
                        const next = toggleRowInTableSelection(orphanSelectedSet, s.sessionId);
                        setTableSessionSelections((prev) => ({ ...prev, [orphanTableKey]: next }));
                      }}
                      className="size-4 shrink-0 rounded border border-input accent-primary"
                    />
                  </TableCell>
                  <TableCell>
                    <span className="inline-flex flex-wrap items-center gap-2">
                      <span>{sessionIdFirstSegment(s.sessionId)}</span>
                      {pendingElicitation ? (
                        <span
                          role="status"
                          className="inline-flex shrink-0 items-center rounded border border-amber-300 bg-amber-50 px-1.5 py-0.5 text-xs font-medium text-amber-950"
                          aria-label="Session needs your input or approval"
                          data-testid={`elicitation-indicator-${s.sessionId}`}
                        >
                          Input needed
                        </span>
                      ) : null}
                    </span>
                  </TableCell>
                  <TableCell>{formatSessionCreatedAt(s.createdAt)}</TableCell>
                  <TableCell>{s.status}</TableCell>
                  <TableCell>{s.daemonInstanceId || "—"}</TableCell>
                  <TableCell>{sessionPidDisplay(s.isActive, s.pid)}</TableCell>
                  <SessionWorkflowStatusCells session={s} />
                  <TableCell>
                    <span className="inline-flex flex-wrap items-center gap-2">
                      {s.isActive ? (
                        <>
                          <Button
                            type="button"
                            size="sm"
                            data-testid={`connect-${s.sessionId}`}
                            onClick={() => orphanSessionAction.onClick()}
                          >
                            {orphanSessionAction.label}
                          </Button>
                          <SignalDropdown
                            sessionId={s.sessionId}
                            onSignal={handleSignalSession}
                          />
                          <SessionDeleteButton
                            sessionId={s.sessionId}
                            onDelete={handleDeleteSession}
                          />
                        </>
                      ) : (
                        <InactiveSessionActions
                          sessionId={s.sessionId}
                          onResume={handleResumeSession}
                          onDelete={handleDeleteSession}
                        />
                      )}
                      <SessionMoreActionsMenu
                        sessionId={s.sessionId}
                        onShowFiles={() => setWorkflowFilesSessionId(s.sessionId)}
                      />
                    </span>
                  </TableCell>
                </TableRow>
                );
              })}
            </TableBody>
          </Table>
          </div>
        </>
      )}

      {sessionToken && workflowFilesSessionId ? (
        <SessionWorkflowFilesModal
          open
          onClose={() => setWorkflowFilesSessionId(null)}
          sessionId={workflowFilesSessionId}
          sessionToken={sessionToken}
          client={client}
        />
      ) : null}

      {sessionAttachments.size > 0 &&
      (terminalPresentation === "overlay" || terminalPresentation === "mini") ? (
        <div
          data-testid="terminal-reconnect-overlay-root"
          style={terminalOverlayMinimized ? { display: "none" } : undefined}
          aria-hidden={terminalOverlayMinimized}
        >
          {Array.from(sessionAttachments.entries()).map(([sessionId, att], index) => (
            <div
              key={sessionId}
              data-testid={connectionAttachedTerminalTestId(sessionId)}
              className="relative"
              style={{ zIndex: 50 + index, marginBottom: 12 }}
            >
              <ConnectedTerminal
                livekitUrl={att.livekitUrl}
                roomName={att.roomName}
                identity={att.identity}
                serverIdentity={att.serverIdentity}
                debugLogging={att.debugLogging}
                terminalLayout={terminalPresentation === "overlay" ? "overlay" : "mini"}
                paneSessionLabel={sessionIdFirstSegment(sessionId)}
                onExpandTerminal={() => {
                  navigatePath(terminalPathForSessionId(sessionId), "push");
                  setTerminalPresentation("full");
                }}
                onMinimizePane={() => setTerminalOverlayMinimized(true)}
                onDisconnect={() => removeAttachmentForSession(sessionId, "userDisconnectOverlay")}
                onTerminate={() => void handleSignalSession(sessionId, Signal.SIGTERM)}
                onRemoteSessionEnded={() =>
                  removeAttachmentForSession(sessionId, "remoteSessionEndedOverlay")
                }
              />
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}
