//! Outcome of a MySQL or MariaDB statement, from its OK packet.
//!
//! The OK packet gives the number of affected rows, the auto-increment id
//! of the first row inserted (generated, or written by the statement), the
//! number of warnings, and an info text whose counts
//! depend on the statement: `Records: 3  Duplicates: 1  Warnings: 0` for an
//! insertion of several rows, `Rows matched: 3  Changed: 2  Warnings: 0` for
//! an `UPDATE`. The same `Duplicates` means skipped rows for an
//! `INSERT IGNORE`, replaced rows for a `REPLACE` and updated rows for an
//! upsert: the verb of the statement (`StatementVerb`) tells which. The
//! affected rows count the rows changed, not those found, and twice a row
//! that an upsert updates (`docs/SQL-COMPARISON.md`, section "Outcome of a
//! write"). This module turns all of it into the neutral counts of
//! `StatementOutcome`, and leaves empty what the packet does not tell.
//!
//! The packet only counts the warnings: their text is read by
//! `SHOW WARNINGS` (`notice`), before the next statement clears them.

use prisme_dialects::verb::StatementVerb;

use crate::outcome::{Notice, NoticeLevel, OutcomeCounts, StatementOutcome};

/// What the OK packet of a statement holds.
#[derive(Debug, Default)]
pub(super) struct OkPacket<'a> {
    /// Number of affected rows.
    pub(super) affected: u64,
    /// Auto-increment id of the first row inserted, 0 or absent for none.
    pub(super) last_insert_id: Option<u64>,
    /// Number of warnings and notes.
    pub(super) warnings: u16,
    /// Info text, empty when the server sends none (a single row, a
    /// `DELETE`).
    pub(super) info: &'a str,
}

/// Counts of an info text, by their label.
#[derive(Debug, Default, PartialEq, Eq)]
struct Info {
    /// `Records`: rows the statement handled.
    records: Option<u64>,
    /// `Duplicates`: rows whose key already existed.
    duplicates: Option<u64>,
    /// `Rows matched`: rows the condition of an `UPDATE` found.
    matched: Option<u64>,
    /// `Changed`: rows whose values changed.
    changed: Option<u64>,
}

/// Reads the counts of an info text, label by label; an unknown label is
/// left out.
fn parse_info(text: &str) -> Info {
    let mut info = Info::default();
    let mut rest = text;
    while let Some((label, after)) = rest.split_once(':') {
        let after = after.trim_start();
        let digits = after.len() - after.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        let value = after[..digits].parse().ok();
        match label.trim() {
            "Records" => info.records = value,
            "Duplicates" => info.duplicates = value,
            "Rows matched" => info.matched = value,
            "Changed" => info.changed = value,
            _ => {}
        }
        rest = &after[digits..];
    }
    info
}

/// Outcome of a statement of verb `verb`, from its OK packet.
pub(super) fn outcome(verb: StatementVerb, packet: &OkPacket) -> StatementOutcome {
    let affected = packet.affected;
    let info = parse_info(packet.info);
    StatementOutcome {
        affected: Some(affected),
        counts: counts(verb, affected, &info),
        inserted_id: packet
            .last_insert_id
            .filter(|id| *id > 0)
            .map(|id| id.to_string()),
        warnings: Some(u64::from(packet.warnings)),
        notices: Vec::new(),
    }
}

/// Notice of a row of `SHOW WARNINGS`: its level (`Note`, `Warning`,
/// `Error`), its code and its message. An unknown level counts as a
/// warning.
pub(super) fn notice(level: &str, code: u32, message: String) -> Notice {
    let level = match level {
        "Note" => NoticeLevel::Note,
        "Error" => NoticeLevel::Error,
        _ => NoticeLevel::Warning,
    };
    Notice {
        level,
        code: code.to_string(),
        message,
    }
}

