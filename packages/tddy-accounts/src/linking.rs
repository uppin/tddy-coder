//! Adding a credential without becoming its owner.
//!
//! Logging in and holding a credential are two different things, and this module is where they come
//! apart. `ExchangeCode` (`#keyring` 2/9) returns a `session_token`, a `refresh_token` and a user —
//! completing it *is* becoming that person, so running it a second time to add a second account
//! signs the first one out. Linking runs the provider's authorization dance and then stores a
//! record. It mints nothing.
//!
//! **The line this module must not cross**: a link never produces a session token. The convenient
//! implementation — call the login path and discard the token it returns — passes every positive
//! test here, and its first symptom in production is a person silently acting as someone else. The
//! message shapes in `accounts.proto` carry no token field, and
//! [`account_linking_acceptance`](../tests/account_linking_acceptance.rs) asserts it over the
//! serialised response rather than over the struct.

use tddy_credentials::{AccountId, CredentialRecord, ProviderId};

/// Metadata key holding the provider's own immutable identifier for the account.
///
/// **Dedup is on this and never on [`META_SUBJECT`]**: a GitHub login can be changed and the old
/// name re-registered by someone else, so a record keyed on the name would eventually attach a
/// stranger's token to a person's project.
pub const META_SUBJECT_ID: &str = "subject_id";

/// Metadata key holding the display name at the provider — a GitHub login. Mutable, and shown
/// beside the label as `AccountSummary.subject`.
pub const META_SUBJECT: &str = "subject";

/// Who a completed authorization turned out to be, in terms no provider owns.
///
/// Two fields because they have different lifetimes: [`subject_id`](Self::subject_id) is what the
/// provider will still call this person after they rename themselves, and
/// [`login`](Self::login) is what a person recognises on a screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedIdentity {
    /// The provider's immutable id — a GitHub user id, rendered as text. Never reused by the
    /// provider, which is what makes it safe to dedup on.
    pub subject_id: String,
    /// The provider's display handle — a GitHub login. Changeable, and therefore never an identity.
    pub login: String,
}

/// What the operator is shown when a link begins, and the handle the next poll uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkChallenge {
    /// This daemon's handle on the attempt. Deliberately **not** the provider's device code: that
    /// code is a bearer credential for completing the flow and never leaves the daemon.
    pub link_id: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in_seconds: u64,
    /// The provider's *minimum* seconds between polls.
    pub interval_seconds: u64,
}

/// Where one poll of a link attempt left it.
///
/// Carries no token in any variant except [`Approved`](Self::Approved), whose `access_token` is the
/// **provider's** credential on its way into the vault — never a session token, and never a value
/// that reaches an RPC response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkProgress {
    /// Not approved yet. Poll again after `interval_seconds`, which the provider may have widened.
    Pending { interval_seconds: u64 },
    /// Approved at the provider. Nothing is stored yet — that is the caller's next step.
    Approved {
        identity: LinkedIdentity,
        access_token: String,
    },
    /// The operator refused. Over, and never retried silently.
    Denied,
    /// The code outlived its window. A new attempt is needed.
    Expired,
}

/// Why a link could not proceed.
///
/// Shaped after `#keyring` 4/9's `AccountsError`, and holding `Locked` apart from every provider
/// outcome for the reason `LinkState` does: the provider refusing and this daemon being unable to
/// store the result are unrelated failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkError {
    /// The token names no live session. There is no key, no vault and nothing to link into.
    NoSuchSession,
    /// A vault exists and this session's key does not unwrap it.
    Locked,
    /// The `link_id` names no attempt this daemon started — expired from memory, or never issued.
    NoSuchLink,
    /// Nothing here knows how to link an account at that provider.
    UnsupportedProvider(String),
    /// Anything else, verbatim. A swallowed reason is what turns a one-line fix into an afternoon.
    Unavailable(String),
}

/// Why the store must keep an account the caller asked to forget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemovalRefusal {
    /// This is the account the caller's session was established with, and the vault's key derives
    /// from it — forgetting it would seal the store with no obvious route back.
    ///
    /// ⚠ The alternative is coherent and was considered: remove it and let the vault lock, which is
    /// what losing that credential *means*. It is refused instead because the lock would arrive
    /// with no warning and no recovery a person could find. Recorded in the `#keyring` 8/9
    /// changeset so a reviewer can overrule it.
    SessionAccount,
}

/// Run a provider's authorization dance, and nothing else.
///
/// Deliberately narrow: it begins an attempt and polls it. It does not open the vault, write a
/// record, or know what a session is — which is what makes it impossible for an implementation of
/// this port to mint one.
pub trait AccountLinker: Send + Sync {
    /// Begin an attempt at `provider`.
    fn begin(&self, provider: &ProviderId) -> Result<LinkChallenge, LinkError>;

    /// Ask the provider where the attempt stands.
    fn poll(&self, link_id: &str) -> Result<LinkProgress, LinkError>;
}

/// The write half of the credential store, as linking needs it.
///
/// Separate from `#keyring` 4/9's `AccountStore`, which is read-and-curate only by design — that
/// service "shows and curates what exists, it never links an account". Linking is the other half,
/// and it arrives as its own port rather than as a fourth method on one a parent owns.
pub trait LinkedAccountStore: Send + Sync {
    /// What this session's vault already holds at `provider` — the input to deduplication.
    fn held(
        &self,
        session_token: &str,
        provider: &ProviderId,
    ) -> Result<Vec<CredentialRecord>, LinkError>;

    /// Retain a linked credential, replacing whatever sits at the same `(provider, account)`.
    fn put(&self, session_token: &str, record: CredentialRecord) -> Result<(), LinkError>;

    /// The account this session was established with, when it was established with one.
    ///
    /// `None` is an ordinary answer: a daemon configured with a server-side credential has a
    /// session that belongs to no linked account, and every account it holds is removable.
    fn session_account(
        &self,
        session_token: &str,
    ) -> Result<Option<(ProviderId, AccountId)>, LinkError>;
}

/// The record a completed link should store, given what the vault already holds.
///
/// Pure, for the same reason `#keyring` 5/9's `resolve_account` is: deduplication and preservation
/// are the whole of the decision, and neither needs a sealed file to be tested.
///
/// A match on [`META_SUBJECT_ID`] is a **re-link**, and a re-link keeps the record's
/// `account` and `label` — a fresh account id would leave every `#keyring` 5/9 project assignment
/// pointing at nothing, and the resolver would answer `NotAssigned`, which is indistinguishable
/// from never having assigned one. That silence is why this is a function with its own tests.
#[must_use]
pub fn record_for_link(
    held: &[CredentialRecord],
    provider: &ProviderId,
    identity: &LinkedIdentity,
    access_token: &str,
    linked_at: u64,
) -> CredentialRecord {
    let _ = (held, provider, identity, access_token, linked_at);
    todo!("TODO(keyring 8/9): dedup on subject id; a re-link keeps account id and label")
}

/// Whether `RemoveAccount` may forget this account.
///
/// `session_account` is what [`LinkedAccountStore::session_account`] answered for the caller.
pub fn removal_allowed(
    target: (&ProviderId, &AccountId),
    session_account: Option<(&ProviderId, &AccountId)>,
) -> Result<(), RemovalRefusal> {
    let _ = (target, session_account);
    todo!("TODO(keyring 8/9): refuse only the account this session was established with")
}
