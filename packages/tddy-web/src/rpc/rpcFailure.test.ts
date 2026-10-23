/**
 * Unit tests for the failure text every screen shows — in particular a call to a service the
 * daemon never registered, which the router answers with a bare `Unknown service: <name>`.
 */

import { describe, expect, it } from "bun:test";
import { Code, ConnectError } from "@connectrpc/connect";
import { rpcFailureText, unservedService } from "./rpcFailure";

/** What `tddy-rpc`'s router answers for a service it has no entry for. */
function anUnknownServiceError(service: string): ConnectError {
  return new ConnectError(`Unknown service: ${service}`, Code.NotFound);
}

describe("unservedService", () => {
  it("names the service a daemon does not serve", () => {
    // Given
    const error = anUnknownServiceError("accounts.AccountsService");

    // When
    const service = unservedService(error);

    // Then
    expect(service).toBe("accounts.AccountsService");
  });

  it("is null for a not-found that is about a record, not a service", () => {
    // Given
    const error = new ConnectError("project proj-alpha not found", Code.NotFound);

    // When
    const service = unservedService(error);

    // Then
    expect(service).toBeNull();
  });

  it("is null for the same words under another code", () => {
    // Given
    const error = new ConnectError("Unknown service: accounts.AccountsService", Code.Internal);

    // When
    const service = unservedService(error);

    // Then
    expect(service).toBeNull();
  });
});

describe("rpcFailureText", () => {
  it("says a missing service is not served by this host, rather than the router's bare words", () => {
    // Given
    const error = anUnknownServiceError("accounts.AccountsService");

    // When
    const text = rpcFailureText(error);

    // Then
    expect(text).toBe(
      "This host's daemon does not serve accounts.AccountsService — the service is not enabled in its config.",
    );
  });

  it("shows any other refusal in the daemon's own words", () => {
    // Given
    const error = new ConnectError("the vault file is truncated", Code.Internal);

    // When
    const text = rpcFailureText(error);

    // Then
    expect(text).toBe("the vault file is truncated");
  });
});
