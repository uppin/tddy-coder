/**
 * Acceptance tests: each host row reports the git identity that host is configured with and the
 * state of the GitHub CLI on it.
 *
 * Mounts `HostRowTooling` — this node's own component — rather than the whole screen, so a failure
 * is attributable here rather than to `#hosts-screen 1/8`'s row rendering, which is not implemented.
 *
 * Every state is asserted as *distinguishable*, not merely present: each test names the state it
 * mounts and also denies the neighbouring states it must not be confused with. A test that only
 * checks its own string passes even when the component collapses two states into one rendering —
 * and collapsing "no identity configured" into "probe failed", or "logged out" into "not
 * installed", sends an operator to fix the wrong thing.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-09-06-host-identity.md
 */

import { create } from "@bufbuild/protobuf";
import {
  HostGitIdentitySchema,
  HostGithubCliSchema,
  ProbeOutcome,
} from "../../src/gen/connection_pb";
import { HostRowTooling } from "../../src/components/hosts/HostRowTooling";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { hostToolingPage } from "../support/pages/hostsScreenPage";

const HOST = "workstation-1";

function mountTooling(
  git: ReturnType<typeof create<typeof HostGitIdentitySchema>> | undefined,
  gh: ReturnType<typeof create<typeof HostGithubCliSchema>> | undefined,
) {
  mountWithRpc(
    withSelectedDaemon(<HostRowTooling instanceId={HOST} git={git} githubCli={gh} />),
    anInMemoryRpcBackend(),
  );
}

// The wire states, named so each test reads as the situation on a host rather than as proto
// construction. Keeping them here rather than inline is what lets a test body be one line of
// Given and one of Then.

const aGitIdentityOf = (userName: string, userEmail: string) =>
  create(HostGitIdentitySchema, { outcome: ProbeOutcome.OK, configured: true, userName, userEmail });

const noGitIdentityConfigured = () =>
  create(HostGitIdentitySchema, { outcome: ProbeOutcome.OK, configured: false });

const aGitProbeThatFailed = (failureReason: string) =>
  create(HostGitIdentitySchema, { outcome: ProbeOutcome.FAILED, failureReason });

const ghNotInstalled = () =>
  create(HostGithubCliSchema, { outcome: ProbeOutcome.OK, installed: false, authenticated: false });

const ghLoggedOut = () =>
  create(HostGithubCliSchema, { outcome: ProbeOutcome.OK, installed: true, authenticated: false });

const ghAuthenticatedAs = (login: string) =>
  create(HostGithubCliSchema, {
    outcome: ProbeOutcome.OK,
    installed: true,
    authenticated: true,
    login,
  });

describe("Hosts screen tooling", () => {
  it("shows the git identity a host is configured with", () => {
    // Given — a host whose OS user has both halves of an identity set
    // When — the row reports on it
    mountTooling(aGitIdentityOf("Ada Lovelace", "ada@example.com"), ghNotInstalled());

    // Then — both halves are shown, and it does not read as an unconfigured or unreachable host
    hostToolingPage.gitIdentity(HOST).should("contain.text", "Ada Lovelace");
    hostToolingPage.gitIdentity(HOST).should("contain.text", "ada@example.com");
    hostToolingPage.gitIdentity(HOST).should("not.contain.text", "Not configured");
    hostToolingPage.gitIdentity(HOST).should("not.contain.text", "Could not check");
  });

  it("reports a host with no git identity as not configured", () => {
    // Given — a probe that ran fine and found no identity
    // When — the row reports on it
    mountTooling(noGitIdentityConfigured(), ghNotInstalled());

    // Then — a real finding, and not confused with a probe that could not run
    hostToolingPage.gitIdentity(HOST).should("contain.text", "Not configured");
    hostToolingPage.gitIdentity(HOST).should("not.contain.text", "Could not check");
  });

  it("distinguishes a host with no git identity from one that failed to report", () => {
    // Given — a probe that could not run at all
    // When — the row reports on it
    mountTooling(aGitProbeThatFailed("git not on PATH"), ghNotInstalled());

    // Then — it admits it does not know, rather than claiming the host has no identity. These two
    // send an operator to different places: one is a host to go and configure, the other is not.
    hostToolingPage.gitIdentity(HOST).should("contain.text", "Could not check");
    hostToolingPage.gitIdentity(HOST).should("not.contain.text", "Not configured");
  });

  it("reports a host without the github cli as not installed", () => {
    // Given — a host where `gh` is absent
    // When — the row reports on it
    mountTooling(noGitIdentityConfigured(), ghNotInstalled());

    // Then — not confused with an installed-but-logged-out host, which is a different fix
    hostToolingPage.githubCli(HOST).should("contain.text", "Not installed");
    hostToolingPage.githubCli(HOST).should("not.contain.text", "Not authenticated");
  });

  it("reports a host with the github cli logged out as not authenticated", () => {
    // Given — a host where `gh` is installed but logged out
    // When — the row reports on it
    mountTooling(noGitIdentityConfigured(), ghLoggedOut());

    // Then — not confused with `gh` being absent, and it claims no login
    hostToolingPage.githubCli(HOST).should("contain.text", "Not authenticated");
    hostToolingPage.githubCli(HOST).should("not.contain.text", "Not installed");
  });

  it("reports the login the github cli on a host is authenticated as", () => {
    // Given — a host whose `gh` is authenticated
    // When — the row reports on it
    mountTooling(noGitIdentityConfigured(), ghAuthenticatedAs("octocat"));

    // Then — the login is shown, and neither negative state is rendered alongside it. Without the
    // denials, a component that rendered "Not authenticated (octocat)" would still pass.
    hostToolingPage.githubCli(HOST).should("contain.text", "octocat");
    hostToolingPage.githubCli(HOST).should("not.contain.text", "Not authenticated");
    hostToolingPage.githubCli(HOST).should("not.contain.text", "Not installed");
  });

  it("labels the gh login as the hosts rather than the signed in user", () => {
    // Given — a host whose `gh` is authenticated as a login that is not the tddy session user
    // When — the row reports on it
    mountTooling(noGitIdentityConfigured(), ghAuthenticatedAs("octocat"));

    // Then — the cell says whose login this is. Three GitHub identities can disagree — the tddy
    // session user in `UserAvatar`, a GITHUB_TOKEN in some environment, and the host's own `gh` —
    // so a bare login beside the row's other identities reads as whichever one the operator
    // expected. Asserting the static "gh" label alone would prove nothing: it renders in every
    // state, for every login, and even when there is no login at all.
    hostToolingPage
      .githubCli(HOST)
      .should("have.attr", "title")
      .and("match", /this host's/i);
    hostToolingPage.githubCli(HOST).should("have.attr", "title").and("contain", "octocat");
  });

  it("says nothing about a host that has not answered yet", () => {
    // Given — no probe result for this host, the state a row is in before the RPC answers
    // When — the row reports on it
    mountTooling(undefined, undefined);

    // Then — it must not borrow the shape of an answer. "Not configured" or "Not installed" here
    // would be a finding about a host nothing has asked about yet.
    hostToolingPage.gitIdentity(HOST).should("not.contain.text", "Not configured");
    hostToolingPage.githubCli(HOST).should("not.contain.text", "Not installed");
    hostToolingPage.githubCli(HOST).should("not.contain.text", "Not authenticated");
  });
});
