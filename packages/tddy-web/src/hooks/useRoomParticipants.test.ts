import { describe, expect, it } from "bun:test";
import { aDaemonAdvertisementMeta } from "../test-utils";
import {
  inferParticipantRole,
  metadataLooksLikeDaemonAdvertisement,
} from "./useRoomParticipants";

describe("inferParticipantRole", () => {
  it("infers browser role for identities prefixed with web-", () => {
    // Given / When / Then
    expect(inferParticipantRole("web-u-1-x", "")).toBe("browser");
  });

  it("infers browser role for identities prefixed with browser-", () => {
    // Given / When / Then
    expect(inferParticipantRole("browser-1", "")).toBe("browser");
  });

  it("infers coder role for the exact identity 'server'", () => {
    // Given / When / Then
    expect(inferParticipantRole("server", "")).toBe("coder");
  });

  it("infers coder role for identities prefixed with server-", () => {
    // Given / When / Then
    expect(inferParticipantRole("server-1", "")).toBe("coder");
  });

  it("infers coder role for identities prefixed with daemon-uuid", () => {
    // Given / When / Then
    expect(inferParticipantRole("daemon-019d7d74-3a7f-7b03-88d2-f50bb7efb2f0", "")).toBe("coder");
  });

  it("infers daemon role when metadata is a daemon advertisement with matching identity", () => {
    // Given
    const meta = aDaemonAdvertisementMeta({ instanceId: "LT-R1VXTH2V6H", label: "LT-R1VXTH2V6H (this daemon)" });

    // When / Then
    expect(inferParticipantRole("LT-R1VXTH2V6H", meta)).toBe("daemon");
  });

  it("infers daemon role for another daemon advertisement example (udoo host)", () => {
    // Given
    const meta = aDaemonAdvertisementMeta({ instanceId: "udoo", label: "udoo (this daemon)" });

    // When / Then
    expect(inferParticipantRole("udoo", meta)).toBe("daemon");
  });

  it("coder role wins over daemon metadata when identity is a daemon-uuid session", () => {
    // Given — a daemon-uuid identity that also carries advertisement metadata
    const meta = aDaemonAdvertisementMeta({ instanceId: "x", label: "x (this daemon)" });

    // When / Then — daemon-uuid prefix overrides daemon metadata heuristic
    expect(inferParticipantRole("daemon-019d7d74-3a7f-7b03-88d2-f50bb7efb2f0", meta)).toBe("coder");
  });

  it("returns unknown when no heuristic matches", () => {
    // Given / When / Then
    expect(inferParticipantRole("random-peer", "")).toBe("unknown");
  });
});

describe("metadataLooksLikeDaemonAdvertisement", () => {
  it("accepts the documented daemon advertisement shape", () => {
    // Given
    const meta = aDaemonAdvertisementMeta({ instanceId: "a", label: "a (this daemon)" });

    // When / Then
    expect(metadataLooksLikeDaemonAdvertisement(meta)).toBe(true);
  });

  it("rejects metadata that only contains codex_oauth (not a daemon advertisement)", () => {
    // Given
    const meta = JSON.stringify({ codex_oauth: { pending: true } });

    // When / Then
    expect(metadataLooksLikeDaemonAdvertisement(meta)).toBe(false);
  });
});
