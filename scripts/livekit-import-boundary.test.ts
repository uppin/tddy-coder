import { describe, expect, test } from "bun:test";
import { execFileSync } from "node:child_process";

/**
 * Only the LiveKit packages may import `livekit-client`: `tddy-livekit-web` (rooms, presence, media,
 * the LiveKit connection provider) and `tddy-livekit-rpc-web` (RPC over a room). Every other web
 * package reaches LiveKit through the presence, media and clients ports.
 */

const ALLOWED = ["packages/tddy-livekit-web/", "packages/tddy-livekit-rpc-web/"];


function importersOfLiveKit(): string[] {
  try {
    return execFileSync(
      "git",
      ["grep", "-l", "-E", "from ['\"]livekit-client['\"]|import\\(['\"]livekit-client['\"]\\)", "--", "packages/*/src/**", "packages/*/cypress/**"],
      { cwd: `${import.meta.dir}/..`, encoding: "utf8" },
    )
      .split("\n")
      .filter(Boolean);
  } catch {
    return [];
  }
}

describe("the LiveKit import boundary", () => {
  test("only tddy-livekit-web and tddy-livekit-rpc-web import livekit-client", () => {
    // When
    const outside = importersOfLiveKit()
      .filter((f) => !ALLOWED.some((dir) => f.startsWith(dir)));

    // Then
    expect(outside).toEqual([]);
  });
});
