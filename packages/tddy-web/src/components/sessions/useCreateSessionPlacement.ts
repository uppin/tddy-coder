import { useEffect, useState } from "react";
import type { DaemonHost } from "../../lib/participantRole";
import { sshConfigListDaemonId } from "./CreateSessionSshConfigSelect";
import type { SessionType } from "./createSessionRequest";
import {
  placementAfterToggling,
  sandboxedCodebaseUnavailability,
  type CodebasePlacementChoice,
} from "./codebasePlacement";

export interface UseCreateSessionPlacementArgs {
  sessionType: SessionType;
  /** The served agent declares it can stand on one host against a codebase on another (`split_host`). */
  agentCanSplit: boolean;
  /** The served agent's spec allows a jailed (`sandbox`) placement (`placements: [jail]`). */
  agentOffersJail: boolean;
  daemons: DaemonHost[];
  /** The host the form will ask for, exactly as the form holds it (empty = the connected daemon). */
  daemonInstanceId: string;
  /** The host running the session, in the spelling the fan-out stamps its rows with. */
  agentHostInstanceId: string;
}

/**
 * Where the new session's codebase lives and what confines it: the Sandbox, Managed-codebase and
 * jailed-codebase toggles, the codebase host of a split placement, the SSH config host, and every
 * rule derived from them. Lifted out of `CreateSessionPane.tsx` with its state and its two reset
 * effects, in their original order.
 */
