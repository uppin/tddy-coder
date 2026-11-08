import React from "react";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import type { AppHandler } from "tddy-react-commons/container";
import { aClientsPortDouble, aMediaDouble, aNavigationDouble, aRoomRpcDouble, anAuthDouble, aPresenceDouble } from "tddy-client-web/testkit";
import { composeApp, type AppPorts } from "../../src/app/composeApp";

function aDoubledApp(path: string): { handler: AppHandler<AppPorts>; ports: AppPorts } {
  const ports: AppPorts = {
    clients: aClientsPortDouble(anInMemoryRpcBackend()),
    navigation: aNavigationDouble(path),
    auth: anAuthDouble({ isAuthenticated: true }),
    presence: aPresenceDouble(),
    media: aMediaDouble(),
    roomRpc: aRoomRpcDouble(anInMemoryRpcBackend()),
  };
  return { ports, handler: { start: () => ({ context: ports, teardown: () => {} }) } };
}

describe("the composed app", () => {
  for (const [path, screen] of [
    ["/sessions", "sessions-drawer"],
    ["/hosts", "hosts-screen"],
    ["/projects", "projects-screen"],
    ["/rpc-playground", "rpc-playground-home"],
  ] as const) {
    it(`routes ${path} to the package that owns it`, () => {
      // Given
      const { handler } = aDoubledApp(path);
      // The serving daemon's config: a daemon-mode page, whose sign-in the doubled auth port has done.
      cy.intercept("GET", "/api/config", { daemon_mode: true, auth_flow: "redirect" });

      // When
      cy.mount(composeApp(handler));

      // Then
      cy.get(`[data-testid^='${screen}']`).should("exist");
    });
  }
});
