import React from "react";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import type { AppHandler } from "tddy-react-commons/container";
import { aClientsPortDouble, aMediaDouble, aNavigationDouble, anAuthDouble, aPresenceDouble } from "tddy-client-web/testkit";
import { composeApp, type AppPorts } from "../../src/app/composeApp";

function aDoubledApp(path: string): { handler: AppHandler<AppPorts>; ports: AppPorts } {
  const ports: AppPorts = {
    clients: aClientsPortDouble(anInMemoryRpcBackend()),
    navigation: aNavigationDouble(path),
    auth: anAuthDouble({ isAuthenticated: true }),
    presence: aPresenceDouble(),
    media: aMediaDouble(),
  };
  return { ports, handler: { start: () => ({ context: ports, teardown: () => {} }) } };
}

describe("the composed app", () => {
  for (const [path, screen] of [
    ["/sessions", "sessions-drawer"],
    ["/hosts", "hosts-screen"],
    ["/projects", "projects-screen"],
    ["/rpc-playground", "rpc-playground-screen"],
  ] as const) {
    it(`routes ${path} to the package that owns it`, () => {
      // Given
      const { handler } = aDoubledApp(path);

      // When
      cy.mount(composeApp(handler));

      // Then
      cy.get(`[data-testid^='${screen}']`).should("exist");
    });
  }
});
