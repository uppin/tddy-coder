# 2026-09-06 — The ssh-agent section on a Hosts row
**Type:** Feature

`HostRowSshAgent` renders one `GetHostTooling` answer's `ssh_agent` block, taking
`{instanceId, sshAgent}` and fetching nothing. `HostRowTooling` mounts it as its third section.

It repeats node 4's **outcome-first** guard rather than sharing one: only `ProbeOutcome.OK` licenses
a finding, so an outcome a newer daemon added and this bundle's generated enum cannot name renders
"Could not check". Written the other way round — listing the outcomes that mean "no finding" — an
unknown value would fall through to `reachable` and render "No agent" with full confidence about a
probe whose answer was not understood, sending an operator to start an agent that is very possibly
already running.

Past the guard it reads `reachable` before it reads the key list. "No agent" and "No keys loaded"
both arrive with an empty `keys`, and only `reachable` tells them apart, so emptiness is never the
thing consulted.

A held key renders its type, its **whole** fingerprint and its comment. The fingerprint is not
shortened: two keys can share any prefix of one, and a truncated one matches nothing in an operator's
own `ssh-add -l`. The comment is italic free text with no label suggesting it locates anything — the
agent does not know which file a key came from, and presenting a comment as a path would be a
fabricated fact.

`HostsScreenSshAgentAcceptance.cy.tsx` mounts the component directly and covers the key list, the
three summary states told apart from one another, a comment rendered as a comment rather than a path,
an outcome this bundle cannot name, and a host that has not answered yet. `hostSshAgentPage` on
`hostsScreenPage.ts` owns the selectors, including the prefix match that collects the held keys.

⚠ **Nothing in `src/` mounts `HostRowTooling` or issues `GetHostTooling`.** Its only call sites are
the Cypress specs. That is inherited rather than introduced here, but it means the section's states
are covered and the assembled path has never run.

Node 5 of the `#hosts-screen` stack — [PR #457](https://github.com/uppin/tddy-coder/pull/457).

See [`hosts-screen.md`](../hosts-screen.md).
