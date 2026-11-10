/**
 * In-memory backend for the Worktrees screen: `worktree.WorktreeService` (stats stream, delete) and
 * the project registry (`project.ProjectService.ListProjects`). Auth and the daemon roster come from
 * the port doubles, not from RPC.
 */
import { create, type MessageInitShape } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { ProjectEntrySchema, ProjectService, type ProjectEntry } from "tddy-client-web/gen/project_pb";
import { WorktreeService } from "tddy-client-web/gen/worktree_pb";
import {
  aWorktreeServiceFake,
  type WorktreeServiceControls,
  type WorktreeServiceScenario,
} from "./worktreeServiceBackend";

export interface WorktreesScreenScenario extends WorktreeServiceScenario {
  projectsOverride?: Partial<ProjectEntry>[];
}

export interface WorktreesScreenBackend extends WorktreeServiceControls {
  backend: InMemoryRpcBackend;
}

const DEFAULT_PROJECT: Partial<ProjectEntry> = {
  projectId: "proj-1",
  name: "Demo",
  gitUrl: "https://github.com/test/demo.git",
  mainRepoPath: "/repos/demo",
  daemonInstanceId: "",
};

export function aWorktreesScreenBackend(scenario: WorktreesScreenScenario = {}): WorktreesScreenBackend {
  const { handlers, ...controls } = aWorktreeServiceFake(scenario);
  const projects = scenario.projectsOverride ?? [DEFAULT_PROJECT];
  return {
    backend: anInMemoryRpcBackend()
      .implement(WorktreeService, handlers)
      .implement(ProjectService, {
        listProjects: async () => ({ projects: projects.map((p) => create(ProjectEntrySchema, p as MessageInitShape<typeof ProjectEntrySchema>)) }),
      }),
    ...controls,
  };
}
