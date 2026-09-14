/**
 * Acceptance tests: each host row lists the explicit OpenSSH Host aliases from that host's
 * `~/.ssh/config`, and distinguishes listed / empty / failed.
 *
 * A failed read must never look like "no Host aliases": empty is LocalShell as a choice, failure
 * is not a choice.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-09-14-ssh-config-hosts.md
 */

import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  HostService,
  ListSshConfigHostsResponseSchema,
  ProbeOutcome,
  SshConfigHostSchema,
} from "../../src/gen/host_pb";
import { HostRowSshConfig } from "../../src/components/hosts/HostRowSshConfig";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { hostSshConfigPage } from "../support/pages/hostsScreenPage";

const HOST = "workstation-1";

function aListingOf(aliases: string[]) {
  return create(ListSshConfigHostsResponseSchema, {
    outcome: ProbeOutcome.OK,
    hosts: aliases.map((alias) => create(SshConfigHostSchema, { alias })),
  });
}

function mountConfig(backend = anInMemoryRpcBackend()) {
  mountWithRpc(withSelectedDaemon(<HostRowSshConfig instanceId={HOST} />), backend);
}

describe("Hosts screen SSH config", () => {
  it("lists each explicit Host alias from this host's config", () => {
    // Given — this host's config names buildbox
    const backend = anInMemoryRpcBackend().implement(HostService, {
      listSshConfigHosts: async () => aListingOf(["buildbox"]),
    });

    // When
    mountConfig(backend);

    // Then
    hostSshConfigPage.section(HOST).should("contain.text", "buildbox");
    hostSshConfigPage.alias(HOST, "buildbox").should("exist");
    hostSshConfigPage.section(HOST).should("not.contain.text", "No SSH hosts");
    hostSshConfigPage.section(HOST).should("not.contain.text", "Could not check");
  });

  it("distinguishes no aliases from a config that could not be read", () => {
    // Given — a readable config with no Host aliases
    const empty = anInMemoryRpcBackend().implement(HostService, {
      listSshConfigHosts: async () => aListingOf([]),
    });

    // When
    mountConfig(empty);

    // Then — empty is a finding, not a failure
    hostSshConfigPage.section(HOST).should("contain.text", "No SSH hosts");
    hostSshConfigPage.section(HOST).should("not.contain.text", "Could not check");

    // Given — an unreadable config
    const failed = anInMemoryRpcBackend().implement(HostService, {
      listSshConfigHosts: async () =>
        create(ListSshConfigHostsResponseSchema, {
          outcome: ProbeOutcome.FAILED,
          failureReason: "permission denied",
        }),
    });

    // When
    mountConfig(failed);

    // Then — never dressed as "no destinations"
    hostSshConfigPage.section(HOST).should("contain.text", "Could not check");
    hostSshConfigPage.section(HOST).should("not.contain.text", "No SSH hosts");
  });
});
