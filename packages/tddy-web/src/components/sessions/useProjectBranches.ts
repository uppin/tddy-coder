import { useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { SessionService } from "../../gen/session_pb";
import type { ProjectService } from "../../gen/project_pb";
import type { BranchWorktreeIntent } from "../../lib/branchConflict";
import { localBranchName } from "../../lib/branchNames";

export interface UseProjectBranchesArgs {
  /**
   * Only a dependency, never called: the effect below re-runs on it because that is what
   * `CreateSessionPane` has always listed, and narrowing the list is a behaviour change, not a
   * tidy-up. `projectClient` is the one the read goes through.
   */
  client: Client<typeof SessionService>;
  projectClient: Client<typeof ProjectService>;
  sessionToken: string;
  projectId: string;
  branchIntent: BranchWorktreeIntent;
  daemonInstanceId: string;
  /** The caller's pre-filled branch, which wins over the first entry while the project offers it. */
  preFilledBranchToWorkOn: string;
  /**
   * Called with the branch the load settles on. The selection itself is the form's, not this
   * hook's — the request sends it and the picker renders it.
   */
  setSelectedBranchToWorkOn: (branch: string) => void;
}

/**
 * The remote-tracking branches the "Work on existing branch" picker offers, loaded whenever the
 * project or the branch mode changes. Lifted out of `CreateSessionPane.tsx` with its state; the
 * effect body is unchanged.
 */
export function useProjectBranches({
  client,
  projectClient,
  sessionToken,
  projectId,
  branchIntent,
  daemonInstanceId,
  preFilledBranchToWorkOn,
  setSelectedBranchToWorkOn,
}: UseProjectBranchesArgs): string[] {
  const [remoteBranches, setRemoteBranches] = useState<string[]>([]);


  // Load branches when projectId changes and intent is work_on_selected_branch
  useEffect(() => {
    if (!projectId || branchIntent !== "work_on_selected_branch") return;
    let cancelled = false;
    projectClient
      .listProjectBranches({ sessionToken, projectId, daemonInstanceId })
      .then((resp) => {
        if (!cancelled) {
          setRemoteBranches(resp.branches);
          if (resp.branches.length > 0) {
            // A pre-filled branch wins over the default first entry, but only while the project
            // actually offers it — otherwise the <select> would hold a value none of its options
            // match, and submit would send a branch this project does not have.
            //
            // Matched on the *local* branch name behind each option, because `ListProjectBranches`
            // lists remote-tracking refs (`<remote>/<branch>`) while callers name the branch the way
            // the rest of the domain does. Comparing the raw strings never matches, and the
            // pre-fill then degrades silently into an unrelated branch — the operator resumes the
            // wrong branch with no warning. The remote is the daemon-resolved default
            // (`resp.defaultRemote`), so a non-`origin` project strips the right prefix.
            const remote = resp.defaultRemote || "origin";
            const wanted = localBranchName(preFilledBranchToWorkOn, remote);
            const offered = resp.branches.find((b) => localBranchName(b, remote) === wanted);
            setSelectedBranchToWorkOn(offered ?? resp.branches[0]!);
          }
        }
      })
      .catch((err) => {
        if (!cancelled) {
          console.debug("[CreateSessionPane] listProjectBranches error", err);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [client, sessionToken, projectId, branchIntent, daemonInstanceId, preFilledBranchToWorkOn]);

  return remoteBranches;
}
