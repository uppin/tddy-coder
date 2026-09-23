import { useEffect, useState, type FormEvent } from "react";
import { Button } from "@/components/ui/button";
import { VaultState } from "../gen/auth_pb";
import { useAuthContext } from "../hooks/authProvider";
import { inputClassName, labelClassName } from "./connection/standaloneFormStyles";

/** The daemon refuses a shorter one (`tddy_credentials::MIN_PASSPHRASE_CHARS`); saying so first. */
const MIN_PASSPHRASE_CHARS = 8;

type Mode = "unlock" | "create" | "reset";

/**
 * Asks a signed-in operator for their credential vault passphrase while the daemon holds the vault
 * closed — `LOCKED` (it restarted, or this is a browser without an unlock key) or `UNINITIALIZED`
 * (no vault yet; a first passphrase creates it). Until then the GitHub token their login received
 * is held on the daemon in memory, and GitHub-backed views report themselves unavailable.
 *
 * A forgotten passphrase leads to a reset, behind a warning: the old vault is set aside on the
 * daemon, not deleted, and the credentials in it must be linked again.
 *
 * Renders nothing for an open vault or a login the daemon keeps no vault for. "Not now" hides it
 * until the vault's state next changes — the rest of the app works without the vault.
 */
export function CredentialVaultPrompt() {
  const { isAuthenticated, vaultState, unlockVault, resetVault } = useAuthContext();
  const [resetting, setResetting] = useState(false);
  const [dismissed, setDismissed] = useState(false);
  const [passphrase, setPassphrase] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    setDismissed(false);
    setResetting(false);
    setPassphrase("");
    setConfirmation("");
    setError(null);
  }, [vaultState]);

  const asking = vaultState === VaultState.LOCKED || vaultState === VaultState.UNINITIALIZED;
  if (!isAuthenticated || !asking || dismissed) return null;

  const mode: Mode = vaultState === VaultState.UNINITIALIZED ? "create" : resetting ? "reset" : "unlock";
  const choosing = mode !== "unlock";
  const ready = choosing
    ? passphrase.length >= MIN_PASSPHRASE_CHARS && passphrase === confirmation
    : passphrase.length > 0;

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!ready || busy) return;
    setBusy(true);
    setError(null);
    try {
      if (mode === "reset") {
        await resetVault(passphrase);
      } else {
        await unlockVault(passphrase, mode === "create");
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "The credential vault could not be opened");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4">
      <form
        data-testid="credential-vault-prompt"
        onSubmit={(event) => void submit(event)}
        className="w-full max-w-md rounded-lg border border-border bg-background p-6 text-foreground shadow-lg font-sans"
      >
        <h2 className="text-lg font-semibold mt-0 mb-2">{HEADINGS[mode]}</h2>
        <p className="text-sm text-muted-foreground mt-0 mb-4">{EXPLANATIONS[mode]}</p>
        {mode === "reset" ? (
          <p data-testid="credential-vault-reset-warning" className="text-sm text-destructive mt-0 mb-4">
            A reset starts an empty vault. Your current one is set aside on the daemon, not deleted,
            but only its old passphrase opens it: every credential it held must be linked again.
          </p>
        ) : null}
        <label className={labelClassName} htmlFor="credential-vault-passphrase">
          {choosing ? "New passphrase" : "Passphrase"}
        </label>
        <input
          id="credential-vault-passphrase"
          data-testid="credential-vault-passphrase"
          type="password"
          autoComplete={choosing ? "new-password" : "current-password"}
          className={inputClassName}
          value={passphrase}
          onChange={(event) => setPassphrase(event.target.value)}
        />
        {choosing ? (
          <>
            <label className={labelClassName} htmlFor="credential-vault-passphrase-confirm">
              Type it again
            </label>
            <input
              id="credential-vault-passphrase-confirm"
              data-testid="credential-vault-passphrase-confirm"
              type="password"
              autoComplete="new-password"
              className={inputClassName}
              value={confirmation}
              onChange={(event) => setConfirmation(event.target.value)}
            />
            <p className="text-xs text-muted-foreground mt-0 mb-3">
              At least {MIN_PASSPHRASE_CHARS} characters. It is never stored: forgetting it means a reset.
            </p>
          </>
        ) : null}
        {error ? (
          <p data-testid="credential-vault-error" className="text-sm text-destructive mt-0 mb-3">
            {error}
          </p>
        ) : null}
        <div className="flex flex-wrap items-center gap-2">
          <Button type="submit" data-testid="credential-vault-submit" disabled={!ready || busy}>
            {SUBMIT_LABELS[mode]}
          </Button>
          {mode === "unlock" ? (
            <Button
              type="button"
              variant="ghost"
              data-testid="credential-vault-forgot"
              onClick={() => {
                setResetting(true);
                setPassphrase("");
                setError(null);
              }}
            >
              Forgot passphrase?
            </Button>
          ) : null}
          {mode === "reset" ? (
            <Button type="button" variant="ghost" onClick={() => setResetting(false)}>
              Back
            </Button>
          ) : null}
          <Button type="button" variant="ghost" onClick={() => setDismissed(true)}>
            Not now
          </Button>
        </div>
      </form>
    </div>
  );
}

const HEADINGS: Record<Mode, string> = {
  unlock: "Unlock your credential vault",
  create: "Create your credential vault",
  reset: "Reset your credential vault",
};

const EXPLANATIONS: Record<Mode, string> = {
  unlock:
    "The daemon keeps your GitHub credential sealed under your vault passphrase. It is locked on this daemon until you give it.",
  create:
    "Choose a passphrase to seal your GitHub credential under on this daemon. You will be asked for it again after the daemon restarts on a browser that has not unlocked it before.",
  reset: "Choose a new passphrase for a fresh, empty vault.",
};

const SUBMIT_LABELS: Record<Mode, string> = {
  unlock: "Unlock",
  create: "Create vault",
  reset: "Reset vault",
};
