import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { anAuthDouble } from "../testkit";
import { AuthPortProvider, type AuthPort, useAuth } from "./auth";

afterEach(cleanup);

function inPort(port: AuthPort) {
  return ({ children }: { children: ReactNode }) => <AuthPortProvider port={port}>{children}</AuthPortProvider>;
}

describe("the auth port", () => {
  test("useAuth returns the port's state and actions", async () => {
    // Given
    const port = anAuthDouble({ isAuthenticated: true, sessionToken: "tok" });

    // When
    const { result } = renderHook(() => useAuth(), { wrapper: inPort(port) });
    await result.current.logout();

    // Then
    expect(result.current.isAuthenticated).toBe(true);
    expect(result.current.sessionToken).toBe("tok");
    expect(port.logout).toHaveBeenCalledTimes(1);
  });

  test("useAuth outside a provider throws naming the port", () => {
    // When / Then
    expect(() => renderHook(() => useAuth())).toThrow(/AuthPortProvider/);
  });
});
