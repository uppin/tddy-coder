import { useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { SessionService } from "../../gen/session_pb";
import type { ProjectService } from "../../gen/project_pb";
import type { BranchWorktreeIntent } from "../../lib/branchConflict";
import { localBranchName } from "../../lib/branchNames";
import { rpcFailureText } from "../../rpc/rpcFailure";

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

/** The branches the picker offers, and why the last load failed (`null` while it has not). */
export interface ProjectBranches {
  remoteBranches: string[];
  /** Ready to render beside the picker; cleared by the next successful load. */
  error: string | null;
}

/**
 * The remote-tracking branches the "Work on existing branch" picker offers, loaded whenever the
 * project or the branch mode changes. Lifted out of `CreateSessionPane.tsx` with its state. A failed
 * load keeps the last good list and reports why, so the picker never reads as "no branches".
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
}: UseProjectBranchesArgs): ProjectBranches {
  const [remoteBranches, setRemoteBranches] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);


  // Load branches when projectId changes and intent is work_on_selected_branch
  useEffect(() => {
    if (!projectId || branchIntent !== "work_on_selected_branch") return;
    let cancelled = false;
    projectClient
      .listProjectBranches({ sessionToken, projectId, daemonInstanceId })
      .then((resp) => {
        if (!cancelled) {
          setRemoteBranches(resp.branches);
          setError(null);
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
      .catch((err: unknown) => {
        if (!cancelled) setError(rpcFailureText(err));
      });
    return () => {
      cancelled = true;
    };
  }, [client, sessionToken, projectId, branchIntent, daemonInstanceId, preFilledBranchToWorkOn]);

  return { remoteBranches, error };
}
