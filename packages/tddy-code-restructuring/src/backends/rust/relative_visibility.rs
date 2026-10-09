//! A relative visibility keeps its meaning when its item moves one module deeper.
//!
//! `extract_module` relocates items into a new child of the module that held them. The pass that
//! puts a widened item back "as written" restores the visibility **text**, and for the absolute
//! forms (`pub`, `pub(crate)`, `pub(in crate::…)`) the text is the meaning. For the relative ones it
//! is not: `pub(super)` written in `imports` means "visible in `rust`"; the same text in the new
//! child `imports::local_uses` means "visible in `imports`". The parent's `pub(super) use` facade
//! then fails `E0364` and every caller outside fails `E0603`.
//!
//! The rule is one level per extraction, which is all `extract_module` does: a relative path gains
//! one `super::`, and `self` becomes `super`.

use super::{declares_item, visibility_in, ModuleBlock, MovedItem, WIDENED};

/// `visibility` as it must be written one module deeper to be visible in the same place.
pub(super) fn rebased_for_child(visibility: &str) -> String {
    let Some(inner) = visibility
        .strip_prefix("pub(")
        .and_then(|rest| rest.strip_suffix(')'))
    else {
        return visibility.to_string();
    };

    match inner.trim() {
        "super" => "pub(in super::super)".to_string(),
        "self" => "pub(super)".to_string(),
        scoped => match scoped.strip_prefix("in ").map(str::trim) {
            Some(path) => rebased_path(path).map_or_else(
                || visibility.to_string(),
                |rebased| format!("pub(in {rebased})"),
            ),
            None => visibility.to_string(),
        },
    }
}

/// The `in` path of a relative scope, one level deeper, or `None` for an absolute one.
fn rebased_path(path: &str) -> Option<String> {
    if path == "self" {
        return Some("super".to_string());
    }
    if let Some(below_self) = path.strip_prefix("self::") {
        return Some(format!("super::{below_self}"));
    }
    let relative = path == "super" || path.starts_with("super::");
    relative.then(|| format!("super::{path}"))
}

/// `line` with the visibility it opens with rebased; the rest of the line is untouched.
///
/// Only the leading visibility is read, so a `pub(super)` in a comment or a string is left alone.
pub(super) fn rebased_declaration(line: &str) -> String {
    let indent_len = line.len() - line.trim_start().len();
    let (indent, rest) = line.split_at(indent_len);
    let visibility = visibility_in(rest);

    format!(
        "{indent}{}{}",
        rebased_for_child(&visibility),
        &rest[visibility.len()..]
    )
}

/// The declaration `line` of a widened `item`, put back at the visibility it was written with, as
/// that visibility reads from inside the new module.
///
/// An item the seam moved inside an inline module of its own has not changed its place relative to
/// that module, so its visibility is written as it was.
pub(super) fn put_back(line: &str, item: &MovedItem) -> String {
    let indent = &line[..line.len() - line.trim_start().len()];
    let rest = line.trim_start().strip_prefix(WIDENED).unwrap_or_default();
    if item.visibility.is_empty() {
        return format!("{indent}{rest}");
    }

    let written = format!("{indent}{} {rest}", item.visibility);
    if item.within.is_empty() {
        rebased_declaration(&written)
    } else {
        written
    }
}

/// Rebase the declaration of `item` in the new module when the assist left it at the relative
/// visibility it was written with, and say whether it did.
///
/// The assist widens a private item to `pub(crate)`, which [`put_back`] handles; a `pub(super)` it
/// does not touch at all, so the text in the module is the original and reads one module too shallow.
/// Whether anything outside reaches the item does not matter: it is rewritten either way.
pub(super) fn rebase_left_as_written(
    source: &mut [String],
    block: &ModuleBlock,
    item: &MovedItem,
) -> bool {
    if !item.within.is_empty() || rebased_for_child(&item.visibility) == item.visibility {
        return false;
    }

    let prefix = format!("{} ", item.visibility);
    let Some(index) = (block.opened..block.closed).find(|&index| {
        source[index]
            .trim_start()
            .strip_prefix(prefix.as_str())
            .is_some_and(|rest| declares_item(rest, &item.name))
    }) else {
        return false;
    };

    source[index] = rebased_declaration(&source[index]);
    true
}

