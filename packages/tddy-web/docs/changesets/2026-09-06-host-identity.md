# 2026-09-06 — The tooling section on a Hosts row
**Type:** Feature

`HostRowTooling` renders one `GetHostTooling` answer as two labelled cells — git identity and `gh`
state — taking `{instanceId, git, githubCli}` and fetching nothing itself.

Its guard is written **outcome-first**: `unanswered()` runs before either block's own states, and
only `ProbeOutcome.OK` licenses a finding. `ProbeOutcome` is a proto3 enum and therefore open, and
this message grows, so listing the outcomes that mean "no finding" would let an unknown value from a
newer daemon fall through and render "Not configured" with full confidence about a probe whose
answer was not understood. Listing the one outcome that permits a finding cannot.

The `gh` cell's `title` is load-bearing, not decoration: the visible `gh` label is static across
every state, so it identifies nothing. The `title` names the login as **this host's**, which is what
separates it from the tddy session user in `UserAvatar`. `hostToolingPage.expectGhLoginLabelledAsHosts`
keeps that contract in the page object, because a test asserting the cell's text contains `"gh"`
passes whatever the component does.

`HostsScreenToolingAcceptance.cy.tsx` covers one behaviour per test, and each state additionally
**denies** the neighbour it must not be confused with — per-state specs in isolation pass for a
component that collapses two states into one rendering.

Node 4 of the `#hosts-screen` stack — [PR #456](https://github.com/uppin/tddy-coder/pull/456).

See [`hosts-screen.md`](../hosts-screen.md).
