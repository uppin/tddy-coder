import { useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { SessionService, SessionEntry } from "../../gen/session_pb";
import type { ProjectService, ProjectEntry } from "../../gen/project_pb";
import type { CatalogService, ToolInfo } from "../../gen/catalog_pb";
import { projectSelectOptions } from "../../lib/projectSelectOptions";
import { rpcFailureText } from "../../rpc/rpcFailure";

export interface UseCreateSessionCatalogsArgs {
  client: Client<typeof SessionService>;
  projectClient: Client<typeof ProjectService>;
  catalogClient: Client<typeof CatalogService>;
  sessionToken: string;
  /** The tool path to auto-select — called only when the host offers at least one tool. */
  setToolPath: (toolPath: string) => void;
  /** The project to auto-select — called only when exactly one option was offered. */
  setProjectId: (projectId: string) => void;
}

/** What one mount's reads answer with. */
export interface CreateSessionCatalogs {
  projects: ProjectEntry[];
  tools: ToolInfo[];
  sessions: SessionEntry[];
  /**
   * Why each read failed, ready to render beside the picker it emptied — `null` while it has not.
   * Each is its own slot, so one failed read does not hide the others' results.
   */
  projectsError: string | null;
  toolsError: string | null;
  sessionsError: string | null;
}

/**
 * The three catalogs the new-session form opens against: the host's sessions (a failure costs the
 * stack pickers their options and nothing else), its projects and its tools. Each read fails on its
 * own and reports why in its own error slot, so a picker left empty by a failure says so instead of
 * reading as "there is nothing". The two auto-selections happen as each list lands — the form owns
 * those values, so they are handed back rather than held here.
 */
export function useCreateSessionCatalogs({
  client,
  projectClient,
  catalogClient,
  sessionToken,
  setToolPath,
  setProjectId,
}: UseCreateSessionCatalogsArgs): CreateSessionCatalogs {
  const [projects, setProjects] = useState<ProjectEntry[]>([]);
  const [tools, setTools] = useState<ToolInfo[]>([]);
  const [sessions, setSessions] = useState<SessionEntry[]>([]);
  const [projectsError, setProjectsError] = useState<string | null>(null);
  const [toolsError, setToolsError] = useState<string | null>(null);
  const [sessionsError, setSessionsError] = useState<string | null>(null);

  // Load data on mount
  useEffect(() => {
    let cancelled = false;

    // Each read is fetched on its own so one failure doesn't block the rest of the form; a failure
    // leaves the last good list in place and is reported beside the picker it emptied.
    client
      .listSessions({ sessionToken })
      .then((resp) => {
        if (cancelled) return;
        setSessions(resp.sessions as SessionEntry[]);
        setSessionsError(null);
      })
      .catch((err: unknown) => {
        // Without the list the stack-parent and stack-base pickers have nothing to offer.
        if (!cancelled) setSessionsError(rpcFailureText(err));
      });

    // Agents are not read here: they are fanned out across every host by `useSelectableAgents`,
    // since one daemon's answer speaks only for itself.
    projectClient
      .listProjects({ sessionToken })
      .then((projectsResp) => {
        if (cancelled) return;
        const loadedProjects = projectsResp.projects as ProjectEntry[];
        setProjects(loadedProjects);
        setProjectsError(null);
        // Auto-select projectId when there is exactly one choice — no meaningful decision. Counted
        // in offered options, not rows: a single project carried by two hosts is still one choice.
        const loadedOptions = projectSelectOptions(loadedProjects);
        if (loadedOptions.length === 1) {
          setProjectId(loadedOptions[0]!.projectId);
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) setProjectsError(rpcFailureText(err));
      });

    catalogClient
      .listTools({})
      .then((toolsResp) => {
        if (cancelled) return;
        const loadedTools = toolsResp.tools as ToolInfo[];
        setTools(loadedTools);
        setToolsError(null);
        // Auto-select toolPath.
        if (loadedTools.length > 0) {
          setToolPath(loadedTools[0]!.path);
        }
      })
      .catch((err: unknown) => {
        // A tool session cannot be submitted without a tool, so the reason is shown on its fields.
        if (!cancelled) setToolsError(rpcFailureText(err));
      });
    return () => {
      cancelled = true;
    };
  }, [client, projectClient, catalogClient, sessionToken]);

  return { projects, tools, sessions, projectsError, toolsError, sessionsError };
}
