/**
 * Unit tests for the desktop build's own host registration.
 *
 * Two rules carry this node. **LiveKit is configured or it is not**, and "not" must be a working
 * state rather than a broken one — with either the URL or the common room missing, nothing is
 * joined, no token is minted, and no `Room` is constructed. And **the local host is always there**,
 * from `daemonInstanceId`, which the daemon serves ungated and therefore before sign-in.
 *
 * Changeset: `docs/dev/1-WIP/2026-09-05-optional-livekit-desktop-ipc-host.md`
 */

import { describe, it, expect } from "bun:test";
import { createIpcConnectionProvider, liveKitIsConfigured } from "./localHost";

const THIS_HOST = "instance-this-host";

function aRegistration() {
  return { daemonInstanceId: THIS_HOST, label: "this daemon" };
}

describe("whether LiveKit should be brought up", () => {
  it("is configured when both a url and a common room are given", () => {
    expect(
      liveKitIsConfigured({ livekitUrl: "wss://livekit.example", commonRoom: "tddy" }),
    ).toBe(true);
  });

  it("is not configured when the common room is missing", () => {
    // A url with no room names a server but nothing to join — there is no host list to build
    expect(liveKitIsConfigured({ livekitUrl: "wss://livekit.example" })).toBe(false);
  });

  it("is not configured when the url is missing", () => {
    expect(liveKitIsConfigured({ commonRoom: "tddy" })).toBe(false);
  });

  it("is not configured when nothing is given at all", () => {
    // The desktop app's default. This must be a *working* state: no room joined, no token minted,
    // no Room constructed, and the app fully usable on its own host.
    expect(liveKitIsConfigured({})).toBe(false);
  });

  it("treats blank and whitespace-only settings as unconfigured", () => {
    // The daemon serves absent fields as empty strings, so a blank must not read as configured —
    // it would produce a join attempt against `""` and a connection error on every screen
    expect(liveKitIsConfigured({ livekitUrl: "", commonRoom: "" })).toBe(false);
    expect(liveKitIsConfigured({ livekitUrl: "   ", commonRoom: "tddy" })).toBe(false);
    expect(liveKitIsConfigured({ livekitUrl: "wss://livekit.example", commonRoom: "  " })).toBe(
      false,
    );
  });
});

// The directory source (`createLocalHostDirectorySource`) is covered in `/green`, not here: it
// returns node 2's `HostDirectorySource`, which is not on this branch's PR head. See the note in
// `localHost.ts` and this node's `## Dependencies`.

describe("the desktop's own connection provider", () => {
  it("claims its own host and nothing else", () => {
    // Given the provider
    const provider = createIpcConnectionProvider(aRegistration());

    // Then it reaches its own machine over IPC, and declines every peer — which is what leaves the
    // peers to the LiveKit provider registered behind it
    expect(provider.connectHost(THIS_HOST)).not.toBeNull();
    expect(provider.connectHost("instance-a-peer")).toBeNull();
  });

  it("advertises rpc only, never media or presence", () => {
    // Given a connection to the local host
    const connection = createIpcConnectionProvider(aRegistration()).connectHost(THIS_HOST);

    // Then it is honest about what a frame pipe can carry. Publishing media into a LiveKit room to
    // fill the gap would make the desktop's own host quietly require the thing this stack made
    // optional — the surfaces are absent instead, which is node 4's job.
    expect(connection).not.toBeNull();
    expect([...(connection?.capabilities ?? [])]).toEqual(["rpc"]);
  });
});
