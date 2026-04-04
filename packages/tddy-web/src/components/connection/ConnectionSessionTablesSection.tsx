import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Trash2 } from "lucide-react";
import {
  Signal,
  type AgentInfo,
  type EligibleDaemonEntry,
  type ProjectEntry,
  type SessionEntry,
  type ToolInfo,
} from "../../gen/connection_pb";
import { buildAgentSelectOptionsFromRpc } from "./agentOptions";
import { defaultProjectSessionForm, type ProjectSessionForm } from "./projectSessionForm";
import {
  SESSION_TABLE_COLUMN_HEADER_LABEL,
  SESSION_TABLE_COLUMN_KEYS_IN_TABLE_ORDER,
  sessionTableColumnHeaderTestId,
  sessionTableResponsiveContainerCss,
} from "./sessionTableColumns";

const SESSION_TABLE_RESPONSIVE_CSS = sessionTableResponsiveContainerCss();
import {
  isSessionOrphan,
  sortedSessionsForProjectTable,
} from "../../utils/sessionProjectTable";
import { sortSessionsForDisplay } from "../../utils/sessionSort";
import { SessionWorkflowStatusCells } from "../SessionWorkflowStatusCells";
import { SessionMoreActionsMenu } from "../session/SessionMoreActionsMenu";
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
  formatSessionCreatedAt,
  sessionIdFirstSegment,
  sessionPidDisplay,
} from "../../utils/sessionDisplay";

const sessionControlSelectClassName =
  "box-border w-full min-w-[9rem] max-w-[16rem] rounded-md border border-input bg-background px-2 py-1.5 text-sm text-foreground shadow-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring";

const orphanDebugLabelStyle = {
  display: "flex" as const,
  alignItems: "center" as const,
  gap: 8,
  marginTop: 8,
  marginBottom: 4,
  fontWeight: 500,
};

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
            <option value="bugfix">Bugfix (reproduce → fix)</option>
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

export type ConnectionSessionTablesSectionProps = {
  projects: ProjectEntry[];
  sessions: SessionEntry[];
  tools: ToolInfo[];
  agents: AgentInfo[];
  daemons: EligibleDaemonEntry[];
  projectForms: Record<string, ProjectSessionForm>;
  loading: boolean;
  orphanSessionDebug: boolean;
  onOrphanSessionDebugChange: (checked: boolean) => void;
  onUpdateProjectForm: (projectId: string, patch: Partial<ProjectSessionForm>) => void;
  onStartSession: (projectId: string) => void | Promise<void>;
  onConnectSession: (sessionId: string) => void | Promise<void>;
  onResumeSession: (sessionId: string) => void | Promise<void>;
  onSignalSession: (sessionId: string, signal: Signal) => void | Promise<void>;
  onDeleteSession: (sessionId: string) => void | Promise<void>;
  onShowWorkflowFiles: (sessionId: string) => void;
};

