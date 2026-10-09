//! What `verify` is told about a `self` the author rebound (`read_fields_through`), so it can
//! account for it: rule R-rebind.
//!
//! A declaration, never an inference. **R-rebind** pairs a lost and a gained statement 1:1 when the
//! gained one equals the lost one with every whole-identifier `self` (outside strings, comments and
//! lifetimes) written `NAME`, a `&` directly before a rebound `self.` allowed to have gone (the
//! borrow the state value already provides), and excuses one gained `let NAME = …;` per
//! declaration. A rebind nobody declared, a second `let` and a changed argument stay reported.

use std::str::FromStr;

/// One declared rebind: `self` in a range is now written `name`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rebind {
    pub name: String,
}

impl FromStr for Rebind {
    type Err = String;

    /// Read `NAME`, the form `--rebind` and `VerifyRequest.rebinds` carry: one identifier other
    /// than `self`.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        // TODO(reshape-methods-leave-type): implement
        Err(format!(
            "`--rebind {text}` is not implemented yet (TODO(reshape-methods-leave-type))"
        ))
    }
}