/// Counts of a statement of verb `verb`, from its affected rows and its info
/// text. Without an info text, a single row was written.
fn counts(verb: StatementVerb, affected: u64, info: &Info) -> OutcomeCounts {
    match verb {
        // `INSERT IGNORE` skips the duplicates; a plain `INSERT` has none.
        StatementVerb::Insert => OutcomeCounts {
            inserted: Some(affected),
            ignored: info.duplicates,
            ..OutcomeCounts::default()
        },
        // Each replaced row counts twice: deleted, then inserted.
        StatementVerb::Replace => {
            let replaced = info.duplicates.unwrap_or(u64::from(affected > 1));
            let records = info.records.unwrap_or(1);
            OutcomeCounts {
                inserted: Some(records.saturating_sub(replaced)),
                replaced: Some(replaced),
                ..OutcomeCounts::default()
            }
        }
        // A new row counts once, an updated row twice, a row left as it was
        // not at all; `Duplicates` counts the updated rows.
        StatementVerb::Upsert => match (info.records, info.duplicates) {
            (Some(records), Some(changed)) => {
                let inserted = affected.saturating_sub(2 * changed);
                OutcomeCounts {
                    inserted: Some(inserted),
                    changed: Some(changed),
                    unchanged: Some(records.saturating_sub(inserted + changed)),
                    ..OutcomeCounts::default()
                }
            }
            _ => OutcomeCounts {
                inserted: Some(u64::from(affected == 1)),
                changed: Some(u64::from(affected == 2)),
                unchanged: Some(u64::from(affected == 0)),
                ..OutcomeCounts::default()
            },
        },
        StatementVerb::Update => OutcomeCounts {
            matched: info.matched,
            changed: Some(info.changed.unwrap_or(affected)),
            unchanged: info
                .matched
                .zip(info.changed)
                .map(|(matched, changed)| matched.saturating_sub(changed)),
            ..OutcomeCounts::default()
        },
        StatementVerb::Delete => OutcomeCounts {
            deleted: Some(affected),
            ..OutcomeCounts::default()
        },
        // `Records` counts the rows copied by a `CREATE TABLE ... SELECT` as
        // well as by an `ALTER TABLE`: no count of its own.
        StatementVerb::Truncate | StatementVerb::Schema | StatementVerb::Other => {
            OutcomeCounts::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Counts of a statement of verb `verb`, from its affected rows and its
    /// info text, as measured on MySQL 9.7.4 and MariaDB 12.3.3.
    fn counted(verb: StatementVerb, affected: u64, info: &str) -> OutcomeCounts {
        let packet = OkPacket {
            affected,
            info,
            ..OkPacket::default()
        };
        outcome(verb, &packet).counts
    }

    #[test]
    fn reads_the_labels_of_the_info_text() {
        assert_eq!(
            parse_info("Records: 3  Duplicates: 1  Warnings: 0"),
            Info {
                records: Some(3),
                duplicates: Some(1),
                ..Info::default()
            }
        );
        assert_eq!(
            parse_info("Rows matched: 3  Changed: 2  Warnings: 0"),
            Info {
                matched: Some(3),
                changed: Some(2),
                ..Info::default()
            }
        );
        assert_eq!(parse_info(""), Info::default());
        assert_eq!(parse_info("Something else"), Info::default());
    }

    #[test]
    fn counts_the_insertions_and_the_skipped_duplicates() {
        let plain = counted(
            StatementVerb::Insert,
            3,
            "Records: 3  Duplicates: 0  Warnings: 0",
        );
        assert_eq!((plain.inserted, plain.ignored), (Some(3), Some(0)));
        let ignore = counted(
            StatementVerb::Insert,
            1,
            "Records: 2  Duplicates: 1  Warnings: 1",
        );
        assert_eq!((ignore.inserted, ignore.ignored), (Some(1), Some(1)));
        let single = counted(StatementVerb::Insert, 1, "");
        assert_eq!((single.inserted, single.ignored), (Some(1), None));
    }

    #[test]
    fn counts_the_replaced_rows_once() {
        let several = counted(
            StatementVerb::Replace,
            3,
            "Records: 2  Duplicates: 1  Warnings: 0",
        );
        assert_eq!((several.inserted, several.replaced), (Some(1), Some(1)));
        let new = counted(StatementVerb::Replace, 1, "");
        assert_eq!((new.inserted, new.replaced), (Some(1), Some(0)));
        let replaced = counted(StatementVerb::Replace, 2, "");
        assert_eq!((replaced.inserted, replaced.replaced), (Some(0), Some(1)));
    }

    #[test]
    fn tells_the_new_updated_and_unchanged_rows_of_an_upsert() {
        // (1, 1) already there, (2, 7) changes a = 1, (20, 20) is new.
        let several = counted(
            StatementVerb::Upsert,
            3,
            "Records: 3  Duplicates: 1  Warnings: 0",
        );
        assert_eq!(
            (several.inserted, several.changed, several.unchanged),
            (Some(1), Some(1), Some(1))
        );
        for (affected, expected) in [
            (1, (Some(1), Some(0), Some(0))),
            (2, (Some(0), Some(1), Some(0))),
            (0, (Some(0), Some(0), Some(1))),
        ] {
            let single = counted(StatementVerb::Upsert, affected, "");
            assert_eq!(
                (single.inserted, single.changed, single.unchanged),
                expected
            );
        }
    }

    #[test]
    fn tells_the_found_rows_of_an_update_from_the_changed_ones() {
        let update = counted(
            StatementVerb::Update,
            2,
            "Rows matched: 3  Changed: 2  Warnings: 0",
        );
        assert_eq!(
            (update.matched, update.changed, update.unchanged),
            (Some(3), Some(2), Some(1))
        );
        let same = counted(
            StatementVerb::Update,
            0,
            "Rows matched: 3  Changed: 0  Warnings: 0",
        );
        assert_eq!(
            (same.matched, same.changed, same.unchanged),
            (Some(3), Some(0), Some(3))
        );
    }

    #[test]
    fn counts_the_deleted_rows_and_nothing_for_the_schema() {
        assert_eq!(counted(StatementVerb::Delete, 4, "").deleted, Some(4));
        assert_eq!(
            counted(
                StatementVerb::Schema,
                6,
                "Records: 6  Duplicates: 0  Warnings: 0"
            ),
            OutcomeCounts::default()
        );
    }

    #[test]
    fn reads_the_levels_of_the_warnings() {
        let truncated = notice("Warning", 1265, "Data truncated".to_string());
        assert_eq!(truncated.level, NoticeLevel::Warning);
        assert_eq!(truncated.code, "1265");
        assert_eq!(notice("Note", 1051, String::new()).level, NoticeLevel::Note);
        assert_eq!(
            notice("Error", 1062, String::new()).level,
            NoticeLevel::Error
        );
        assert_eq!(
            notice("Other", 1, String::new()).level,
            NoticeLevel::Warning
        );
    }

    #[test]
    fn keeps_the_inserted_id_and_the_warnings() {
        let packet = OkPacket {
            affected: 3,
            last_insert_id: Some(42),
            warnings: 2,
            info: "Records: 3  Duplicates: 0  Warnings: 2",
        };
        let reported = outcome(StatementVerb::Insert, &packet);
        assert_eq!(reported.affected, Some(3));
        assert_eq!(reported.inserted_id.as_deref(), Some("42"));
        assert_eq!(reported.warnings, Some(2));
        let none = OkPacket {
            last_insert_id: Some(0),
            ..OkPacket::default()
        };
        assert_eq!(outcome(StatementVerb::Update, &none).inserted_id, None);
    }
}
