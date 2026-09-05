/**
 * Acceptance spec: the host directory works with LiveKit switched off.
 *
 * Today `SelectedDaemonProvider` is the host list: it joins a common room and reads the daemons off
 * its participants. With no `livekitUrl` / `commonRoom`, `useCommonRoom` short-circuits to `idle`,
 * `daemons` stays `[]`, and the selector offers **nothing** — not even the daemon serving the page,
 * which `/api/config` has always named as `daemon_instance_id`. That is why a `tddy-desktop` which
 * does not join a common room by default can reach no host at all.
 *
 * These specs pin the replacement: a directory merged from sources, where an unconfigured source
 * contributes nothing and reports `idle` rather than `error`, and the serving daemon is always
 * offered.
 *
 * Changeset: `docs/dev/1-WIP/2026-09-05-optional-livekit-host-directory.md`
 * Stack: `optional-livekit` node 2 of 7.
 */

import React from "react";
import { useHostDirectory } from "../../src/rpc/hostDirectory/useHostDirectory";
import { useHostPresence } from "../../src/rpc/hostDirectory/useHostPresence";
import type { HostDirectorySource } from "../../src/rpc/hostDirectory/types";
import { HostDirectorySources } from "../../src/rpc/hostDirectory/useHostDirectory";
import { byTestId } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const THIS_HOST = "instance-this-host";
const A_PEER = "instance-a-peer";

/** The daemon serving this page, from `/api/config`'s `daemon_instance_id`. */
function aServingSource(): HostDirectorySource {
  return {
    id: "serving",
    status: "connected",
    error: null,
    hosts: [{ hostId: THIS_HOST, label: "this daemon", sourceId: "serving" }],
  };
}

/** A common room that was never configured: no hosts, and idle rather than broken. */
function anUnconfiguredLiveKitSource(): HostDirectorySource {
  return { id: "livekit", status: "idle", error: null, hosts: [] };
}

/** A common room that is up, advertising one peer. */
function aConnectedLiveKitSource(): HostDirectorySource {
  return {
    id: "livekit",
    status: "connected",
    error: null,
    hosts: [{ hostId: A_PEER, label: "laptop-b (this daemon)", sourceId: "livekit" }],
  };
}

/** A common room that cannot be reached. */
function aFailedLiveKitSource(): HostDirectorySource {
  return {
    id: "livekit",
    status: "error",
    error: "could not reach the LiveKit server",
    hosts: [],
  };
}

// ---------------------------------------------------------------------------
// Probes
// ---------------------------------------------------------------------------

/** Renders the directory the way a host selector would read it. */
function DirectoryProbe() {
  const directory = useHostDirectory();
  return (
    <div>
      <div data-testid="host-ids">{directory.hosts.map((h) => h.hostId).join(",") || "none"}</div>
      <div data-testid="directory-status">{directory.status}</div>
      <div data-testid="directory-error">{directory.error ?? "no error"}</div>
      <div data-testid="livekit-source-status">
        {directory.sources.find((s) => s.id === "livekit")?.status ?? "absent"}
      </div>
    </div>
  );
}

/** Asks for a host's participant roster by name, which a presence-less connection refuses. */
function PresenceProbe({ hostId }: { hostId: string }) {
  const presence = useHostPresence(hostId);
  return <div data-testid="presence">{presence ? "available" : "unavailable"}</div>;
}

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

describe("the host directory with LiveKit unconfigured", () => {
  it("offers the daemon serving the page when no common room is configured", () => {
    // Given a page with no `livekitUrl` and no `commonRoom` — tddy-desktop's default
    cy.mount(
      <HostDirectorySources sources={[aServingSource(), anUnconfiguredLiveKitSource()]}>
        <DirectoryProbe />
      </HostDirectorySources>,
    );

    // Then the serving daemon is offered and the directory is usable. Today this list is empty
    // and the selector shows nothing at all.
    byTestId("host-ids").should("have.text", THIS_HOST);
    byTestId("directory-status").should("have.text", "connected");
  });

  it("calls an unconfigured common room idle, never an error", () => {
    // Given the same page
    cy.mount(
      <HostDirectorySources sources={[aServingSource(), anUnconfiguredLiveKitSource()]}>
        <DirectoryProbe />
      </HostDirectorySources>,
    );

    // Then nothing reports a failure. An operator who deliberately did not configure LiveKit
    // must not be shown a connection error for it on every screen.
    byTestId("livekit-source-status").should("have.text", "idle");
    byTestId("directory-error").should("have.text", "no error");
  });

  it("refuses presence on a host whose connection has none", () => {
    // Given a host reached without LiveKit
    cy.mount(
      <HostDirectorySources sources={[aServingSource(), anUnconfiguredLiveKitSource()]}>
        <PresenceProbe hostId={THIS_HOST} />
      </HostDirectorySources>,
    );

    // Then a component that wants the participant roster is told it is unavailable, rather than
    // reaching a `Room` off a shared context. This is the seam node 4 gates the presence
    // surfaces on.
    byTestId("presence").should("have.text", "unavailable");
  });
});

describe("the host directory with LiveKit configured", () => {
  it("shows the serving daemon alongside the common room's peers", () => {
    // Given a page that has both
    cy.mount(
      <HostDirectorySources sources={[aServingSource(), aConnectedLiveKitSource()]}>
        <DirectoryProbe />
      </HostDirectorySources>,
    );

    // Then both are selectable in the same session — which is what lets a desktop app keep its own
    // host on IPC while reaching its peers over LiveKit
    byTestId("host-ids").should("have.text", `${THIS_HOST},${A_PEER}`);
    byTestId("directory-status").should("have.text", "connected");
  });

  it("keeps the local host usable when the common room fails", () => {
    // Given a reachable serving daemon and a common room that will not connect
    cy.mount(
      <HostDirectorySources sources={[aServingSource(), aFailedLiveKitSource()]}>
        <DirectoryProbe />
      </HostDirectorySources>,
    );

    // Then the directory is still connected and the local host is still offered. A LiveKit
    // failure degrades the peers, not the host the operator is sitting in front of.
    byTestId("directory-status").should("have.text", "connected");
    byTestId("host-ids").should("have.text", THIS_HOST);
    byTestId("livekit-source-status").should("have.text", "error");
  });
});
