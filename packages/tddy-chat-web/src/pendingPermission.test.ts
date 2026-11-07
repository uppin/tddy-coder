/**
 * Unit tests — reading a subagent conversation's permission request out of its transcript.
 *
 * The conversation view names a request by its tool call's id and reads the request from that call's
 * input. Both readings are pinned here, because a request read wrongly would offer the operator the
 * wrong options.
 */

import { describe, expect, it } from "bun:test";
import { permissionRequestIdOf, permissionRequestOf } from "./pendingPermission";

const A_REQUEST = {
  requestId: "perm-1",
  title: "Run cargo test",
  options: [
    { optionId: "allow-once", name: "Allow once", kind: "allow_once" },
    { optionId: "reject-once", name: "Reject", kind: "reject_once" },
  ],
  deferredToUser: true,
};

describe("permissionRequestIdOf", () => {
  it("names the request a permission tool call id carries", () => {
    expect(permissionRequestIdOf("permission:perm-1")).toEqual("perm-1");
  });

  it("names no request for a tool call id that is not a permission", () => {
    expect(permissionRequestIdOf("subagent:conv-1")).toBeNull();
    expect(permissionRequestIdOf("toolu_01")).toBeNull();
  });

  it("names no request for a permission id with nothing after the prefix", () => {
    expect(permissionRequestIdOf("permission:")).toBeNull();
  });
});

describe("permissionRequestOf", () => {
  it("reads the request the input carries", () => {
    expect(permissionRequestOf(JSON.stringify(A_REQUEST))).toEqual(A_REQUEST);
  });

  it("reads nothing from an input that was not recorded", () => {
    expect(permissionRequestOf(undefined)).toBeNull();
  });

  it("reads nothing from an input that is not JSON", () => {
    expect(permissionRequestOf("Run cargo test")).toBeNull();
  });

  it("reads nothing when the deferral is not stated", () => {
    const { deferredToUser: _omitted, ...withoutDeferral } = A_REQUEST;
    expect(permissionRequestOf(JSON.stringify(withoutDeferral))).toBeNull();
  });

  it("reads nothing when an option is missing its name", () => {
    const broken = {
      ...A_REQUEST,
      options: [{ optionId: "allow-once", kind: "allow_once" }],
    };
    expect(permissionRequestOf(JSON.stringify(broken))).toBeNull();
  });
});
