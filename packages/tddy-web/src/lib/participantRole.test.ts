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

  it("carries each daemon's advertised attachment cap onto the host it yields", () => {
    // Given two daemons advertising different attachment caps
    const participants = [
      {
        identity: "udoo",
        metadata:
          '{"instance_id":"udoo","label":"udoo (this daemon)","max_attachment_bytes":67108864}',
      },
      {
        identity: "srv2",
        metadata: '{"instance_id":"srv2","label":"srv2 (this daemon)"}',
      },
    ];

    // When
    const hosts = daemonHostsFromParticipants(participants);

    // Then the cap reaches the host list the Start-Session form selects from — this is the only
    // path from the common room to `useDaemons()`, so a cap dropped here is a cap the form can
    // never enforce
    expect(hosts).toEqual([
      { instanceId: "udoo", label: "udoo (this daemon)", maxAttachmentBytes: 67108864 },
      { instanceId: "srv2", label: "srv2 (this daemon)" },
    ]);
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

  it("extracts the advertised attachment size cap as maxAttachmentBytes", () => {
    // Given a daemon advertising a 64 MiB attachment cap
    const meta =
      '{"instance_id":"h1","label":"h1 (this daemon)","max_attachment_bytes":67108864}';

    // When
    const host = parseDaemonAdvertisement(meta);

    // Then
    expect(host).toEqual({
      instanceId: "h1",
      label: "h1 (this daemon)",
      maxAttachmentBytes: 67108864,
    });
  });

  it("omits maxAttachmentBytes when the advertisement does not carry one", () => {
    // Given an advertisement from a daemon predating the cap
    const meta = '{"instance_id":"h1","label":"h1 (this daemon)"}';

    // When
    const host = parseDaemonAdvertisement(meta);

    // Then — absent rather than zero, so the form can tell "no cap advertised" from "cap of 0"
    expect(host).toEqual({ instanceId: "h1", label: "h1 (this daemon)" });
  });

  it("ignores a non-numeric attachment cap rather than trusting it", () => {
    // Given an advertisement whose cap is not a number
    const meta =
      '{"instance_id":"h1","label":"h1 (this daemon)","max_attachment_bytes":"lots"}';

    // When
    const host = parseDaemonAdvertisement(meta);

    // Then
    expect(host).toEqual({ instanceId: "h1", label: "h1 (this daemon)" });
  });

  it("ignores a zero or negative attachment cap, which would refuse every file", () => {
    // Given an advertisement with a nonsensical cap
    const meta = '{"instance_id":"h1","label":"h1 (this daemon)","max_attachment_bytes":0}';

    // When
    const host = parseDaemonAdvertisement(meta);

    // Then — a cap of 0 would make every attachment unpickable; treat it as unadvertised
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
