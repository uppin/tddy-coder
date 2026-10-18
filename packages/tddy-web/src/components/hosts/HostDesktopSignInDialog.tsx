/**
 * Asks the operator to sign in to a host's desktop, shaped by the methods that desktop advertises
 * (`#host-desktops 8/8`).
 *
 * Answers a `DESKTOP_CREDENTIALS` question. The fields are the chosen method's and no others:
 *
 * | Method | Fields |
 * |---|---|
 * | VNC Authentication | password |
 * | VeNCrypt Plain / TLSPlain, Apple Remote Desktop, RDP TLS | username, password |
 * | RDP NLA | username, domain (optional), password |
 *
 * The strongest advertised method is preselected and the operator can switch to any other one the
 * desktop advertised. **Remember** asks the host to keep the credential in the operator's
 * credential vault; unticked, it is used once and kept nowhere.
 *
 * Built on `HostPassphraseDialog`'s guarantees: the host's key fingerprint is shown and pinned, a
 * changed key blocks the answer, and the answer leaves this component only as ciphertext — a
 * protobuf-encoded `DesktopCredentialAnswer`, RSA-OAEP-encrypted under the key the question
 * carried. A desktop sign-in never reuses the superseded `VncPassphraseDialog` / `vnc_*` surface.
 *
 * `error` is the desktop's own reason for refusing the last sign-in, shown when the host asks
 * again.
 */

import type React from "react";
import type { DesktopAuthMethod } from "../../gen/screen_sharing_pb";
import type { HostPromptEventLike } from "../../rpc/useHostPrompts";

export interface HostDesktopSignInDialogProps {
  /** The `DESKTOP_CREDENTIALS` question being answered: its host, subject and key. */
  question: HostPromptEventLike;
  /** The methods the desktop advertises that tddy can sign in with, as `InspectHostDesktop` read them. */
  offered: DesktopAuthMethod[];
  /** Receives the RSA-OAEP ciphertext of the encoded `DesktopCredentialAnswer` — nothing else. */
  onSubmit: (answer: Uint8Array) => void;
  onCancel: () => void;
  /** Why the desktop refused the previous sign-in, in its own words. */
  error?: string;
}

export function HostDesktopSignInDialog(props: HostDesktopSignInDialogProps): React.ReactElement {
  void props;
  throw new Error("TODO(sign-in-form): implement");
}
