//! What `verify` is told about methods the author detached into functions (`detach_method`), so it
//! can account for them: rule R-detach (the changeset's R7).
//!
//! A declaration, never an inference. `--detach TYPE=NAME` implies `--rebind NAME`, because a
//! detached body is a rebound body. **R-detach** then pairs 1:1 into `repointed`: a signature whose
//! receiver became `NAME: &mut TYPE` / `NAME: &TYPE` with its original (`Self` written `TYPE`); a
//! statement holding `R.m(a)` with one holding `m(R, a)`, `M::m(R, a)`, `m(&mut R, a)` or
//! `m(&R, a)`; `Self::m(a)` with `m(a)`; and one lost `impl TYPE {` header per block the run
//! emptied. A detach nobody declared, a changed argument and a receiver changed other than by the
//! borrow stay reported.

use std::str::FromStr;

use super::retarget::Passed;

/// One declared detach: the members of `impl ty` became functions over `name`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detach {
    /// The block's self type, one bare path segment.
    pub ty: String,
    /// The parameter the receiver became, one identifier other than `self`.
    pub name: String,
}

impl FromStr for Detach {
    type Err = String;

    /// Read `TYPE=NAME`, the form `--detach` and `VerifyRequest.detaches` carry.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        // TODO(reshape-detach-method): implement
        let _ = text;
        todo!("TODO(reshape-detach-method): read a `--detach TYPE=NAME` declaration")
    }
}

/// R-detach: pair what the declared detaches produce, one to one, over what R-rebind left.
pub(crate) fn account(
    missing: Vec<String>,
    added: Vec<String>,
    declared: &super::Declared,
) -> Passed {
    if declared.detaches.is_empty() {
        return Passed {
            missing,
            added,
            pairs: 0,
        };
    }
    // TODO(reshape-detach-method): implement
    todo!("TODO(reshape-detach-method): R-detach")
}
