//! The daemon's end of a clone's `SSH_ASKPASS` helper.
//!
//! When a clone's ssh needs a key passphrase it runs the helper, which calls the loopback endpoint
//! here. The daemon raises the existing host prompt (`PromptKind::SshKeyPassphrase`) for the GitHub
//! user who started the clone, waits for the browser's RSA-OAEP-encrypted answer, decrypts it with
//! the host's private key and hands the plaintext back to the helper — and to nothing else. ssh
//! then adds the unlocked key to the user's agent itself; the daemon never touches the agent.
//!
//! * [`grants`] — the per-clone tokens that admit a helper.
//! * [`endpoint`] — [`AskpassEndpoint`], the prompt-and-wait logic and its loopback server.
//! * [`outcome`] — what a request ends as.

mod endpoint;
mod grants;
mod http;
mod outcome;

pub use endpoint::AskpassEndpoint;
pub use grants::{CloneGrant, CloneGrants};
pub use outcome::{AskpassOutcome, AskpassReply, Passphrase};
