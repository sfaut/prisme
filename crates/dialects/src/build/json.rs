//! Functions of JSON of MySQL that give a `JSON`: the type of their result,
//! nullable whatever their arguments (`crates/probe/cases/json-functions.txt`,
//! measured on MySQL 8.0.11, 8.0.16, 8.0.17 and 8.4.11). Their result is
//! then read as a `JSON` column is: compared by the rules of JSON in `IN`
//! from 8.0.17, read as a boolean as true unless it is the number 0 from
//! 8.0.17 (`emit::json`, `emit::truth`).
//!
//! The other functions of JSON give a number or a string, of a type unknown
//! until the types of the function catalog; those of MariaDB give a string,
//! of a type unknown as well, their translation between the dialects being
//! refused.

use prisme_types::{DataType, Type};

use crate::Dialect;

/// Functions of MySQL whose result is a `JSON`.
const JSON_RESULTS: [&str; 15] = [
    "JSON_ARRAY",
    "JSON_ARRAY_APPEND",
    "JSON_ARRAY_INSERT",
    "JSON_EXTRACT",
    "JSON_INSERT",
    "JSON_KEYS",
    "JSON_MERGE",
    "JSON_MERGE_PATCH",
    "JSON_MERGE_PRESERVE",
    "JSON_OBJECT",
    "JSON_REMOVE",
    "JSON_REPLACE",
    "JSON_SCHEMA_VALIDATION_REPORT",
    "JSON_SEARCH",
    "JSON_SET",
];

/// Type of the built-in function `name`, in capitals, in `dialect`: a
/// nullable `JSON` for a function of MySQL that gives a `JSON`; `None`
/// otherwise.
pub(super) fn json_type(name: &str, dialect: Dialect) -> Option<Type> {
    (dialect == Dialect::MySql && JSON_RESULTS.contains(&name))
        .then(|| Type::nullable(DataType::Json))
}
