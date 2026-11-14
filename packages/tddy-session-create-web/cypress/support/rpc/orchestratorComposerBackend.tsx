/**
 * The world an orchestrator composer (`#orchestrators` 8/11) is mounted in: two hosts in the common
 * room, both answering from one in-memory backend that serves Claude (which declares
 * `project_home_main` and effort levels) and Cursor (which does neither), a usable provider account,
 * and `StartSession`.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { aClientsPortDouble } from "tddy-client-web/testkit";
import type { DaemonHost } from "tddy-client-web";
import {
  AccountsService,
  ListProviderAccountModelsResponseSchema,
  ListProviderRoutesResponseSchema,
} from "tddy-client-web/gen/accounts_pb";
import {
  CatalogService,
  ListAgentModelsResponseSchema,
  ListAgentsResponseSchema,
  ListSubagentsResponseSchema,
  ListToolsResponseSchema,
} from "tddy-client-web/gen/catalog_pb";
import { ListProjectsResponseSchema, ProjectService } from "tddy-client-web/gen/project_pb";
import { ListSessionsResponseSchema, SessionService, StartSessionResponseSchema } from "tddy-client-web/gen/session_pb";
import { OrchestratorComposer } from "../../../src/OrchestratorComposer";
import { servedAgentWire, withDeclaredSessionTypes } from "../servedAgents";

export const COMPOSER_PROJECT = "proj-beta";
export const HOST_A = "workstation-1";
export const HOST_B = "server-2";
export const COMPOSER_HOSTS: DaemonHost[] = [
  { instanceId: HOST_A, label: "workstation-1" },
  { instanceId: HOST_B, label: "server-2" },
];
export const STARTED_ORCHESTRATOR = "orchestrator-1";

/** The height of the detail pane the composer is mounted in. */
export const PANE_HEIGHT = 760;

export function aDaemonServingAnOrchestratorAgent(): InMemoryRpcBackend {
  return anInMemoryRpcBackend()
    .onUnary(SessionService.method.listSessions, () => create(ListSessionsResponseSchema, { sessions: [] }))
    .onUnary(CatalogService.method.listSubagents, () => create(ListSubagentsResponseSchema, { subagents: [] }))
    .onUnary(CatalogService.method.listTools, () => create(ListToolsResponseSchema, { tools: [] }))
    .onUnary(ProjectService.method.listProjects, () =>
      create(ListProjectsResponseSchema, {
        projects: [{ projectId: COMPOSER_PROJECT, name: "beta", mainRepoPath: "/repo", daemonInstanceId: HOST_A }],
      }),
    )
    .onUnary(CatalogService.method.listAgents, () =>
      create(
        ListAgentsResponseSchema,
        withDeclaredSessionTypes({
          agents: [
            { id: "cursor", label: "Cursor", ...servedAgentWire("cursor") },
            { id: "claude", label: "Claude", ...servedAgentWire("claude"), effortLevels: ["low", "high"] },
          ],
        }),
      ),
    )
    .onUnary(CatalogService.method.listAgentModels, () =>
      create(ListAgentModelsResponseSchema, {
        models: [
          { id: "opus", label: "Claude Opus" },
          { id: "sonnet", label: "Claude Sonnet" },
        ],
        defaultModel: "opus",
      }),
    )
    .onUnary(AccountsService.method.listProviderRoutes, () =>
      create(ListProviderRoutesResponseSchema, {
        harnessRoutable: true,
        harnessReason: "",
        options: [{ accountId: "acct-meta", label: "Meta hackathon", preset: "meta", supported: true, reason: "" }],
      }),
    )
    .onUnary(AccountsService.method.listProviderAccountModels, () =>
      create(ListProviderAccountModelsResponseSchema, { modelIds: ["muse-spark-1.3"] }),
    )
    .onUnary(SessionService.method.startSession, () =>
      create(StartSessionResponseSchema, { sessionId: STARTED_ORCHESTRATOR }),
    );
}

/** Mounts the composer for {@link COMPOSER_PROJECT} on {@link HOST_A}, in a pane {@link PANE_HEIGHT} tall. */
export function mountTheComposer(backend: InMemoryRpcBackend, onStarted: (sessionId: string) => void = cy.stub()) {
  cy.mountWithClients(
    <div style={{ height: PANE_HEIGHT, display: "flex", flexDirection: "column" }}>
      <OrchestratorComposer
        projectId={COMPOSER_PROJECT}
        daemonInstanceId={HOST_A}
        sessionToken="tok-composer"
        onStarted={onStarted}
      />
    </div>,
    backend,
    { clients: aClientsPortDouble(backend, { daemons: COMPOSER_HOSTS }) },
  );
}
