/**
 * Unit tests for the host-directory merge.
 *
 * The rules that matter are the ones that decide what a LiveKit-less page sees. Before the host
 * directory an unconfigured common room yielded an empty host list, so `tddy-desktop` offered
 * nothing at all — not even the daemon it was running in the same process as. These tests pin the
 * replacement: an unconfigured source contributes nothing and reports `idle`, the serving host is
 * always there, and one broken source never condemns the directory.
 *
 * Technical: `packages/tddy-web/docs/host-directory.md`
 */

import { describe, it, expect } from "bun:test";
import { directoryStatusOf, hostsOf, mergeHostDirectory } from "./useHostDirectory";
import type { HostDescriptor, HostDirectorySource } from "./types";
import type { ConnectionStatus } from "../connections/types";

function aHostNamed(hostId: string, sourceId: string, label = hostId): HostDescriptor {
  return { hostId, label, sourceId };
}

function aSourceNamed(
  id: string,
  status: ConnectionStatus,
  hosts: HostDescriptor[],
  error: string | null = null,
): HostDirectorySource {
  return { id, status, error, hosts };
}

/** What a common room that was never configured looks like: no hosts, and idle rather than broken. */
function anUnconfiguredLiveKitSource(): HostDirectorySource {
  return aSourceNamed("livekit", "idle", []);
}

/** The daemon serving this page, which `/api/config` always names. */
function aServingHostSource(hostId: string): HostDirectorySource {
  return aSourceNamed("serving", "connected", [aHostNamed(hostId, "serving", "this daemon")]);
}

describe("host directory merge", () => {
  it("offers the serving daemon when no common room is configured", () => {
    // Given a page with LiveKit switched off entirely — the desktop app's default
    const directory = mergeHostDirectory([
      aServingHostSource("instance-this-host"),
      anUnconfiguredLiveKitSource(),
    ]);

    // Then there is exactly one host and it is usable. This list used to be empty and the selector
    // offered nothing, which is why a LiveKit-less desktop app could reach no host at all.
    expect(directory.hosts.map((h) => h.hostId)).toEqual(["instance-this-host"]);
    expect(directory.status).toEqual("connected");
    expect(directory.error).toBeNull();
  });

  it("shows the serving daemon alongside the common room's peers when both are configured", () => {
    // Given a desktop app that also has LiveKit settings
    const directory = mergeHostDirectory([
      aServingHostSource("instance-this-host"),
      aSourceNamed("livekit", "connected", [
        aHostNamed("instance-a-peer", "livekit"),
        aHostNamed("instance-another-peer", "livekit"),
      ]),
    ]);

    // Then it can reach its own host and its peers in the same session, without a reload
    expect(directory.hosts.map((h) => h.hostId)).toEqual([
      "instance-this-host",
      "instance-a-peer",
      "instance-another-peer",
    ]);
  });

  it("keeps the first source's description of a host both sources know", () => {
    // Given the desktop's own account of its host, and the common room's advertisement of the
    // same machine, with the local source registered first
    const directory = mergeHostDirectory([
      aSourceNamed("local-ipc", "connected", [
        aHostNamed("instance-this-host", "local-ipc", "this daemon"),
      ]),
      aSourceNamed("livekit", "connected", [
        aHostNamed("instance-this-host", "livekit", "laptop-a (this daemon)"),
      ]),
    ]);

    // Then the machine appears once, described by the source that knows it best
    expect(directory.hosts).toHaveLength(1);
    expect(directory.hosts[0]?.sourceId).toEqual("local-ipc");
    expect(directory.hosts[0]?.label).toEqual("this daemon");
  });

  it("stays usable when one source is in error", () => {
    // Given a reachable local host and a common room that will not connect
    const directory = mergeHostDirectory([
      aServingHostSource("instance-this-host"),
      aSourceNamed("livekit", "error", [], "could not reach the LiveKit server"),
    ]);

    // Then the directory is connected, not broken: the local host is still selectable and fully
    // functional, and the failure is reported against the source that had it
    expect(directory.status).toEqual("connected");
    expect(directory.hosts.map((h) => h.hostId)).toEqual(["instance-this-host"]);
    expect(directory.sources.find((s) => s.id === "livekit")?.error).toEqual(
      "could not reach the LiveKit server",
    );
  });

  it("reports error only when every source has failed", () => {
    // Given nothing that can answer
    const status = directoryStatusOf([
      aSourceNamed("serving", "error", [], "the daemon did not answer"),
      aSourceNamed("livekit", "error", [], "could not reach the LiveKit server"),
    ]);

    // Then the directory says so
    expect(status).toEqual("error");
  });

  it("treats an unconfigured source as idle rather than as a failure", () => {
    // Given only a common room that was never configured — the case that must not look broken
    const status = directoryStatusOf([anUnconfiguredLiveKitSource()]);

    // Then the directory is idle. An `error` here is what would put a connection failure on
    // every desktop screen for a feature the operator deliberately did not configure.
    expect(status).toEqual("idle");
  });

  it("reports connecting while a source is still joining", () => {
    // Given a common room mid-join and nothing else yet
    const status = directoryStatusOf([aSourceNamed("livekit", "connecting", [])]);

    // Then the selector chrome can say "connecting" rather than "no hosts" — the distinction that
    // kept the presence panel claiming it was connecting for as long as the tab stayed open
    expect(status).toEqual("connecting");
  });

  it("names the first source's reason when every source has failed", () => {
    // Given nothing that can answer, each source failing for its own reason
    const directory = mergeHostDirectory([
      aSourceNamed("serving", "error", [], "the daemon did not answer"),
      aSourceNamed("livekit", "error", [], "could not reach the LiveKit server"),
    ]);

    // Then the directory carries a reason rather than only a status. A screen that can offer no
    // host at all has to say why, and until every source has failed the reason belongs to the
    // source that had it — which is why this is the only case that publishes one.
    expect(directory.status).toEqual("error");
    expect(directory.error).toEqual("the daemon did not answer");
  });

  it("keeps a source's hosts in the order it reported them", () => {
    // Given one source with several hosts
    const hosts = hostsOf([
      aSourceNamed("livekit", "connected", [
        aHostNamed("instance-c", "livekit"),
        aHostNamed("instance-a", "livekit"),
        aHostNamed("instance-b", "livekit"),
      ]),
    ]);

    // Then the merge does not reorder them — the source decides, and the LiveKit one already
    // orders by its own participant ordering
    expect(hosts.map((h) => h.hostId)).toEqual(["instance-c", "instance-a", "instance-b"]);
  });

  it("yields an empty directory, not a failure, when there are no sources at all", () => {
    // Given a page before anything has registered
    const directory = mergeHostDirectory([]);

    // Then it is idle and empty — the same shape as "nothing has been asked of this yet"
    expect(directory.hosts).toEqual([]);
    expect(directory.status).toEqual("idle");
    expect(directory.error).toBeNull();
  });
});
