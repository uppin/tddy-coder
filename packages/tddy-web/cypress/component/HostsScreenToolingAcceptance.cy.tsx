/**
 * Acceptance tests: each host row reports the git identity that host is configured with and the
 * state of the GitHub CLI on it.
 *
 * Mounts `HostRowTooling` — this node's own component — rather than the whole screen, so a failure
 * is attributable here rather than to `#hosts-screen 1/8`'s row rendering, which is not implemented.
 *
 * The six states are asserted as *distinguishable*, not merely present. Collapsing "no identity
 * configured" into "probe failed", or "logged out" into "not installed", sends an operator to fix
 * the wrong thing.
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
  git: ReturnType<typeof create<typeof HostGitIdentitySchema>>,
  gh: ReturnType<typeof create<typeof HostGithubCliSchema>>,
) {
  mountWithRpc(
    withSelectedDaemon(<HostRowTooling instanceId={HOST} git={git} githubCli={gh} />),
    anInMemoryRpcBackend(),
  );
}

const NO_GH = create(HostGithubCliSchema, {
  outcome: ProbeOutcome.OK,
  installed: false,
  authenticated: false,
});

describe("Hosts screen tooling", () => {
  it("shows the git identity a host is configured with", () => {
    mountTooling(
      create(HostGitIdentitySchema, {
        outcome: ProbeOutcome.OK,
        configured: true,
        userName: "Ada Lovelace",
        userEmail: "ada@example.com",
      }),
      NO_GH,
    );

    hostToolingPage.gitIdentity(HOST).should("contain.text", "Ada Lovelace");
    hostToolingPage.gitIdentity(HOST).should("contain.text", "ada@example.com");
  });

  it("distinguishes a host with no git identity from one that failed to report", () => {
    mountTooling(
      create(HostGitIdentitySchema, { outcome: ProbeOutcome.OK, configured: false }),
      NO_GH,
    );
    hostToolingPage.gitIdentity(HOST).should("contain.text", "Not configured");

    mountTooling(
      create(HostGitIdentitySchema, {
        outcome: ProbeOutcome.FAILED,
        failureReason: "git not on PATH",
      }),
      NO_GH,
    );
    hostToolingPage.gitIdentity(HOST).should("contain.text", "Could not check");
    hostToolingPage.gitIdentity(HOST).should("not.contain.text", "Not configured");
  });

  it("distinguishes gh absent from gh logged out from gh authenticated", () => {
    const git = create(HostGitIdentitySchema, { outcome: ProbeOutcome.OK, configured: false });

    mountTooling(git, NO_GH);
    hostToolingPage.githubCli(HOST).should("contain.text", "Not installed");

    mountTooling(
      git,
      create(HostGithubCliSchema, {
        outcome: ProbeOutcome.OK,
        installed: true,
        authenticated: false,
      }),
    );
    hostToolingPage.githubCli(HOST).should("contain.text", "Not authenticated");

    mountTooling(
      git,
      create(HostGithubCliSchema, {
        outcome: ProbeOutcome.OK,
        installed: true,
        authenticated: true,
        login: "octocat",
      }),
    );
    hostToolingPage.githubCli(HOST).should("contain.text", "octocat");
  });

  it("labels the gh login as the hosts rather than the signed in user", () => {
    mountTooling(
      create(HostGitIdentitySchema, { outcome: ProbeOutcome.OK, configured: false }),
      create(HostGithubCliSchema, {
        outcome: ProbeOutcome.OK,
        installed: true,
        authenticated: true,
        login: "octocat",
      }),
    );

    // Named as the host's gh login, so it is not read as the tddy session user in UserAvatar.
    hostToolingPage.githubCli(HOST).should("contain.text", "gh");
  });
});
