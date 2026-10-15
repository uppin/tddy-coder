import { useEffect, useState } from "react";
import { ProbeOutcome, type HostGithubCli } from "../../gen/host_pb";

/** Asks one host's `gh` about the project's GitHub host (`GetHostTooling` with `githubHostname`). */
export type LoadHostGithubCli = (input: {
  daemonInstanceId: string;
  githubHostname: string;
}) => Promise<HostGithubCli>;

/**
 * The GitHub host a git URL names: `https://H/..`, `ssh://git@H[:port]/..` or scp-like `git@H:..`.
 * `null` when the URL names none, so the caller can say so instead of asking about a guessed host.
 */
export function githubHostnameOf(gitUrl: string): string | null {
  const url = gitUrl.trim();
  const scpLike = /^[^/@\s]+@([^/:\s]+):(?!\/\/)/.exec(url);
  if (scpLike) return scpLike[1];
  try {
    return new URL(url).hostname || null;
  } catch {
    return null;
  }
}

/** The per-project Local gh checkbox. */
export function LocalGhToggle({
  projectId,
  checked,
  onChange,
}: {
  projectId: string;
  checked: boolean;
  onChange: (localGh: boolean) => void;
}) {
  return (
    <label className="mb-3 flex items-center gap-2 text-sm">
      <input
        type="checkbox"
        data-testid={`project-local-gh-toggle-${projectId}`}
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span>Local gh</span>
    </label>
  );
}

/** What one host's own `gh` says about the project's GitHub host, for a Local gh project. */
export function HostGithubState({
  projectId,
  daemonInstanceId,
  githubHostname,
  loadHostGithubCli,
}: {
  projectId: string;
  daemonInstanceId: string;
  githubHostname: string | null;
  loadHostGithubCli: LoadHostGithubCli;
}) {
  const [cli, setCli] = useState<HostGithubCli | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    if (githubHostname === null) return;
    let cancelled = false;
    setFailed(false);
    loadHostGithubCli({ daemonInstanceId, githubHostname })
      .then((loaded) => {
        if (!cancelled) setCli(loaded);
      })
      .catch(() => {
        if (!cancelled) setFailed(true);
      });
    return () => {
      cancelled = true;
    };
  }, [loadHostGithubCli, daemonInstanceId, githubHostname]);

  let text: string;
  if (githubHostname === null) {
    text = "gh: the project's git URL names no GitHub host";
  } else if (failed) {
    text = `gh: could not be read on this host for ${githubHostname}`;
  } else if (cli === null) {
    text = "gh: checking...";
  } else if (cli.outcome !== ProbeOutcome.OK) {
    text = `gh: could not be read${cli.failureReason ? ` (${cli.failureReason})` : ""}`;
  } else if (!cli.installed) {
    text = "gh: not installed";
  } else if (!cli.authenticated) {
    text = `gh: not logged in to ${githubHostname}`;
  } else {
    text = `gh: logged in to ${githubHostname} as ${cli.login}`;
  }

  return (
    <span
      data-testid={`project-host-gh-${projectId}-${daemonInstanceId}`}
      className="text-muted-foreground"
    >
      {text}
    </span>
  );
}
