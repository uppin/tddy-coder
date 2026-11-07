import React, { useEffect, useState } from "react";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { TokenService } from "../../src/gen/token_pb";
import { useDaemonClient } from "../../src/ports/clients";

function TokenProbe() {
  const client = useDaemonClient(TokenService);
  const [token, setToken] = useState("…");
  useEffect(() => {
    void client?.generateToken({ room: "r", identity: "alice" }).then((r) => setToken(r.token));
  }, [client]);
  return <span data-testid="token">{token}</span>;
}

describe("mountWithClients", () => {
  it("a component calls the in-memory backend with no tddy-web provider in the tree", () => {
    // Given
    const backend = anInMemoryRpcBackend().onUnary(TokenService.method.generateToken, (req) => ({
      token: `token-for-${req.identity}`,
      ttlSeconds: 60n,
    }));

    // When
    cy.mountWithClients(<TokenProbe />, backend);

    // Then
    cy.get("[data-testid='token']").should("have.text", "token-for-alice");
  });
});
