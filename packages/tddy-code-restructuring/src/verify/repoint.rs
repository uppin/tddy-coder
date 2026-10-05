//! What `verify` is told about a call re-point the author made (`repoint_call`), so it can account
//! for it: rule R-call.
//!
//! A declaration, never an inference: from the two trees alone an inserted `.hop` cannot be told from
//! a hand edit. A plain re-point to a lowercase module (`f(` becoming `m::f(`) is already excused by
//! the re-point pass without a declaration; what needs one is a hop on a receiver (`self.slot(` becoming
//! `self.peer.slot(`), a method chain, or a path whose new qualifier is a type.
//!
//! TODO(repoint-call): the rule that reads it is not written. R-call pairs a lost and a gained
//! statement when the gained one equals the lost one with every occurrence of `from` immediately
//! followed by `(` or `::<` replaced by `to`, outside strings, comments and lifetimes, and counts
//! the pair in `Excused::repointed`. Until then a declared re-point excuses nothing.

use std::str::FromStr;

/// One declared call re-point: every call written `from(..)` is now written `to(..)`. Callee texts
/// as the plan wrote them, `self.slot=self.peer.slot`; for the bulk form the method and its new
/// hops, `.slot=.agent_roster().slot`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repoint {
    pub from: String,
    pub to: String,
}

impl FromStr for Repoint {
    type Err = String;

    /// Read `OLD=NEW`, the form `--repoint` and `VerifyRequest.repoints` carry.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text.split_once('=') {
            Some((from, to)) if !from.trim().is_empty() && !to.trim().is_empty() && from != to => {
                Ok(Repoint {
                    from: from.trim().to_string(),
                    to: to.trim().to_string(),
                })
            }
            _ => Err(format!(
                "`{text}` is not a repoint: write `OLD=NEW`, two different callee texts"
            )),
        }
    }
}