/// Rebase the relative visibility of every field line in the body of the moved struct `item`, and
/// say how many it rewrote.
///
/// The assist leaves a `pub(super)` field as written, like a `pub(super)` item, so it reads one
/// module too shallow in the new child. Only a block-form struct body is read: a field line is one
/// indented deeper than the declaration, before the line that closes it.
pub(super) fn rebase_field_visibilities(
    source: &mut [String],
    block: &ModuleBlock,
    item: &MovedItem,
) -> usize {
    if !item.within.is_empty() {
        return 0;
    }
    let Some(opened) = (block.opened..block.closed)
        .find(|&index| declares_struct(&source[index], &item.name) && source[index].ends_with('{'))
    else {
        return 0;
    };
    let indent = source[opened].len() - source[opened].trim_start().len();

    let mut rewritten = 0;
    for line in source[opened + 1..block.closed].iter_mut() {
        if line.len() - line.trim_start().len() <= indent && !line.trim().is_empty() {
            break;
        }
        let rebased = rebased_declaration(line);
        if rebased != *line {
            *line = rebased;
            rewritten += 1;
        }
    }
    rewritten
}

fn declares_struct(line: &str, name: &str) -> bool {
    let rest = line.trim_start();
    let rest = rest[visibility_in(rest).len()..].trim_start();
    rest.strip_prefix("struct ")
        .is_some_and(|after| declares_item(&format!("struct {after}"), name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moved(visibility: &str, within: &[&str]) -> MovedItem {
        MovedItem {
            name: "helper".to_string(),
            within: within.iter().map(|name| name.to_string()).collect(),
            visibility: visibility.to_string(),
            stranded_in: Vec::new(),
            reached_from_outside: false,
            reached_from_production: false,
            referenced_in_impl_at: Vec::new(),
        }
    }

    #[test]
    fn rebases_super_to_the_grandparent() {
        assert_eq!(rebased_for_child("pub(super)"), "pub(in super::super)");
    }

    #[test]
    fn puts_one_more_super_in_front_of_a_super_path() {
        assert_eq!(
            rebased_for_child("pub(in super::x)"),
            "pub(in super::super::x)"
        );
        assert_eq!(
            rebased_for_child("pub(in super::super::x)"),
            "pub(in super::super::super::x)"
        );
    }

    #[test]
    fn rebases_an_explicit_self_to_super() {
        assert_eq!(rebased_for_child("pub(self)"), "pub(super)");
    }

    #[test]
    fn rebases_a_self_path_to_a_super_path() {
        assert_eq!(rebased_for_child("pub(in self::x)"), "pub(in super::x)");
    }

    #[test]
    fn leaves_the_absolute_forms_as_they_are() {
        for absolute in ["pub", "pub(crate)", "pub(in crate::a::b)", ""] {
            assert_eq!(rebased_for_child(absolute), absolute);
        }
    }

    #[test]
    fn keeps_the_rest_of_a_declaration_line_byte_identical() {
        assert_eq!(
            rebased_declaration("    pub(super) fn f(a: u32) -> u32 {  a }"),
            "    pub(in super::super) fn f(a: u32) -> u32 {  a }"
        );
    }

    #[test]
    fn leaves_a_line_without_a_qualifier_as_it_is() {
        assert_eq!(rebased_declaration("    fn f() {}"), "    fn f() {}");
        assert_eq!(rebased_declaration("pub fn f() {}"), "pub fn f() {}");
    }

    #[test]
    fn does_not_touch_a_qualifier_in_a_comment_or_a_string() {
        for line in [
            "    // pub(super) fn f() {}",
            "    let note = \"pub(super) fn f\";",
        ] {
            assert_eq!(rebased_declaration(line), line);
        }
    }

    #[test]
    fn puts_a_pub_super_item_back_as_visible_where_it_was() {
        let widened = "    pub(crate) fn helper() -> u32 {";

        assert_eq!(
            put_back(widened, &moved("pub(super)", &[])),
            "    pub(in super::super) fn helper() -> u32 {"
        );
    }

    #[test]
    fn puts_a_private_item_back_private_and_a_pub_crate_item_back_as_written() {
        let widened = "    pub(crate) fn helper() -> u32 {";

        assert_eq!(
            put_back(widened, &moved("", &[])),
            "    fn helper() -> u32 {"
        );
        assert_eq!(put_back(widened, &moved("pub(crate)", &[])), widened);
    }

    fn block_of(source: &[String]) -> ModuleBlock {
        super::super::module_bounds(source, "inner").unwrap()
    }

    #[test]
    fn rebases_a_pub_super_declaration_the_assist_left_as_written() {
        let mut source: Vec<String> = "mod inner {\n    pub(super) fn helper() {}\n}"
            .split('\n')
            .map(str::to_string)
            .collect();
        let block = block_of(&source);

        let handled = rebase_left_as_written(&mut source, &block, &moved("pub(super)", &[]));

        assert!(handled);
        assert_eq!(source[1], "    pub(in super::super) fn helper() {}");
    }

    #[test]
    fn leaves_an_absolute_declaration_for_the_pass_to_handle() {
        let mut source: Vec<String> = "mod inner {\n    pub(crate) fn helper() {}\n}"
            .split('\n')
            .map(str::to_string)
            .collect();
        let block = block_of(&source);

        assert!(!rebase_left_as_written(
            &mut source,
            &block,
            &moved("pub(crate)", &[])
        ));
        assert_eq!(source[1], "    pub(crate) fn helper() {}");
    }

    #[test]
    fn keeps_the_text_of_an_item_the_seam_moved_inside_a_module_of_its_own() {
        let widened = "        pub(crate) fn helper() -> u32 {";

        assert_eq!(
            put_back(widened, &moved("pub(super)", &["inner"])),
            "        pub(super) fn helper() -> u32 {"
        );
    }

    fn lines_of(text: &str) -> Vec<String> {
        text.split('\n').map(str::to_string).collect()
    }

    fn named(name: &str) -> MovedItem {
        MovedItem {
            name: name.to_string(),
            ..moved("", &[])
        }
    }

    #[test]
    fn rebases_a_pub_super_field_of_a_moved_struct() {
        let mut source = lines_of(
            "mod inner {\n    pub(crate) struct Gauge {\n        pub(super) level: u32,\n        pub(in super::x) scale: u32,\n        pub(crate) kept: u32,\n        private: u32,\n    }\n}",
        );
        let block = block_of(&source);

        let rewritten = rebase_field_visibilities(&mut source, &block, &named("Gauge"));

        assert_eq!(rewritten, 2);
        assert_eq!(source[2], "        pub(in super::super) level: u32,");
        assert_eq!(source[3], "        pub(in super::super::x) scale: u32,");
        assert_eq!(source[4], "        pub(crate) kept: u32,");
        assert_eq!(source[5], "        private: u32,");
    }

    #[test]
    fn leaves_the_fields_of_a_struct_that_was_not_moved_and_comments_alone() {
        let mut source = lines_of(
            "mod inner {\n    struct Other {\n        // pub(super) a: u32,\n        pub(super) b: u32,\n    }\n    pub(super) fn f() {}\n}",
        );
        let block = block_of(&source);
        let before = source.clone();

        assert_eq!(
            rebase_field_visibilities(&mut source, &block, &named("Gauge")),
            0
        );
        assert_eq!(source, before);
    }
}
