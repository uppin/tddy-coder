// TODO(sharpen): lint corrections after the apply — these are the unused copied-header imports the engine's end-of-run tidy would have removed (its tidy is unreachable on a resumed run; see the changeset's Technical debt).
use crate::plan::malformed;
use crate::plan::FileHint;
use crate::Result;

/// What a v2 header says about the file at `path` as it stands: its hash and when it last changed.
pub(crate) fn hint_of(path: &std::path::Path) -> Result<FileHint> {
    let modified = std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .map_err(|error| malformed(format!("{} could not be read: {error}", path.display())))?;
    Ok(FileHint {
        sha256: crate::apply::hash_file(path)?,
        modified: rfc3339(modified),
    })
}

/// `time` as an RFC 3339 UTC timestamp, to the second — or `None` for a time this cannot state,
/// which is one before the epoch. Answering `1970-01-01` for it would be a date that is not the
/// file's, and the hint is never read, so leaving it out loses nothing.
pub(crate) fn rfc3339(time: std::time::SystemTime) -> Option<String> {
    let seconds = i64::try_from(time.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs()).ok()?;
    let (days, within_day) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));

    // Days since 1970-01-01 to a civil date: the era/day-of-era arithmetic of Howard Hinnant's
    // `civil_from_days`, which holds for every date the proleptic Gregorian calendar has.
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);

    Some(format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        within_day / 3_600,
        within_day % 3_600 / 60,
        within_day % 60
    ))
}
