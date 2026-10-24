/**
 * `SessionMainPane`'s contract with its caller: the services it is given, the session state it
 * renders and the callbacks it answers on. Lifted out of `SessionMainPane.tsx` unchanged.
 */

import type { Client } from "@connectrpc/connect";
import type { Room } from "livekit-client";
import type { SessionService, SessionEntry } from "../../gen/session_pb";
import type { ProjectService, ProjectEntry } from "../../gen/project_pb";
import type { AccountsService } from "../../gen/accounts_pb";
import type { CatalogService } from "../../gen/catalog_pb";
import type { ExecToolService } from "../../gen/exec_tools_pb";
import type { PrStackService } from "../../gen/pr_stack_pb";
import type { ActivityService } from "../../gen/activity_pb";
import type { SessionAgentService } from "../../gen/session_agents_pb";
import type { SessionFilesService } from "../../gen/session_files_pb";
import type { TerminalSessionService } from "../../gen/terminal_session_pb";
import type { CodeNavigationService } from "../../gen/code_navigation_pb";
import type { WorktreeService } from "../../gen/worktree_pb";
import type { TaskService } from "../../gen/tasks_pb";
import type { SessionAttachmentState } from "./useSessionAttachment";
import type { SessionAttachmentHint } from "../../rpc/connections/session";
import type { HostConnection } from "../../rpc/connections/types";
import type { SessionMetadata } from "../../lib/sessionParticipantMetadata";
import type { InspectorDrawerState } from "./SessionInspectorDrawer";
import type { ToolShortcutDef } from "../../lib/toolShortcuts";
import type { ByteDelta, SessionRuntimeState } from "./sessionRuntimeRegistry";

type ConnectionClient = Client<typeof SessionService>;
type ProjectClient = Client<typeof ProjectService>;
type CatalogClient = Client<typeof CatalogService>;
type AccountsClient = Client<typeof AccountsService>;
type ExecToolClient = Client<typeof ExecToolService>;
type PrStackClient = Client<typeof PrStackService>;
type TaskClient = Client<typeof TaskService>;
type SessionFilesClient = Client<typeof SessionFilesService>;
export type SessionAgentClient = Client<typeof SessionAgentService>;
type ActivityClient = Client<typeof ActivityService>;
type TerminalClient = Client<typeof TerminalSessionService>;
type WorktreeClient = Client<typeof WorktreeService>;
type CodeNavigationClient = Client<typeof CodeNavigationService>;

