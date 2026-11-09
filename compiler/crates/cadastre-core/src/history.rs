//! The history of the resources (spec § 6, History): the last change of each, from the version
//! control of the project. The host reads it (`git blame`, the core reads no file) and gives
//! the last change of each line of each file; a resource takes the latest of its own lines,
//! those of the resources it contains left to them.

use std::collections::HashMap;
use std::ops::Range;

use crate::registry::{Registry, ResourceId};
use crate::source::FileId;

/// A change of the version control: when, by whom, in which commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// Seconds since 1970-01-01 UTC.
    pub time: i64,
    pub author: String,
    pub commit: String,
}

impl Change {
    /// Its day, `2026-11-09` (UTC): compared as text, in the order of time.
    pub fn date(&self) -> String {
        date(self.time)
    }
}

/// The last change of each line of a file: its bytes in the text, its indentation left out, and
/// the change. A line not committed yet is left out.
pub type Lines = Vec<(Range<usize>, Change)>;

/// The last change of each resource, from the last change of the lines of the files.
pub fn changes(registry: &Registry, files: &[(FileId, Lines)]) -> HashMap<ResourceId, Change> {
    let by_file: HashMap<FileId, &Lines> = files.iter().map(|(file, lines)| (*file, lines)).collect();
    let mut out = HashMap::new();
    for id in registry.ids() {
        let resource = registry.resource(id);
        let span = resource.span;
        let Some(lines) = by_file.get(&span.file) else { continue };
        // The lines of a resource contained in it, wholly: theirs.
        let children: Vec<Range<usize>> = resource
            .children
            .iter()
            .map(|&c| registry.resource(c).span)
            .filter(|s| s.file == span.file)
            .map(|s| s.start..s.end)
            .collect();
        let last = lines
            .iter()
            .filter(|(bytes, _)| bytes.start < span.end && bytes.end > span.start)
            .filter(|(bytes, _)| !children.iter().any(|c| c.start <= bytes.start && bytes.end <= c.end))
            .map(|(_, change)| change)
            .max_by_key(|change| change.time);
        if let Some(change) = last {
            out.insert(id, change.clone());
        }
    }
    out
}

/// The day of a time, `YYYY-MM-DD` in UTC (the civil calendar from the days since 1970).
pub fn date(time: i64) -> String {
    let days = time.div_euclid(86_400);
    // From Howard Hinnant's `civil_from_days`.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_of_the_calendar() {
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(951_782_400), "2000-02-29");
        assert_eq!(date(1_762_646_400), "2025-11-09");
        assert_eq!(date(1_762_732_799), "2025-11-09");
        assert_eq!(date(-86_400), "1969-12-31");
    }
}