export function useCreateSessionPlacement({
  sessionType,
  agentCanSplit,
  agentOffersJail,
  daemons,
  daemonInstanceId,
  agentHostInstanceId,
}: UseCreateSessionPlacementArgs) {
  const [sandbox, setSandbox] = useState(false);
  // Run the agent inside a tmux session on the host, for every CLI harness type
  // (docs/ft/daemon/claude-cli-session.md § tmux mode).
  // Exclusive with every placement that is not a plain agent on this host — see `runInTmuxUnavailable`.
  const [runInTmux, setRunInTmux] = useState(false);
  const [managedCodebase, setManagedCodebase] = useState(false);
  // The inverted placement: this session's own checkout inside a `--workspace-tools` jail, with the
  // agent beside it, unconfined and stripped of its native filesystem and shell tools.
  // See docs/ft/daemon/amendments/PRD-2026-09-18-sandboxed-codebase-from-the-web.md.
  const [sandboxedCodebase, setSandboxedCodebase] = useState(false);
  // Which daemon's filesystem holds the worktree. Empty means "same as host" — the co-located
  // placement every session had before docs/ft/daemon/remote-managed-worktree.md.
  const [codebaseDaemonInstanceId, setCodebaseDaemonInstanceId] = useState("");
  // OpenSSH Host alias the exec catalog runs on. Empty is LocalShell on the code-managing host.
  const [sshConfigHost, setSshConfigHost] = useState("");
  /**
   * Whether placing the codebase on another daemon is even on offer.
   *
   * Only an agent whose spec declares `split_host`: it is the one that can be *prevented* from
   * touching a local filesystem (its spec's tool-withdrawal), so it is the only kind the daemon
   * accepts a split for. And never without a common room to name a host in.
   *
   * This is what the selector renders on. `isSplitCodebase` below builds on it rather than
   * restating it, so the control's visibility and everything a split withdraws cannot drift apart.
   */
  const canChooseCodebaseHost = agentCanSplit && managedCodebase && daemons.length > 0;

  /**
   * The session's worktree lives on a daemon other than the one running its agent — see
   * docs/ft/daemon/remote-managed-worktree.md.
   *
   * Governs everything a split cannot also ask for: the workflow recipe and the permission bypass
   * both resolve a worktree on the daemon running the agent, which a split session does not have,
   * and the daemon refuses each by name. The sandbox is not among them — on a split placement it
   * confines the codebase host (the jail runs where the checkout is), so the combination is
   * admitted. Specialized agents and the semantic index are not among them either — an agent is
   * placeable on any host, and the index is built wherever the worktree is, which on a split
   * session is the codebase host.
   */
  const isSplitCodebase =
    canChooseCodebaseHost &&
    codebaseDaemonInstanceId !== "" &&
    // Naming the session's own host is the explicit spelling of "co-located", and the daemon
    // classifies it exactly that way. Treating it as a split here would withdraw the recipe from a
    // session that is going to run with one.
    codebaseDaemonInstanceId !== daemonInstanceId;

  /** The directory's entry for the host running this session, or `null` when it names none. */
  const sessionHost = daemons.find((d) => d.instanceId === agentHostInstanceId) ?? null;

  /**
   * Why that host cannot jail this session's checkout, or `null` when it can.
   *
   * Read off the host's own advertisement rather than guessed from a platform: a daemon too old to
   * advertise the capability would answer the request field by starting an ordinary, unconfined
   * session. A host the directory does not name at all is no advertisement either.
   */
  const sandboxedCodebaseUnavailableReason = sandboxedCodebaseUnavailability(sessionHost);

  /** What that host's jail leaves unconfined, when it says so. Absent = nothing to caveat. */
  const jailSharesTheFilesystemRoot =
    sessionHost?.sandboxedCodebase?.confinesFilesystem === false;

  // A placement the selected host cannot serve is one the form must not hold: the host is pickable
  // after the placement is, and a request carrying a choice the disabled control would never have
  // allowed is how a session comes back unconfined with nothing said.
  useEffect(() => {
    if (sandboxedCodebaseUnavailableReason !== null) setSandboxedCodebase(false);
  }, [sandboxedCodebaseUnavailableReason]);

  /**
   * The placement the form currently holds, as the rules module names it.
   *
   * `Sandbox` and `Managed codebase` coexist — on a split placement the sandbox confines the
   * codebase host, so the daemon admits the pair — and either of them means the codebase is not
   * jailed, which is all the rule needs from them. Their order here is therefore immaterial.
   */
  const currentPlacement: CodebasePlacementChoice = sandboxedCodebase
    ? "sandboxedCodebase"
    : managedCodebase
      ? "managed"
      : sandbox
        ? "sandbox"
        : "none";

  /**
   * Apply the placement rule for the control the operator just toggled.
   *
   * Only the jailed codebase is exclusive with *both* of the others, so this owns exactly that
   * axis: the two older toggles keep their own state, and choosing either of them clears the jail.
   * Choosing the jail clears them, together with the codebase host only the managed section could
   * offer — a selection the operator can no longer see is one the form must no longer hold.
   *
   * The specialized agents and the Semantic index are deliberately **not** cleared: they are
   * offered on every placement now, so they are still on screen after this runs, and a placement
   * has never been what decides whether an agent may be attached.
   */
  /**
   * Whether this placement withdraws `--dangerously-skip-permissions`.
   *
   * Both placements that withdraw it confine the agent the same way — by taking its native
   * filesystem and shell tools off its argv — so on both, the deny list *is* the confinement.
   * Whether that list survives the bypass flag is not something this repo pins, so the control is
   * withheld rather than the combination assumed safe, and the daemon refuses each by name. A
   * split session withdraws it because it has no local worktree for the flag to be about; a
   * jailed codebase withdraws it because the flag would be about the one thing keeping the agent
   * out of the host's filesystem.
   * See docs/ft/daemon/amendments/PRD-2026-09-18-sandboxed-codebase-from-the-web.md
   * § What's staying the same.
   */
  const placementWithdrawsPermissionBypass = isSplitCodebase || sandboxedCodebase;

  /**
   * Whether the placement rules out a tmux-run agent. The daemon refuses `run_in_tmux` with the
   * sandbox (the agent runs in its own jail), the jailed codebase (no agent on the host to run) and
   * a split placement (the worktree is on another daemon), so the control is disabled for all three.
   */
  const runInTmuxUnavailable = sandbox || sandboxedCodebase || isSplitCodebase;

  // Choosing one of those placements clears the tmux choice rather than leaving it set behind a
  // disabled control: the screen must show what the request will carry.
  useEffect(() => {
    if (runInTmuxUnavailable) setRunInTmux(false);
  }, [runInTmuxUnavailable]);

  // The placement state is shared across session types, but only an agent whose spec declares
  // `split_host` offers the jailed codebase, and only one whose placements include `jail` the
  // sandbox. Switching to an agent without the placement clears what another agent left on, or
  // it would disable the tmux choice (and withhold it from the request) with nothing on screen
  // to say why.
  useEffect(() => {
    if (!agentOffersJail) setSandbox(false);
    if (!agentCanSplit) setSandboxedCodebase(false);
  }, [sessionType, agentOffersJail, agentCanSplit]);

  const applyPlacement = (toggled: CodebasePlacementChoice, on: boolean) => {
    const next = placementAfterToggling(currentPlacement, toggled, on);
    setSandboxedCodebase(next === "sandboxedCodebase");
    if (next === "sandboxedCodebase") {
      setSandbox(false);
      setManagedCodebase(false);
      setCodebaseDaemonInstanceId("");
    }
  };

  /**
   * Host whose `~/.ssh/config` the SSH dropdown lists. Co-located: the session host. Split: the
   * codebase host — that daemon is the OpenSSH client (n4).
   */
  const sshListDaemonId = sshConfigListDaemonId(
    agentHostInstanceId,
    codebaseDaemonInstanceId,
  );

  useEffect(() => {
    setSshConfigHost("");
  }, [sshListDaemonId]);

  return {
    sandbox,
    setSandbox,
    runInTmux,
    setRunInTmux,
    runInTmuxUnavailable,
    managedCodebase,
    setManagedCodebase,
    sandboxedCodebase,
    codebaseDaemonInstanceId,
    setCodebaseDaemonInstanceId,
    sshConfigHost,
    setSshConfigHost,
    canChooseCodebaseHost,
    isSplitCodebase,
    sandboxedCodebaseUnavailableReason,
    jailSharesTheFilesystemRoot,
    placementWithdrawsPermissionBypass,
    applyPlacement,
    sshListDaemonId,
  };
}
