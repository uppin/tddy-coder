//! What `verify` is told about an `impl` retarget the author made, so it can account for it.
//!
//! A declaration, never an inference: from the two trees alone a retarget cannot be told from a hand
//! edit, and `verify` exists to show hand edits. Without one `verify` behaves exactly as before.
//!
//! TODO(retarget-impl): the two rules that read it are not written. R1 (rename pairing) pairs a lost
//! and a gained statement that differ only by every whole-identifier `from` becoming `to`; R2
//! (header accounting) excuses at most two further inherent `impl` headers per declared retarget.
//! Both join the passes between visibility pairing and re-point pairing and count into
//! `Excused::repointed`. Until then a declared retarget excuses nothing.

use std::str::FromStr;

/// The retargets an author declares to `verify`, one per `retarget_impl` they ran.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Declared {
    pub retargets: Vec<Retarget>,
}

/// One declared retarget: the members of `impl from` became members of `impl to`. Bare type
/// identifiers, generics stripped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Retarget {
    pub from: String,
    pub to: String,
}

impl FromStr for Retarget {
    type Err = String;

    /// Read `OLD=NEW`, the form `--retarget` and `VerifyRequest.retargets` carry.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let identifier = |name: &str| {
            let mut characters = name.chars();
            characters
                .next()
                .is_some_and(|first| first.is_alphabetic() || first == '_')
                && characters.all(|rest| rest.is_alphanumeric() || rest == '_')
        };
        match text.split_once('=') {
            Some((from, to)) if identifier(from) && identifier(to) && from != to => Ok(Retarget {
                from: from.to_string(),
                to: to.to_string(),
            }),
            _ => Err(format!(
                "`{text}` is not a retarget: write `OLD=NEW`, two different bare type names"
            )),
        }
    }
}

impl Declared {
    /// The declaration the texts name, or the first that is not one.
    pub fn from_texts<'a>(texts: impl IntoIterator<Item = &'a String>) -> Result<Declared, String> {
        Ok(Declared {
            retargets: texts
                .into_iter()
                .map(|text| text.parse())
                .collect::<Result<_, _>>()?,
        })
    }
}
