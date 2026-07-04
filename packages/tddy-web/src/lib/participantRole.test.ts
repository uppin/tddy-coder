import { describe, expect, it } from "bun:test";
import { aDaemonAdvertisementMeta } from "../test-utils";
import { daemonHostsFromParticipants, daemonRpcIdentity, parseDaemonAdvertisement } from "./participantRole";

describe("daemonHostsFromParticipants", () => {
  it("keeps only daemon-role participants and reads instance id + label from the advertisement", () => {
    // Given a mix of a genuine daemon, a coder session, and this browser
    const participants = [
      { identity: "udoo", metadata: aDaemonAdvertisementMeta({ instanceId: "udoo", label: "udoo (this daemon)" }) },
      { identity: "daemon-019d7d74-3a7f-7b03-88d2-f50bb7efb2f0", metadata: "" },
      { identity: "web-u-1-x", metadata: "" },
    ];

    // When
    const hosts = daemonHostsFromParticipants(participants);

    // Then
    expect(hosts).toEqual([{ instanceId: "udoo", label: "udoo (this daemon)" }]);
  });

  it("excludes a coder session even when it publishes advertisement-shaped metadata", () => {
    // Given a coder session (daemon-<uuid> identity) whose metadata looks like an advertisement
    const participants = [
      {
        identity: "daemon-019d7d74-3a7f-7b03-88d2-f50bb7efb2f0",
        metadata: aDaemonAdvertisementMeta({ instanceId: "proj-x", label: "proj-x (this daemon)" }),
      },
    ];

    // When
    const hosts = daemonHostsFromParticipants(participants);

    // Then
    expect(hosts).toEqual([]);
  });

  it("deduplicates daemons by instance id, preserving first-seen order", () => {
    // Given the same daemon advertised twice plus a second daemon
    const participants = [
      { identity: "udoo", metadata: aDaemonAdvertisementMeta({ instanceId: "udoo", label: "udoo (this daemon)" }) },
      { identity: "udoo", metadata: aDaemonAdvertisementMeta({ instanceId: "udoo", label: "udoo (this daemon)" }) },
      { identity: "srv2", metadata: aDaemonAdvertisementMeta({ instanceId: "srv2", label: "srv2 (this daemon)" }) },
    ];

    // When
    const hosts = daemonHostsFromParticipants(participants);

    // Then
    expect(hosts.map((h) => h.instanceId)).toEqual(["udoo", "srv2"]);
  });
});

describe("parseDaemonAdvertisement", () => {
  it("extracts the advertised base clone location as reposBasePath", () => {
    // Given a daemon advertisement that includes its repos_base_path
    const meta = '{"instance_id":"h1","label":"h1 (this daemon)","repos_base_path":"repos"}';

    // When
    const host = parseDaemonAdvertisement(meta);

    // Then
    expect(host).toEqual({ instanceId: "h1", label: "h1 (this daemon)", reposBasePath: "repos" });
  });

  it("omits reposBasePath when the advertisement does not carry one", () => {
    // Given an advertisement from an older daemon with no repos_base_path
    const meta = '{"instance_id":"h1","label":"h1 (this daemon)"}';

    // When
    const host = parseDaemonAdvertisement(meta);

    // Then
    expect(host).toEqual({ instanceId: "h1", label: "h1 (this daemon)" });
  });
});

describe("daemonRpcIdentity", () => {
  it("prefixes the instance id with 'daemon-' to form the RPC-server identity", () => {
    // Given / When / Then — a daemon's discovery identity ("udoo") is distinct from the
    // participant that actually serves RPC ("daemon-udoo"); see `main.rs`'s `rpc_identity`.
    expect(daemonRpcIdentity("udoo")).toBe("daemon-udoo");
  });

  it("always applies the prefix, even for an instance id that already starts with 'daemon-'", () => {
    // Given / When / Then — instance ids are opaque strings chosen by config/hostname; one that
    // happens to start with "daemon-" itself must still get the prefix applied, not be treated
    // as already-prefixed
    expect(daemonRpcIdentity("daemon-worker-3")).toBe("daemon-daemon-worker-3");
  });
});