export function ConnectionSessionTablesSection({
  projects,
  sessions,
  tools,
  agents,
  daemons,
  projectForms,
  loading,
  orphanSessionDebug,
  onOrphanSessionDebugChange,
  onUpdateProjectForm,
  onStartSession,
  onConnectSession,
  onResumeSession,
  onSignalSession,
  onDeleteSession,
  onShowWorkflowFiles,
}: ConnectionSessionTablesSectionProps) {
  const orphanSessions = useMemo(
    () => sortSessionsForDisplay(sessions.filter((s) => isSessionOrphan(s, projects))),
    [sessions, projects],
  );

  return (
    <div
      className="min-w-0 w-full max-w-full"
      style={{ containerType: "inline-size", containerName: "session-tables" }}
      data-testid="session-tables-layout-host"
    >
      <style dangerouslySetInnerHTML={{ __html: SESSION_TABLE_RESPONSIVE_CSS }} />
      <h3 style={{ marginTop: 24, fontSize: 16 }}>Projects</h3>
      {projects.length === 0 ? (
        <p style={{ fontSize: 14, color: "#666" }}>No projects yet. Create one above.</p>
      ) : (
        projects.map((p) => {
          const projectSessions = sortedSessionsForProjectTable(sessions, p, projects);
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
                agents={agents}
                daemons={daemons}
                form={projectForms[p.projectId] ?? defaultProjectSessionForm(tools, agents, daemons)}
                onChange={(patch) => onUpdateProjectForm(p.projectId, patch)}
                startSessionButton={
                  <Button
                    type="button"
                    data-testid={`start-session-${p.projectId}`}
                    onClick={() => void onStartSession(p.projectId)}
                    disabled={
                      loading ||
                      !(projectForms[p.projectId] ?? defaultProjectSessionForm(tools, agents, daemons))
                        .toolPath ||
                      !(projectForms[p.projectId] ?? defaultProjectSessionForm(tools, agents, daemons))
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
                <Table
                  className="mt-3 w-full min-w-0 overflow-x-auto"
                  data-testid={`sessions-table-${p.projectId}`}
                >
                  <TableHeader>
                    <TableRow>
                      {SESSION_TABLE_COLUMN_KEYS_IN_TABLE_ORDER.map((col) => (
                        <TableHead
                          key={col}
                          data-session-col={col}
                          data-testid={sessionTableColumnHeaderTestId(col)}
                        >
                          {SESSION_TABLE_COLUMN_HEADER_LABEL[col]}
                        </TableHead>
                      ))}
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {projectSessions.map((s) => (
                      <TableRow key={s.sessionId}>
                        <TableCell data-session-col="id">
                          {sessionIdFirstSegment(s.sessionId)}
                        </TableCell>
                        <TableCell data-session-col="date">
                          {formatSessionCreatedAt(s.createdAt)}
                        </TableCell>
                        <TableCell data-session-col="status">{s.status}</TableCell>
                        <TableCell data-session-col="host">
                          {s.daemonInstanceId || "—"}
                        </TableCell>
                        <TableCell data-session-col="pid">
                          {sessionPidDisplay(s.isActive, s.pid)}
                        </TableCell>
                        <SessionWorkflowStatusCells session={s} />
                        <TableCell data-session-col="actions">
                          <span className="inline-flex flex-wrap items-center gap-2">
                            {s.isActive ? (
                              <>
                                <Button
                                  type="button"
                                  size="sm"
                                  data-testid={`connect-${s.sessionId}`}
                                  onClick={() => void onConnectSession(s.sessionId)}
                                >
                                  Connect
                                </Button>
                                <SignalDropdown sessionId={s.sessionId} onSignal={onSignalSession} />
                                <SessionDeleteButton
                                  sessionId={s.sessionId}
                                  onDelete={onDeleteSession}
                                />
                              </>
                            ) : (
                              <InactiveSessionActions
                                sessionId={s.sessionId}
                                onResume={onResumeSession}
                                onDelete={onDeleteSession}
                              />
                            )}
                            <SessionMoreActionsMenu
                              sessionId={s.sessionId}
                              onShowFiles={() => onShowWorkflowFiles(s.sessionId)}
                            />
                          </span>
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
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
            style={orphanDebugLabelStyle}
            htmlFor="orphan-session-debug"
          >
            <input
              id="orphan-session-debug"
              data-testid="orphan-session-debug"
              type="checkbox"
              checked={orphanSessionDebug}
              onChange={(e) => onOrphanSessionDebugChange(e.target.checked)}
            />
            Debug logging (browser terminal, Connect / Resume below)
          </label>
          <Table className="mt-3 w-full min-w-0 overflow-x-auto" data-testid="sessions-table-orphan">
            <TableHeader>
              <TableRow>
                {SESSION_TABLE_COLUMN_KEYS_IN_TABLE_ORDER.map((col) => (
                  <TableHead
                    key={col}
                    data-session-col={col}
                    data-testid={sessionTableColumnHeaderTestId(col)}
                  >
                    {SESSION_TABLE_COLUMN_HEADER_LABEL[col]}
                  </TableHead>
                ))}
              </TableRow>
            </TableHeader>
            <TableBody>
              {orphanSessions.map((s) => (
                <TableRow key={s.sessionId}>
                  <TableCell data-session-col="id">
                    {sessionIdFirstSegment(s.sessionId)}
                  </TableCell>
                  <TableCell data-session-col="date">
                    {formatSessionCreatedAt(s.createdAt)}
                  </TableCell>
                  <TableCell data-session-col="status">{s.status}</TableCell>
                  <TableCell data-session-col="host">
                    {s.daemonInstanceId || "—"}
                  </TableCell>
                  <TableCell data-session-col="pid">
                    {sessionPidDisplay(s.isActive, s.pid)}
                  </TableCell>
                  <SessionWorkflowStatusCells session={s} />
                  <TableCell data-session-col="actions">
                    <span className="inline-flex flex-wrap items-center gap-2">
                      {s.isActive ? (
                        <>
                          <Button
                            type="button"
                            size="sm"
                            data-testid={`connect-${s.sessionId}`}
                            onClick={() => void onConnectSession(s.sessionId)}
                          >
                            Connect
                          </Button>
                          <SignalDropdown sessionId={s.sessionId} onSignal={onSignalSession} />
                          <SessionDeleteButton sessionId={s.sessionId} onDelete={onDeleteSession} />
                        </>
                      ) : (
                        <InactiveSessionActions
                          sessionId={s.sessionId}
                          onResume={onResumeSession}
                          onDelete={onDeleteSession}
                        />
                      )}
                      <SessionMoreActionsMenu
                        sessionId={s.sessionId}
                        onShowFiles={() => onShowWorkflowFiles(s.sessionId)}
                      />
                    </span>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </>
      )}
    </div>
  );
}
