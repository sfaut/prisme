//! Flavors of the sources: the product a connector detects when it connects
//! (MySQL or MariaDB through the same driver), and what Prisme knows of
//! each one.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Flavor of the server, detected on connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum Flavor {
    /// MySQL server (Oracle).
    MySql,
    /// MariaDB server.
    MariaDb,
}

impl Flavor {
    /// Oldest version of the flavor that Prisme supports: the first general
    /// availability release of MySQL 8.0 and of MariaDB 10.6. An older server
    /// still works, with a warning (`docs/V1.md`, section 2.10).
    pub fn oldest_supported(self) -> &'static str {
        match self {
            Self::MySql => "8.0.11",
            Self::MariaDb => "10.6.3",
        }
    }
}
