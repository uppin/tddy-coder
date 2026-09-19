import { useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { SessionService, SessionEntry } from "../../gen/session_pb";
import type { ProjectService, ProjectEntry } from "../../gen/project_pb";
import type { CatalogService, ToolInfo } from "../../gen/catalog_pb";
import { projectSelectOptions } from "../../lib/projectSelectOptions";

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
}

/**
 * The three catalogs the new-session form opens against: the host's sessions (best-effort — a
 * failure costs the stack pickers their options and nothing else), its projects and its tools.
 * Lifted out of `CreateSessionPane.tsx` with their state; the effect body is unchanged, and the two
 * auto-selections still happen at the same moment on the same inputs — the form owns those values,
 * so they are handed back rather than held here.
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

  // Load data on mount
  useEffect(() => {
    let cancelled = false;

    // Fetch sessions separately so a network failure doesn't block the rest of the form.
    client
      .listSessions({ sessionToken })
      .then((resp) => {
        if (cancelled) return;
        setSessions(resp.sessions as SessionEntry[]);
      })
      .catch(() => {
        // Session list is best-effort; failing to fetch it just leaves the stack-parent and
        // stack-base pickers with nothing to offer.
      });

    // Agents are not read here: they are fanned out across every host by `useSelectableAgents`,
    // since one daemon's answer speaks only for itself.
    Promise.all([projectClient.listProjects({ sessionToken }), catalogClient.listTools({})])
      .then(([projectsResp, toolsResp]) => {
        if (cancelled) return;

        const loadedProjects = projectsResp.projects as ProjectEntry[];
        const loadedTools = toolsResp.tools as ToolInfo[];

        setProjects(loadedProjects);
        setTools(loadedTools);

        // Auto-select toolPath.
        if (loadedTools.length > 0) {
          setToolPath(loadedTools[0]!.path);
        }
        // Auto-select projectId when there is exactly one choice — no meaningful decision. Counted
        // in offered options, not rows: a single project carried by two hosts is still one choice.
        const loadedOptions = projectSelectOptions(loadedProjects);
        if (loadedOptions.length === 1) {
          setProjectId(loadedOptions[0]!.projectId);
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
  }, [client, projectClient, catalogClient, sessionToken]);

  return { projects, tools, sessions };
}
