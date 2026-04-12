import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import React from "react";
import { TunnelManagementPanel } from "../../src/components/tunnels/TunnelManagementPanel";
import {
  ListTunnelAdvertisementsResponseSchema,
  OpenBrowserForTunnelRequestSchema,
  OpenBrowserForTunnelResponseSchema,
  TunnelAdvertisementSchema,
  TunnelBindingState,
  TunnelKind,
} from "../../src/gen/tunnel_management_pb";

/** Binary request body from `cy.intercept` (ArrayBuffer, Buffer, Uint8Array, or binary string). */
function connectRequestBodyToUint8(body: unknown): Uint8Array {
  if (body instanceof Uint8Array) {
    return body;
  }
  if (body instanceof ArrayBuffer) {
    return new Uint8Array(body);
  }
  if (typeof Buffer !== "undefined" && Buffer.isBuffer(body)) {
    return new Uint8Array(body.buffer, body.byteOffset, body.byteLength);
  }
  if (typeof body === "string") {
    const out = new Uint8Array(body.length);
    for (let i = 0; i < body.length; i++) {
      out[i] = body.charCodeAt(i) & 0xff;
    }
    return out;
  }
  throw new Error(`Unsupported request body: ${Object.prototype.toString.call(body)}`);
}

const toArrayBuffer = (u8: Uint8Array) => {
  const buf = new ArrayBuffer(u8.length);
  new Uint8Array(buf).set(u8);
  return buf;
};

describe("TunnelManagementPanel (acceptance)", () => {
  it("web_tunnel_panel_invokes_open_browser_rpc_with_expected_message", () => {
    const rpcBase = `${Cypress.config("baseUrl")?.replace(/\/$/, "")}/rpc`;

    cy.intercept("POST", "**/rpc/tunnel_management.TunnelManagementService/ListTunnelAdvertisements", (req) => {
      expect(req.url).to.contain("tunnel_management.TunnelManagementService/ListTunnelAdvertisements");
      const ad = create(TunnelAdvertisementSchema, {
        operatorLoopbackPort: 9999,
        sessionCorrelationId: "rpc-listed-tunnel-session-key",
        kind: TunnelKind.CODEX_OAUTH,
        state: TunnelBindingState.PENDING,
        authorizeUrl: "https://oauth.example.test/authorize?client=acceptance",
      });
      const listResp = create(ListTunnelAdvertisementsResponseSchema, { advertisements: [ad] });
      const body = toBinary(ListTunnelAdvertisementsResponseSchema, listResp);
      req.reply({
        statusCode: 200,
        headers: { "content-type": "application/proto" },
        body: toArrayBuffer(body),
      });
    }).as("listTunnelAds");

    cy.intercept("POST", "**/rpc/tunnel_management.TunnelManagementService/OpenBrowserForTunnel", (req) => {
      expect(req.url).to.contain("tunnel_management.TunnelManagementService/OpenBrowserForTunnel");
      const u8 = connectRequestBodyToUint8(req.body);
      const decoded = fromBinary(OpenBrowserForTunnelRequestSchema, u8);
      // Contract: session key from RPC list/start flow (opaque correlation id for the tunnel row).
      expect(decoded.sessionCorrelationId).to.eq("rpc-listed-tunnel-session-key");
      expect(decoded.url).to.eq("https://oauth.example.test/authorize?client=acceptance");
      const emptyOpen = create(OpenBrowserForTunnelResponseSchema, {});
      const openBody = toBinary(OpenBrowserForTunnelResponseSchema, emptyOpen);
      req.reply({
        statusCode: 200,
        headers: { "content-type": "application/proto" },
        body: toArrayBuffer(openBody),
      });
    }).as("openBrowserTunnel");

    cy.mount(<TunnelManagementPanel rpcBaseUrl={rpcBase} />);
    cy.wait("@listTunnelAds");
    cy.get("[data-testid='tunnel-open-browser']").click();
    cy.wait("@openBrowserTunnel");
  });
});
