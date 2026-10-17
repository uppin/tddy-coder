use crate::crate_move::manifest_edits::target_tables;
use crate::edit::TextEdit;

/// The dependency lines a destination's manifest gains for what the moved code names: those for
/// its plain table, and those for each `[target.'<cfg>'.…]` table, keyed by the header verbatim.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct CarriedLines {
    pub(crate) plain: Vec<String>,
    pub(crate) by_target: std::collections::BTreeMap<String, Vec<String>>,
}
/// One `with_lines_under` edit per target table the destination's manifest `text` has to gain
/// lines in, each keyed by its header as the origin wrote it.
pub(super) fn target_table_edits<'a>(
    text: &str,
    tables: impl Iterator<Item = (&'a String, &'a Vec<String>)>,
) -> Vec<TextEdit> {
    tables
        .flat_map(|(header, lines)| target_tables::with_lines_under(text, header, lines))
        .collect()
}
