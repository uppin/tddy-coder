//! What `verify` is told about methods the author detached into functions (`detach_method`), so it
//! can account for them: rule R-detach.
//!
//! A declaration, never an inference. `--detach TYPE=NAME` implies `--rebind NAME` (a detached body
//! is a rebound body). **R-detach** then pairs 1:1 into `repointed`: a signature whose receiver
//! became `NAME: &mut TYPE` / `NAME: &TYPE` with its original (`Self` written `TYPE`); a statement
//! holding `R.m(a)` with one holding `m(R, a)`, `M::m(R, a)` or `m(&mut R, a)`; `Self::m(a)` with
//! `m(a)`; and a lost `impl TYPE {` header once per block the run emptied. A detach nobody declared,
//! a changed argument and a receiver changed other than by the borrow stay reported.

use std::str::FromStr;

/// One declared detach: the methods of `impl type_name` became functions over `name`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detach {
    pub type_name: String,
    pub name: String,
}

impl FromStr for Detach {
    type Err = String;

    /// Read `TYPE=NAME`, the form `--detach` and `VerifyRequest.detaches` carry: `TYPE` one path
    /// segment, `NAME` one identifier other than `self`.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        // TODO(reshape-backend-session): implement
        Err(format!(
            "`--detach {text}` is not implemented yet (TODO(reshape-backend-session))"
        ))
    }
}