export interface SessionMainPaneProps {
  selectedSession: SessionEntry | null;
  attachment: SessionAttachmentState;
  /** How the attached session is reached — passed to the custom workflow views, whose chat panels
   *  join the session's room as their own participant. `null` unless a room-backed session is
   *  attached. */
  attachmentHint?: SessionAttachmentHint | null;
  inspectorState: InspectorDrawerState;
  onToggleInspector: () => void;
  onInspectorClose: () => void;
  onInspectorExpand: () => void;
  onInspectorRestore: () => void;
  onResume: (sessionId: string) => void;
  onDelete: (sessionId: string) => void;
  onTerminate: (sessionId: string) => void;
  // Create session mode
  isCreating?: boolean;
  client?: ConnectionClient;
  /** `project.ProjectService` on the same host as `client` — create form project registry reads. */
  projectClient?: ProjectClient;
  /** The task service on the same host as `client`: the create form cancels a start in progress through it. */
  taskClient?: TaskClient;
  /**
   * The worktree service on the same host as `client` — the Code pane, the inspector's Worktree tab
   * and the create form's host-document picker all read worktrees through it. Absent for the same
   * reason `client` can be: no daemon is reachable yet.
   */
  worktreeClient?: WorktreeClient;
  /**
   * The code-navigation service on the same host as `worktreeClient` — definition, references and
   * hover for the Code pane's preview. Absent, the preview stays read-only text.
   */
  codeNavigationClient?: CodeNavigationClient;
  /**
   * The terminal service on the same host as `client` — each runtime claims its control lease and
   * carries its host-served terminal I/O over it. Absent for the same reason `client` can be: no
   * daemon is reachable yet.
   */
  terminalClient?: TerminalClient;
  /**
   * The session-files service on the same host as `client` — the inspector's Files tab and the
   * create form's attachment staging read and write through it. Absent for the same reason `client`
   * can be: no daemon is reachable yet.
   */
  sessionFilesClient?: SessionFilesClient;
  /**
   * The session-agent service on the same host as `client` — the Add-agent flow attaches through
   * it and each runtime's conversation panes talk to their agent over it. `#unbundle` node 7 took
   * the roster and conversation RPCs out of `session.SessionService`, so they need their own
   * client; absent for the same reason `client` can be.
   */
  sessionAgentClient?: SessionAgentClient;
  /**
   * The activity service on the same host as `client` — the ACP-replay overlay and the Activities
   * view read their transcript through it. Node 7 took those RPCs out too.
   */
  activityClient?: ActivityClient;
  /** `catalog.CatalogService` on the same host as `client` — create form and agent pickers. */
  catalogClient?: CatalogClient;
  /** `accounts.AccountsService` — the create form lists the person's provider accounts through it. */
  accountsClient?: AccountsClient;
  /** `exec_tools.ExecToolService` on the same host as `client` — inspector Tools tab. */
  execToolClient?: ExecToolClient;
  /** `pr_stack.PrStackService` on the same host as `client` — PR-stack workflow view. */
  prStackClient?: PrStackClient;
  /** The connection to the daemon that owns the selected session — a runtime attaches its spawned
   *  child conversations over it, and the inspector's media tabs are gated on it. `null` until a
   *  host is reachable.
   *
   *  Required, with no default, for the reason `SessionInspectorDrawer.hostConnection` (which this
   *  feeds) and `InspectorTabs.mediaAvailable` are: the only possible default is `null`, `null`
   *  hides the VNC and Screen Sharing tabs, and so a call site that forgot the prop would lose them
   *  silently — the pane would look exactly like one whose host cannot carry a track. */
  host: HostConnection | null;
  sessionToken?: string;
  onCancelCreate?: () => void;
  onSessionCreated?: (sessionId: string) => void;
  /** LiveKit room for the connected session — used by the VNC / screen-sharing overlay. */
  room?: Room | null;
  /** Shortcut presets for the connected session — shown as the mobile shortcut overlay. */
  mobileShortcuts?: ToolShortcutDef[];
  /** Fired when a custom workflow view (e.g. PrStackScreen) spawns a child session. */
  onChildSessionStarted?: (entry: {
    sessionId: string;
    recipe: string;
    orchestratorSessionId: string;
    projectId: string;
  }) => void;
  /** Fired when the operator clicks a subagent session's "Switch" in the Inspector's Agents tab —
   *  the parent selects that session in the drawer (focuses its runtime). No transport of its own. */
  onSwitchPeer?: (sessionId: string) => void;
  /** Inspector I/O traffic (req 5 dual source): live runtime counters for active sessions,
   *  daemon-sourced `SessionEntry` fields for inactive / non-LiveKit sessions. */
  traffic?: { bytesIn: number; bytesOut: number; lastDataReceivedAt: number | null } | null;
  /** Project registry — used to resolve an unscoped session's project (empty `projectId`) from its
   *  `repoPath` before the worktree Code pane's RPCs, which require a non-empty `project_id`. */
  projects?: ReadonlyArray<ProjectEntry>;
  /** Attached runtimes — one mounted terminal per entry (focused visible, others hidden). */
  runtimes?: ReadonlyArray<SessionRuntimeState>;
  /** The full drawer session list — passed to each runtime so it can render its spawned child
   *  conversations (`orchestratorSessionId === session`) as tabs. */
  sessions?: ReadonlyArray<SessionEntry>;
  /** The focused runtime's session id (visible); others are `display:none` but stay mounted. */
  focusedRuntimeId?: string | null;
  /** Register a session's Agent-terminal text-insert (for the inspector Files-tab click/tap route). */
  onSessionRegisterInsert?: (sessionId: string, insertInput: (text: string) => void) => void;
  /** Insert an uploaded file's host path into the focused session's terminal (Files tab → Insert /
   *  tap), closing the inspector. */
  onInsertPathIntoTerminal?: (hostPath: string) => void;
  /** Evict a session's runtime terminal (e.g. remote session ended). */
  onSessionDisconnect?: (sessionId: string) => void;
  /** Fold a session's terminal I/O bytes into its runtime counters (inspector I/O meter). */
  onSessionBytes?: (sessionId: string, delta: ByteDelta) => void;
  /** Lazy builder for a session-scoped `exec_tools.ExecToolService` client (session-participant
   *  routing) — used by the inspector's Tools tab. */
  buildSessionExecToolClient?: () => ExecToolClient | null;
  /** Lazy builder for a session-scoped `activity.ActivityService` client. Separate from
   *  `buildSessionExecToolClient` because `tddy-coder`'s session participant serves the replay and activity
   *  families at their own coordinate since `#unbundle` node 7 — one builder could only return one
   *  service's client, and the transcript must come off the session's own process when it has one. */
  buildSessionActivityClient?: () => ActivityClient | null;
  /** The `session` metadata block each live participant publishes, keyed by session id — passed to
   *  the custom workflow view, where the PR-Stack screen joins planned nodes to cross-host child
   *  sessions with it (D37, D38). */
  sessionMetadataBySessionId?: ReadonlyMap<string, SessionMetadata>;
}
