//! Role of each pragma of SQLite, for the write switches (`classify.rs`).
//!
//! SQLite runs an unknown pragma without an error, and reads `name = value`
//! and `name(value)` alike: the value of a pragma that reads is its argument
//! (`table_info(t)`), that of any other pragma changes a setting
//! (`journal_mode(WAL)`). The text alone cannot tell them apart: Prisme keeps
//! a closed list of the pragmas of the library it ships, each with its role,
//! and refuses any other (`docs/SQL-COMPARISON.md`, section 6).
//!
//! Only the settings of the connection itself can change, as a `SET SESSION`
//! of MySQL: in the editor and in the queries run when a connection opens.
//! The other settings change the file or the whole library (`journal_mode`,
//! `user_version`, `writable_schema`...): only read.

/// What a pragma does, by the form of its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PragmaRole {
    /// Reads, without a value; a value would change a setting other than one
    /// of the connection: refused.
    Read,
    /// Reads, with an argument or without (`table_info(t)`,
    /// `integrity_check`).
    ReadArgument,
    /// Setting of the connection: reads without a value, changes the setting
    /// with one, like a `SET SESSION`.
    Setting,
    /// Writes in the file, with a value or without (`optimize`, which runs
    /// `ANALYZE`): under the "Maintenance" switch.
    Maintenance,
}

/// Settings of the connection, which a value changes (`PragmaRole::Setting`):
/// those the editor and the queries run when a connection opens can change.
pub const SESSION_PRAGMAS: [&str; 7] = [
    "busy_timeout",
    "cache_size",
    "case_sensitive_like",
    "defer_foreign_keys",
    "foreign_keys",
    "recursive_triggers",
    "temp_store",
];

/// Pragmas that read with an argument (`PragmaRole::ReadArgument`).
const ARGUMENT_PRAGMAS: [&str; 10] = [
    "foreign_key_check",
    "foreign_key_list",
    "index_info",
    "index_list",
    "index_xinfo",
    "integrity_check",
    "quick_check",
    "table_info",
    "table_list",
    "table_xinfo",
];

/// Pragmas that write in the file (`PragmaRole::Maintenance`).
const MAINTENANCE_PRAGMAS: [&str; 3] = ["incremental_vacuum", "optimize", "wal_checkpoint"];

/// Pragmas that only read (`PragmaRole::Read`): lists, counters, and the
/// settings of the file or of the library. `shrink_memory` frees the cache
/// of the connection, and writes nothing. `data_store_directory` exists in
/// the SQLite of Windows only: elsewhere, the library ignores it, as any
/// unknown pragma, and its bare form, the only one allowed, reads nothing.
const READ_PRAGMAS: [&str; 47] = [
    "analysis_limit",
    "application_id",
    "auto_vacuum",
    "automatic_index",
    "cache_spill",
    "cell_size_check",
    "checkpoint_fullfsync",
    "collation_list",
    "compile_options",
    "count_changes",
    "data_store_directory",
    "data_version",
    "database_list",
    "default_cache_size",
    "empty_result_callbacks",
    "encoding",
    "freelist_count",
    "full_column_names",
    "fullfsync",
    "function_list",
    "hard_heap_limit",
    "ignore_check_constraints",
    "journal_mode",
    "journal_size_limit",
    "legacy_alter_table",
    "locking_mode",
    "max_page_count",
    "mmap_size",
    "module_list",
    "page_count",
    "page_size",
    "pragma_list",
    "query_only",
    "read_uncommitted",
    "reverse_unordered_selects",
    "schema_version",
    "secure_delete",
    "short_column_names",
    "shrink_memory",
    "soft_heap_limit",
    "synchronous",
    "temp_store_directory",
    "threads",
    "trusted_schema",
    "user_version",
    "wal_autocheckpoint",
    "writable_schema",
];

/// Role of the pragma `name` (in lowercase), `None` for a pragma Prisme does
/// not know.
pub(crate) fn role(name: &str) -> Option<PragmaRole> {
    let listed = |names: &[&str]| names.contains(&name);
    if listed(&SESSION_PRAGMAS) {
        Some(PragmaRole::Setting)
    } else if listed(&ARGUMENT_PRAGMAS) {
        Some(PragmaRole::ReadArgument)
    } else if listed(&MAINTENANCE_PRAGMAS) {
        Some(PragmaRole::Maintenance)
    } else if listed(&READ_PRAGMAS) {
        Some(PragmaRole::Read)
    } else {
        None
    }
}

/// Does the table function of the pragma named `name` (`pragma_optimize`,
/// in lowercase) write in the file? Only `optimize`, of the pragmas that
/// write, has a table function (measured, `docs/SQL-COMPARISON.md`,
/// section 6).
pub(crate) fn function_writes(name: &str) -> bool {
    name.strip_prefix("pragma_")
        .is_some_and(|pragma| MAINTENANCE_PRAGMAS.contains(&pragma))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn knows_every_pragma_of_the_library() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        let library: BTreeSet<String> = db
            .prepare("SELECT name FROM pragma_pragma_list")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        let listed: BTreeSet<String> = [
            &SESSION_PRAGMAS[..],
            &ARGUMENT_PRAGMAS,
            &MAINTENANCE_PRAGMAS,
            &READ_PRAGMAS,
        ]
        .concat()
        .iter()
        // A pragma of the SQLite of Windows only.
        .filter(|name| cfg!(windows) || **name != "data_store_directory")
        .map(|name| name.to_string())
        .collect();
        assert_eq!(listed, library);
        assert_eq!(
            listed.len() + usize::from(!cfg!(windows)),
            SESSION_PRAGMAS.len()
                + ARGUMENT_PRAGMAS.len()
                + MAINTENANCE_PRAGMAS.len()
                + READ_PRAGMAS.len(),
            "a pragma in two lists"
        );
    }

    #[test]
    fn the_functions_that_write_are_those_of_the_library() {
        // A pragma that writes has a table function only if it is listed by
        // `function_writes`.
        let db = rusqlite::Connection::open_in_memory().unwrap();
        for pragma in MAINTENANCE_PRAGMAS {
            let function = format!("pragma_{pragma}");
            let exists = db.prepare(&format!("SELECT * FROM {function}")).is_ok();
            assert!(!exists || function_writes(&function), "{function}");
        }
        assert!(function_writes("pragma_optimize"));
        assert!(!function_writes("pragma_table_info"));
        assert!(!function_writes("optimize"));
    }
}
