import { describe, expect, test } from "bun:test";
import {
  inferParticipantRole,
  metadataLooksLikeDaemonAdvertisement,
} from "./useRoomParticipants";

describe("inferParticipantRole", () => {
  test("browser for web- and browser- identities", () => {
    expect(inferParticipantRole("web-u-1-x", "")).toBe("browser");
    expect(inferParticipantRole("browser-1", "")).toBe("browser");
  });

  test("coder for server and daemon- session identities", () => {
    expect(inferParticipantRole("server", "")).toBe("coder");
    expect(inferParticipantRole("server-1", "")).toBe("coder");
    expect(inferParticipantRole("daemon-019d7d74-3a7f-7b03-88d2-f50bb7efb2f0", "")).toBe("coder");
  });

  test("daemon for common-room advertisement metadata", () => {
    const meta = JSON.stringify({
      instance_id: "LT-R1VXTH2V6H",
      label: "LT-R1VXTH2V6H (this daemon)",
    });
    expect(inferParticipantRole("LT-R1VXTH2V6H", meta)).toBe("daemon");
    expect(inferParticipantRole("udoo", '{"instance_id":"udoo","label":"udoo (this daemon)"}')).toBe(
      "daemon",
    );
  });

  test("coder wins over daemon metadata when identity is daemon-uuid", () => {
    const meta = JSON.stringify({
      instance_id: "x",
      label: "x (this daemon)",
    });
    expect(inferParticipantRole("daemon-019d7d74-3a7f-7b03-88d2-f50bb7efb2f0", meta)).toBe("coder");
  });

  test("unknown when no heuristic matches", () => {
    expect(inferParticipantRole("random-peer", "")).toBe("unknown");
  });
});

describe("metadataLooksLikeDaemonAdvertisement", () => {
  test("accepts documented shape", () => {
    expect(
      metadataLooksLikeDaemonAdvertisement(
        '{"instance_id":"a","label":"a (this daemon)"}',
      ),
    ).toBe(true);
  });

  test("rejects codex oauth only", () => {
    expect(
      metadataLooksLikeDaemonAdvertisement(
        JSON.stringify({ codex_oauth: { pending: true } }),
      ),
    ).toBe(false);
  });
});
