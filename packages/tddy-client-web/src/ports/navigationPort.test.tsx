import { afterEach, describe, expect, test } from "bun:test";
import { act, cleanup, renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { aNavigationDouble } from "../testkit";
import { NavigationProvider, type NavigationPort, useAppLocation } from "./navigation";

afterEach(cleanup);

function inPort(port: NavigationPort) {
  return ({ children }: { children: ReactNode }) => <NavigationProvider port={port}>{children}</NavigationProvider>;
}

describe("the navigation port", () => {
  test("useAppLocation reads the port's location", () => {
    // Given
    const port = aNavigationDouble("/sessions/abc?inspector=files");

    // When
    const { result } = renderHook(() => useAppLocation(), { wrapper: inPort(port) });

    // Then
    expect(result.current.location).toEqual({ path: "/sessions/abc", params: { inspector: "files" } });
  });

  test("navigating calls the port and re-renders with the new location", () => {
    // Given
    const port = aNavigationDouble("/");
    const { result } = renderHook(() => useAppLocation(), { wrapper: inPort(port) });

    // When
    act(() => result.current.navigate("/hosts"));

    // Then
    expect(port.history.at(-1)?.path).toBe("/hosts");
    expect(result.current.location.path).toBe("/hosts");
  });

  test("setParams patches the params and keeps the path", () => {
    // Given
    const port = aNavigationDouble("/sessions/abc?inspector=files&full=1");
    const { result } = renderHook(() => useAppLocation(), { wrapper: inPort(port) });

    // When
    act(() => result.current.setParams({ inspector: "usage", full: null }));

    // Then
    expect(result.current.location).toEqual({ path: "/sessions/abc", params: { inspector: "usage" } });
  });

  test("useAppLocation outside a provider throws naming the port", () => {
    // When / Then
    expect(() => renderHook(() => useAppLocation())).toThrow(/NavigationProvider/);
  });
});
