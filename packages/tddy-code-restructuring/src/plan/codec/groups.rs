// TODO(sharpen): lint corrections after the apply — these are the unused copied-header imports the engine's end-of-run tidy would have removed (its tidy is unreachable on a resumed run; see the changeset's Technical debt).
use crate::plan::malformed;
use crate::plan::RefactorOp;
use crate::Result;

/// Refuse a group whose members have an operation of another group, or of none, between them: that
/// operation would be applied inside a unit it is not part of, and rolled back with it.
pub(super) fn refuse_split_groups(ops: &[RefactorOp]) -> Result<()> {
    let mut closed = std::collections::BTreeSet::new();
    let mut current: Option<&str> = None;
    for op in ops {
        let group = op.group.as_deref();
        if group == current {
            continue;
        }
        if let Some(finished) = current {
            closed.insert(finished);
        }
        if let Some(group) = group.filter(|group| closed.contains(group)) {
            return Err(malformed(format!(
                "group `{group}` is split: its members must be consecutive operations, and \
                 another operation sits between them"
            )));
        }
        current = group;
    }
    Ok(())
}
