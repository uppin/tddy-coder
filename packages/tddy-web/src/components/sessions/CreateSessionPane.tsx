import React, { useEffect, useState } from "react";
import { flushSync } from "react-dom";
import type { Client } from "@connectrpc/connect";
import type { AgentInfo, ConnectionService, ProjectEntry, SessionEntry, SubagentInfo, ToolInfo } from "../../gen/connection_pb";
import { prStackOrchestrators } from "../../utils/stackParents";
import { useDaemons, useSelectedDaemon } from "../../rpc/selectedDaemon";
import { useAgentModels } from "../../rpc/useAgentModels";
import { Button } from "../ui/button";

/** Pseudo-agent key used to fetch the claude-cli session type's model catalog. */
const CLAUDE_CLI_AGENT = "claude-cli";

const WORKFLOW_RECIPES = [
  "tdd",
  "tdd-small",
  "bugfix",
  "free-prompting",
  "grill-me",
  "review",
  "merge-pr",
  "pr-stack",
] as const;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type ConnectionClient = Client<typeof ConnectionService>;

type SessionType = "tool" | "claude-cli";
type BranchIntent = "new_branch_from_base" | "work_on_selected_branch";

/**
 * Optional pre-fill for the form's fields. Used when the pane is opened from a context that already
 * knows what the session should look like (e.g. the PR-stack "Start session" flow pre-fills the
 * branch, prompt, and stack parent). Any field left unset keeps the form's own default.
 */
export type CreateSessionInitialValues = Partial<{
  sessionType: SessionType;
  projectId: string;
  recipe: string;
  model: string;
  permissionMode: string;
  stackParent: string;
  branchIntent: BranchIntent;
  newBranchName: string;
  initialPrompt: string;
  daemonInstanceId: string;
}>;

