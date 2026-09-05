/**
 * Acceptance tests: the RPC Playground's service / method selection round-trips through the URL,
 * so "the call I am debugging" is a shareable link.
 *
 * The screen reads and writes the location through the shared hash store (`useAppLocation`), which
 * is a module singleton — so it needs no router ancestor and can be mounted bare here.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-01-url-state-routing.md.
 * Changeset: docs/dev/1-WIP/2026-08-01-web-url-state-routing.md.
 */

import React from "react";
import { RpcPlaygroundScreen } from "../../src/rpc-playground/RpcPlaygroundScreen";
import { rpcPlaygroundPage } from "../support/pages/rpcPlaygroundPage";
import { appLocationPage } from "../support/pages/appLocationPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/**
 * Whether the connection in scope can serve a participant roster. These scenarios are about the
 * service tree and the request editor, so they state the ordinary case — a host reached over
 * LiveKit, where the picker is offered. The wire that carries no roster, and the explanation that
 * replaces the picker on it, are covered by `PresenceCapabilityGatingAcceptance.cy.tsx`.
 */
const A_CONNECTION_THAT_CARRIES_A_ROSTER = true;

const CONNECTION_SERVICE = "connection.ConnectionService";
const TASK_SERVICE = "tasks.TaskService";

const SERVICES = [
  {
    name: CONNECTION_SERVICE,
    methods: [
      { name: "ListSessions", kind: "unary" as const },
      { name: "StartSession", kind: "unary" as const },
    ],
  },
  {
    name: TASK_SERVICE,
    methods: [{ name: "WatchTaskList", kind: "server_streaming" as const }],
  },
];

function mountPlayground() {
  cy.mount(
    <RpcPlaygroundScreen
      services={SERVICES}
      onInvoke={() => Promise.resolve({ kind: "success" as const, json: "{}" })}
      presenceAvailable={A_CONNECTION_THAT_CARRIES_A_ROSTER}
      onNavigate={() => undefined}
    />,
  );
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800);
  appLocationPage.startAt("/rpc-playground");
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

it("selecting a method records its service and method in the URL", () => {
  // Given
  mountPlayground();
  rpcPlaygroundPage.expandService(CONNECTION_SERVICE);

  // When
  rpcPlaygroundPage.chooseMethod(CONNECTION_SERVICE, "StartSession");

  // Then
  appLocationPage.expectParam("service", CONNECTION_SERVICE);
  appLocationPage.expectParam("method", "StartSession");
});

it("a ?service=&method= deep link opens that method's request editor on load", () => {
  // Given
  appLocationPage.startAt(`/rpc-playground?service=${TASK_SERVICE}&method=WatchTaskList`);

  // When
  mountPlayground();

  // Then
  rpcPlaygroundPage.expectEditorFor(TASK_SERVICE, "WatchTaskList");
});

it("an inbound URL change to a method the service does not have clears the request editor", () => {
  // Given — a deep-linked, valid method is open
  appLocationPage.startAt(`/rpc-playground?service=${TASK_SERVICE}&method=WatchTaskList`);
  mountPlayground();
  rpcPlaygroundPage.expectEditorFor(TASK_SERVICE, "WatchTaskList");

  // When — the address bar is edited to name a method that service does not have
  appLocationPage.navigateExternally(`/rpc-playground?service=${TASK_SERVICE}&method=NoSuchMethod`);

  // Then — the unresolvable selection degrades to none, rather than rendering a broken editor
  rpcPlaygroundPage.expectNoMethodSelected();
});
