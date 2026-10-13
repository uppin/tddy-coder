//! What `verify` is told about a `self` the author rebound (`read_fields_through`), so it can
//! account for it: rule R-rebind.
//!
//! A declaration, never an inference. **R-rebind** pairs a lost and a gained statement 1:1 when the
//! gained one equals the lost one with every whole-identifier `self` (outside strings, comments and
//! lifetimes) written `NAME`, a `&` directly before a rebound `self.` allowed to have gone (the
//! borrow the state value already provides), and excuses one gained `let NAME = …;` per
//! declaration. A rebind nobody declared, a second `let` and a changed argument stay reported.

use std::collections::BTreeMap;
use std::str::FromStr;

use super::retarget::{is_word_byte, string_end, word_end, Passed};

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
        let mut characters = text.chars();
        let identifier = characters
            .next()
            .is_some_and(|first| first.is_alphabetic() || first == '_')
            && characters.all(|rest| rest.is_alphanumeric() || rest == '_');
        if identifier && text != "self" {
            Ok(Rebind {
                name: text.to_string(),
            })
        } else {
            Err(format!(
                "`{text}` is not a rebind: write `NAME`, one identifier other than `self`"
            ))
        }
    }
}

/// The most borrows of one statement that are tried as dropped, in every combination.
const MOST_DROPPABLE_BORROWS: usize = 8;

/// R-rebind: pair each lost statement with a gained one equal to it once every `self` is written as
/// a declared name (and any of its borrows dropped), one to one; then excuse one gained
/// `let NAME = …;` per declaration.
pub(crate) fn account(
    missing: Vec<String>,
    added: Vec<String>,
    declared: &super::Declared,
) -> Passed {
    if declared.rebinds.is_empty() {
        return Passed {
            missing,
            added,
            pairs: 0,
        };
    }

    let mut available: BTreeMap<&str, usize> = BTreeMap::new();
    for statement in &added {
        *available.entry(statement).or_default() += 1;
    }
    let mut consumed: BTreeMap<String, usize> = BTreeMap::new();
    let mut unpaired = Vec::new();
    let mut pairs = 0;
    for statement in missing {
        let candidates = declared
            .rebinds
            .iter()
            .flat_map(|rebind| candidates_of(&statement, &rebind.name));
        let taken = candidates.into_iter().find(|candidate| {
            let used = consumed.get(candidate).copied().unwrap_or(0);
            available.get(candidate.as_str()).copied().unwrap_or(0) > used
        });
        match taken {
            Some(candidate) => {
                *consumed.entry(candidate).or_default() += 1;
                pairs += 1;
            }
            None => unpaired.push(statement),
        }
    }
    for rebind in &declared.rebinds {
        let binding = format!("let {} = ", rebind.name);
        let one_let = added.iter().find(|statement| {
            statement.starts_with(&binding)
                && statement.ends_with(';')
                && available.get(statement.as_str()).copied().unwrap_or(0)
                    > consumed.get(*statement).copied().unwrap_or(0)
        });
        if let Some(statement) = one_let {
            *consumed.entry(statement.clone()).or_default() += 1;
            pairs += 1;
        }
    }

    let added = added
        .into_iter()
        .filter(|statement| match consumed.get_mut(statement) {
            Some(left) if *left > 0 => {
                *left -= 1;
                false
            }
            _ => true,
        })
        .collect();
    Passed {
        missing: unpaired,
        added,
        pairs,
    }
}

/// `statement` with every `self` written `name`: as it stands, then with each combination of the
/// borrows directly before a `self.` dropped.
fn candidates_of(statement: &str, name: &str) -> Vec<String> {
    let (_, borrows) = rebound(statement, name, 0);
    (0..1usize << borrows.min(MOST_DROPPABLE_BORROWS))
        .map(|dropped| rebound(statement, name, dropped).0)
        .collect()
}

/// `text` with every whole-identifier `self` written `name`, leaving string literals, comments and
/// lifetimes as written, and the number of `&` / `&mut ` directly before a `self.` found. The
/// `dropped` bit `k` removes the `k`th of them.
fn rebound(text: &str, name: &str, dropped: usize) -> (String, usize) {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut borrows = 0;
    let mut at = 0;
    while at < chars.len() {
        let character = chars[at];
        if character == '"' {
            let end = string_end(&chars, at);
            out.extend(&chars[at..end]);
            at = end;
        } else if character == '/' && chars.get(at + 1) == Some(&'/') {
            out.extend(&chars[at..]);
            break;
        } else if character == '\'' && is_word_byte(chars.get(at + 1)) {
            let end = word_end(&chars, at + 1);
            out.extend(&chars[at..end]);
            at = end;
        } else if character == '&' {
            match borrowed_self(&chars, at) {
                Some(self_at) => {
                    if dropped & (1 << borrows.min(usize::BITS as usize - 1)) == 0 {
                        out.extend(&chars[at..self_at]);
                    }
                    borrows += 1;
                    at = self_at;
                }
                None => {
                    out.push(character);
                    at += 1;
                }
            }
        } else if is_word_byte(Some(&character)) {
            let end = word_end(&chars, at);
            let word: String = chars[at..end].iter().collect();
            out.push_str(if word == "self" { name } else { &word });
            at = end;
        } else {
            out.push(character);
            at += 1;
        }
    }
    (out, borrows)
}

/// The offset of the `self` that the `&` (or `&mut `) at `at` borrows a field of, when it does.
fn borrowed_self(chars: &[char], at: usize) -> Option<usize> {
    let mut cursor = at + 1;
    if chars[cursor..].starts_with(&['m', 'u', 't', ' ']) {
        cursor += "mut ".len();
    }
    let is_self = chars[cursor..].starts_with(&['s', 'e', 'l', 'f'])
        && chars.get(cursor + "self".len()) == Some(&'.');
    let own_word = at == 0 || !is_word_byte(chars.get(at - 1));
    (is_self && own_word).then_some(cursor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_every_self_as_the_name_and_leaves_strings_comments_and_lifetimes_alone() {
        assert_eq!(
            rebound("f(self.a, \"self\", &'a mut selfish) // self", "state", 0).0,
            "f(state.a, \"self\", &'a mut selfish) // self"
        );
    }

    #[test]
    fn drops_the_borrow_of_a_field_only_when_asked_to() {
        assert_eq!(rebound("g(&self.a, &mut self.b)", "s", 0).1, 2);
        assert_eq!(
            rebound("g(&self.a, &mut self.b)", "s", 0b01).0,
            "g(s.a, &mut s.b)"
        );
        assert_eq!(
            rebound("g(&self.a, &mut self.b)", "s", 0b10).0,
            "g(&s.a, s.b)"
        );
    }
}