export interface CreateSessionPaneProps {
  client: ConnectionClient;
  sessionToken: string;
  onCancel: () => void;
  onCreated: (sessionId: string) => void;
  initialValues?: CreateSessionInitialValues;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const inputClass =
  "w-full rounded-md border border-input bg-background px-3 py-1.5 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring";

const labelClass = "block text-sm mb-1 text-muted-foreground";

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function CreateSessionPane({
  client,
  sessionToken,
  onCancel,
  onCreated,
  initialValues,
}: CreateSessionPaneProps) {
  const daemons = useDaemons();
  const { selectedInstanceId } = useSelectedDaemon();

  const [sessionType, setSessionType] = useState<SessionType>(initialValues?.sessionType ?? "tool");
  const [projectId, setProjectId] = useState(initialValues?.projectId ?? "");
  const [agent, setAgent] = useState("");
  const [recipe, setRecipe] = useState(initialValues?.recipe ?? "tdd");
  const [stackParent, setStackParent] = useState(initialValues?.stackParent ?? "");
  const [toolPath, setToolPath] = useState("");
  const [model, setModel] = useState(initialValues?.model ?? "");
  const [permissionMode, setPermissionMode] = useState(initialValues?.permissionMode ?? "auto");
  const [sandbox, setSandbox] = useState(false);
  const [initialPrompt, setInitialPrompt] = useState(initialValues?.initialPrompt ?? "");
  const [branchIntent, setBranchIntent] = useState<BranchIntent>(
    initialValues?.branchIntent ?? "new_branch_from_base",
  );
  const [newBranchName, setNewBranchName] = useState(initialValues?.newBranchName ?? "");
  const [selectedBranchToWorkOn, setSelectedBranchToWorkOn] = useState("");
  // Which daemon/host runs the session. Defaults to the pre-filled host, else the selected daemon,
  // else empty (which the daemon treats as "run locally on the connected daemon"). An empty
  // pre-filled host falls through to the selected daemon so the Host <select>'s displayed option
  // matches the value it will submit.
  const [daemonInstanceId, setDaemonInstanceId] = useState(
    initialValues?.daemonInstanceId || selectedInstanceId || "",
  );

  const [projects, setProjects] = useState<ProjectEntry[]>([]);
  const [agents, setAgents] = useState<AgentInfo[]>([]);
  const [tools, setTools] = useState<ToolInfo[]>([]);
  const [subagents, setSubagents] = useState<SubagentInfo[]>([]);
  const [selectedSubagents, setSelectedSubagents] = useState<string[]>([]);
  const [managedCodebase, setManagedCodebase] = useState(false);
  const [sessions, setSessions] = useState<SessionEntry[]>([]);
  const [remoteBranches, setRemoteBranches] = useState<string[]>([]);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // The model catalog is enumerated per selected backend: the chosen agent for tool sessions, and
  // the "claude-cli" pseudo-agent for the Claude CLI session type.
  const modelAgentKey = sessionType === "claude-cli" ? CLAUDE_CLI_AGENT : agent;
  const agentModels = useAgentModels(client, sessionToken, modelAgentKey, daemonInstanceId);

  // Reset the model selection to the backend's advertised default whenever the catalog changes
  // (agent switch, session-type switch). Empty while loading or on a failed probe.
  useEffect(() => {
    setModel(agentModels.defaultModel);
  }, [agentModels.defaultModel]);

  const toggleSubagent = (name: string) => {
    setSelectedSubagents((prev) =>
      prev.includes(name) ? prev.filter((n) => n !== name) : [...prev, name],
    );
  };

  // Load data on mount
  useEffect(() => {
    let cancelled = false;

    // Fetch sessions separately so a network failure doesn't block the rest of the form.
    client
      .listSessions({ sessionToken })
      .then((resp) => {
        if (cancelled) return;
        const loadedSessions = prStackOrchestrators(resp.sessions as SessionEntry[]);
        setSessions(loadedSessions);
      })
      .catch(() => {
        // Session list is best-effort; failing to fetch it just hides the parent picker.
      });

    // Fetch subagents separately (best-effort, like sessions above) — a daemon that doesn't
    // implement ListSubagents yet, or a test double that doesn't stub it, must not block the
    // core project/agent/tool fields from loading.
    client
      .listSubagents({})
      .then((resp) => {
        if (!cancelled) {
          setSubagents(resp.subagents as SubagentInfo[]);
        }
      })
      .catch(() => {
        // Specialized subagents are best-effort; failing to fetch them just leaves the
        // "Managed codebase" section with no options to pick.
      });

    Promise.all([
      client.listProjects({ sessionToken }),
      client.listAgents({}),
      client.listTools({}),
    ])
      .then(([projectsResp, agentsResp, toolsResp]) => {
        if (cancelled) return;

        const loadedProjects = projectsResp.projects as ProjectEntry[];
        const loadedAgents = agentsResp.agents as AgentInfo[];
        const loadedTools = toolsResp.tools as ToolInfo[];

        setProjects(loadedProjects);
        setAgents(loadedAgents);
        setTools(loadedTools);

        // Auto-select agent and toolPath.
        if (loadedAgents.length > 0) {
          setAgent(loadedAgents[0]!.id);
        }
        if (loadedTools.length > 0) {
          setToolPath(loadedTools[0]!.path);
        }
        // Auto-select projectId when there is exactly one choice — no meaningful decision.
        if (loadedProjects.length === 1) {
          setProjectId(loadedProjects[0]!.projectId);
        }
      })
      .catch((err) => {
        if (!cancelled) {
          console.debug("[CreateSessionPane] load error", err);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [client, sessionToken]);

  // Load branches when projectId changes and intent is work_on_selected_branch
  useEffect(() => {
    if (!projectId || branchIntent !== "work_on_selected_branch") return;
    let cancelled = false;
    client
      .listProjectBranches({ sessionToken, projectId, daemonInstanceId })
      .then((resp) => {
        if (!cancelled) {
          setRemoteBranches(resp.branches);
          if (resp.branches.length > 0) {
            setSelectedBranchToWorkOn(resp.branches[0]!);
          }
        }
      })
      .catch((err) => {
        if (!cancelled) {
          console.debug("[CreateSessionPane] listProjectBranches error", err);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [client, sessionToken, projectId, branchIntent, daemonInstanceId]);

  const isSubmitEnabled = (() => {
    if (submitting) return false;
    // A model is always required and comes from the daemon-advertised catalog; a failed/loading
    // probe leaves `model` empty, which disables Create (no fallback).
    if (sessionType === "tool") {
      return Boolean(projectId && agent && toolPath && model);
    }
    return Boolean(projectId && model);
  })();

  const handleSubmit = async () => {
    // Use flushSync to commit the submitting state synchronously before the async fetch starts.
    // This ensures the Create button is visibly disabled in the very next render cycle, even
    // if the network response arrives quickly (e.g. in tests with a fast stub).
    flushSync(() => {
      setSubmitting(true);
      setError(null);
    });
    try {
      const commonParams = {
        sessionToken,
        projectId,
        branchWorktreeIntent: branchIntent,
        newBranchName,
        selectedIntegrationBaseRef: "",
        selectedBranchToWorkOn,
        daemonInstanceId,
      };
      let res: { sessionId: string };
      if (sessionType === "tool") {
        res = await client.startSession({
          ...commonParams,
          toolPath,
          agent,
          recipe,
          stackParent,
          sessionType: "",
          model,
          permissionMode: "",
          initialPrompt: "",
          sandbox: false,
        });
      } else {
        res = await client.startSession({
          ...commonParams,
          toolPath: "",
          agent: "",
          recipe: managedCodebase ? recipe : "",
          stackParent,
          sessionType: "claude-cli",
          model,
          permissionMode,
          initialPrompt,
          sandbox,
          managedCodebase,
          // Only send subagents when managed codebase is enabled — the picker is hidden otherwise,
          // so a selection made before unchecking the toggle must not leak into the request.
          specializedAgents: managedCodebase ? selectedSubagents : [],
        });
      }
      onCreated(res.sessionId);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
    } finally {
      setSubmitting(false);
    }
  };

  // Model selector — shared by both session types, populated from the daemon-advertised catalog for
  // the current backend. While the probe is in flight it shows a loading line; a failed probe shows
  // an inline error and renders no select (so `model` stays empty and Create is disabled).
  const modelField = (
    <div>
      <label className={labelClass} htmlFor="create-session-model">
        Model
      </label>
      {agentModels.loading ? (
        <p data-testid="create-session-model-loading" className="text-sm text-muted-foreground">
          Loading models…
        </p>
      ) : agentModels.error !== null ? (
        <p data-testid="create-session-model-error" className="text-sm text-destructive">
          {agentModels.error}
        </p>
      ) : (
        <select
          id="create-session-model"
          data-testid="create-session-model-select"
          className={inputClass}
          value={model}
          onChange={(e) => setModel(e.target.value)}
        >
          {agentModels.models.map((m) => (
            <option key={m.id} value={m.id}>
              {m.label}
            </option>
          ))}
        </select>
      )}
    </div>
  );

  return (
    <div
      data-testid="create-session-pane"
      className="flex flex-col h-full overflow-y-auto p-4 space-y-4"
    >
      <h2 className="text-sm font-semibold">New session</h2>

      {/* Session type toggle */}
      <div className="flex gap-2">
        <button
          type="button"
          data-testid="create-session-type-tool"
          aria-pressed={sessionType === "tool"}
          onClick={() => setSessionType("tool")}
          className={`px-3 py-1.5 rounded-md text-sm border transition-colors ${
            sessionType === "tool"
              ? "bg-primary text-primary-foreground border-primary"
              : "bg-background text-foreground border-input hover:bg-muted"
          }`}
        >
          Tool
        </button>
        <button
          type="button"
          data-testid="create-session-type-claude-cli"
          aria-pressed={sessionType === "claude-cli"}
          onClick={() => setSessionType("claude-cli")}
          className={`px-3 py-1.5 rounded-md text-sm border transition-colors ${
            sessionType === "claude-cli"
              ? "bg-primary text-primary-foreground border-primary"
              : "bg-background text-foreground border-input hover:bg-muted"
          }`}
        >
          Claude CLI
        </button>
      </div>

      {/* Host — which daemon runs the session. Only shown when the common room advertises daemons. */}
      {daemons.length > 0 && (
        <div>
          <label className={labelClass} htmlFor="create-session-host">
            Host
          </label>
          <select
            id="create-session-host"
            data-testid="create-session-host-select"
            className={inputClass}
            value={daemonInstanceId}
            onChange={(e) => setDaemonInstanceId(e.target.value)}
          >
            {daemons.map((d) => (
              <option key={d.instanceId} value={d.instanceId}>
                {d.label}
              </option>
            ))}
          </select>
        </div>
      )}

      {/* Project */}
      <div>
        <label className={labelClass} htmlFor="create-session-project">
          Project
        </label>
        <select
          id="create-session-project"
          data-testid="create-session-project-select"
          className={inputClass}
          value={projectId}
          onChange={(e) => setProjectId(e.target.value)}
        >
          <option value="" disabled>
            {projects.length === 0 ? "No projects available" : "Select a project…"}
          </option>
          {projects.map((p) => (
            <option key={p.projectId} value={p.projectId}>
              {p.name || p.projectId}
            </option>
          ))}
        </select>
      </div>

      {/* Tool session fields */}
      {sessionType === "tool" && (
        <>
          <div>
            <label className={labelClass} htmlFor="create-session-agent">
              Agent
            </label>
            <select
              id="create-session-agent"
              data-testid="create-session-agent-select"
              className={inputClass}
              value={agent}
              onChange={(e) => setAgent(e.target.value)}
            >
              {agents.map((a) => (
                <option key={a.id} value={a.id}>
                  {a.label || a.id}
                </option>
              ))}
            </select>
          </div>

          <div>
            <label className={labelClass} htmlFor="create-session-recipe">
              Recipe
            </label>
            <select
              id="create-session-recipe"
              data-testid="create-session-recipe-select"
              className={inputClass}
              value={recipe}
              onChange={(e) => setRecipe(e.target.value)}
            >
              {WORKFLOW_RECIPES.map((r) => (
                <option key={r} value={r}>
                  {r}
                </option>
              ))}
            </select>
          </div>

          {modelField}
        </>
      )}

      {/* Claude CLI session fields */}
      {sessionType === "claude-cli" && (
        <>
          {modelField}

          <div>
            <label className={labelClass} htmlFor="create-session-permission-mode">
              Permission mode
            </label>
            <select
              id="create-session-permission-mode"
              data-testid="create-session-permission-mode-select"
              className={inputClass}
              value={permissionMode}
              onChange={(e) => setPermissionMode(e.target.value)}
            >
              <option value="auto">auto</option>
              <option value="default">default</option>
              <option value="acceptEdits">acceptEdits</option>
              <option value="plan">plan</option>
              <option value="bypassPermissions">bypassPermissions</option>
            </select>
          </div>

          <div>
            <label className="flex items-center gap-2 text-sm text-muted-foreground">
              <input
                data-testid="create-session-sandbox-toggle"
                type="checkbox"
                className="h-4 w-4 rounded border-input"
                checked={sandbox}
                onChange={(e) => setSandbox(e.target.checked)}
              />
              Sandbox
            </label>
          </div>

          <div>
            <label className={labelClass} htmlFor="create-session-initial-prompt">
              Initial prompt
            </label>
            <textarea
              id="create-session-initial-prompt"
              data-testid="create-session-initial-prompt-input"
              className={`${inputClass} resize-y`}
              rows={3}
              value={initialPrompt}
              onChange={(e) => setInitialPrompt(e.target.value)}
              placeholder="Optional initial prompt"
            />
          </div>

          {/* Managed codebase — an explicit toggle that, when on, makes the session workflow-aware
              (recipe picker) and lets the user attach specialized subagents.
              See docs/ft/coder/managed-codebase-workflow.md. */}
          <div>
            <label className="flex items-center gap-2 text-sm text-muted-foreground">
              <input
                data-testid="create-session-managed-codebase-toggle"
                type="checkbox"
                className="h-4 w-4 rounded border-input"
                checked={managedCodebase}
                onChange={(e) => setManagedCodebase(e.target.checked)}
              />
              Managed codebase
            </label>
            {managedCodebase && (
              <div className="mt-2 space-y-3 pl-4">
                <div>
                  <label className={labelClass} htmlFor="create-session-recipe">
                    Recipe
                  </label>
                  <select
                    id="create-session-recipe"
                    data-testid="create-session-recipe-select"
                    className={inputClass}
                    value={recipe}
                    onChange={(e) => setRecipe(e.target.value)}
                  >
                    {WORKFLOW_RECIPES.map((r) => (
                      <option key={r} value={r}>
                        {r}
                      </option>
                    ))}
                  </select>
                </div>
                <div
                  data-testid="create-session-managed-codebase-section"
                  className="space-y-1"
                >
                  {subagents.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                      No specialized subagents available
                    </p>
                  ) : (
                    subagents.map((sa) => (
                      <label
                        key={sa.name}
                        className="flex items-center gap-2 text-sm text-muted-foreground"
                      >
                        <input
                          data-testid={`create-session-subagent-checkbox-${sa.name}`}
                          type="checkbox"
                          className="h-4 w-4 rounded border-input"
                          checked={selectedSubagents.includes(sa.name)}
                          onChange={() => toggleSubagent(sa.name)}
                        />
                        {sa.label || sa.name}
                      </label>
                    ))
                  )}
                </div>
              </div>
            )}
          </div>
        </>
      )}

      {/* PR stack parent picker — shown for both session types when orchestrators are available */}
      {sessions.length > 0 && (
        <div>
          <label className={labelClass} htmlFor="create-session-stack-parent">
            PR stack parent
          </label>
          <select
            id="create-session-stack-parent"
            data-testid="create-session-stack-parent-select"
            className={inputClass}
            value={stackParent}
            onChange={(e) => setStackParent(e.target.value)}
          >
            <option value="">None (standalone session)</option>
            {sessions.map((s) => (
              <option key={s.sessionId} value={s.sessionId}>
                {s.sessionId}
              </option>
            ))}
          </select>
        </div>
      )}

      {/* Branch intent */}
      <div>
        <label className={labelClass} htmlFor="create-session-branch-intent">
          Branch mode
        </label>
        <select
          id="create-session-branch-intent"
          data-testid="create-session-branch-intent-select"
          className={inputClass}
          value={branchIntent}
          onChange={(e) => setBranchIntent(e.target.value as BranchIntent)}
        >
          <option value="new_branch_from_base">New branch from base</option>
          <option value="work_on_selected_branch">Work on existing branch</option>
        </select>
      </div>

      {branchIntent === "new_branch_from_base" && (
        <div>
          <label className={labelClass} htmlFor="create-session-new-branch-name">
            New branch name
          </label>
          <input
            id="create-session-new-branch-name"
            data-testid="create-session-new-branch-name-input"
            type="text"
            className={inputClass}
            value={newBranchName}
            onChange={(e) => setNewBranchName(e.target.value)}
            placeholder="e.g. feature/my-work"
          />
        </div>
      )}

      {branchIntent === "work_on_selected_branch" && (
        <div>
          <label className={labelClass} htmlFor="create-session-branch-to-work-on">
            Branch to work on
          </label>
          <select
            id="create-session-branch-to-work-on"
            data-testid="create-session-branch-to-work-on-select"
            className={inputClass}
            value={selectedBranchToWorkOn}
            onChange={(e) => setSelectedBranchToWorkOn(e.target.value)}
          >
            {remoteBranches.map((b) => (
              <option key={b} value={b}>
                {b}
              </option>
            ))}
          </select>
        </div>
      )}

      {/* Error */}
      {error !== null && (
        <p data-testid="create-session-error" className="text-sm text-destructive">
          {error}
        </p>
      )}

      {/* Actions */}
      <div className="flex gap-2 pt-2">
        <Button
          type="button"
          data-testid="create-session-cancel-btn"
          variant="outline"
          onClick={onCancel}
          disabled={submitting}
        >
          Cancel
        </Button>
        <Button
          type="button"
          data-testid="create-session-submit-btn"
          disabled={!isSubmitEnabled}
          onClick={handleSubmit}
        >
          Create session
        </Button>
      </div>
    </div>
  );
}
