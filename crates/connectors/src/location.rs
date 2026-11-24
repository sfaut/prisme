//! Locations of the databases held in files, and the providers that reach
//! them.
//!
//! A database is designated by a location whose protocol is explicit
//! (`docs/TODO-SQLITE.md`, decision 1): a bare absolute path or a `file:///`
//! URL (`file`), a public `http://` or `https://` URL, or `:memory:`
//! (`memory`). `Location::parse` reads what the user typed; a protocol Prisme
//! knows but does not serve yet (`s3`, `ftp`...) is refused by an error that
//! names it.
//!
//! Each protocol has its provider behind the contract `LocationProvider`,
//! given by the registry `locations::of`: what it can do, the state of a
//! location (`probe`), the creation of an empty database (`create`) and the
//! local form an engine opens (`materialize`): the path of a file, a copy of
//! a remote file in the cache, or a database in memory. An engine names in
//! its capabilities the protocols it reads natively; the others go through
//! their provider. A new protocol is added by a provider, a row of the
//! reference table `schemes`, and its entry in `Scheme`
//! (`docs/DEVELOPER.md`, section "Adding a protocol").

mod file;
mod http;
mod memory;

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use http::purge;

/// Longest location kept, in characters (`connection_files.location`).
pub const ADDRESS_MAX_LENGTH: usize = 2048;

/// What the user types for a database in memory.
pub const MEMORY_INPUT: &str = ":memory:";

/// Protocols Prisme knows without serving them yet: their locations are
/// refused by `LocationError::NotServed`, which names them.
const KNOWN_SCHEMES: [&str; 9] = [
    "s3", "gs", "az", "abfss", "ftp", "ftps", "sftp", "webdav", "hf",
];

/// Protocol of a location, among those Prisme serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum Scheme {
    /// A file reachable by the process of Prisme.
    File,
    /// A file on a public web server, without encryption.
    Http,
    /// A file on a public web server.
    Https,
    /// A database in memory, which lives with its session.
    Memory,
}

impl Scheme {
    /// Every protocol, as the reference table `schemes` of the internal
    /// database lists them.
    pub const ALL: [Self; 4] = [Self::File, Self::Http, Self::Https, Self::Memory];

    /// Key of the protocol, as serialized and as the internal database
    /// stores it.
    pub fn key(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Http => "http",
            Self::Https => "https",
            Self::Memory => "memory",
        }
    }

    /// Protocol of the key `key`.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|scheme| scheme.key() == key)
    }
}

impl std::fmt::Display for Scheme {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.key())
    }
}

/// Where a database is: its protocol and its address in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Location {
    /// Protocol of the location.
    pub scheme: Scheme,
    /// Address in the protocol: absolute path for `file`, URL for `http` and
    /// `https`, empty for `memory`.
    pub address: String,
}

/// Why a location is refused, or could not be reached.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, thiserror::Error)]
#[ts(export)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum LocationError {
    /// Nothing typed.
    #[error("no location given")]
    Empty,
    /// Longer than the internal database keeps.
    #[error("location longer than {max} characters")]
    TooLong {
        /// Characters at most.
        max: u32,
    },
    /// A path that does not start at a root (`D:\`, `/`, `\\server\`).
    #[error("\"{address}\": the path must be absolute")]
    RelativePath {
        /// Path typed.
        address: String,
    },
    /// A URL that cannot be read.
    #[error("\"{address}\": URL not understood")]
    InvalidUrl {
        /// URL typed.
        address: String,
    },
    /// A web address with an account or a password: only public files are
    /// served.
    #[error("\"{address}\": a web location must be public, without an account")]
    Credentials {
        /// URL typed, without its password.
        address: String,
    },
    /// A protocol Prisme knows but does not serve yet.
    #[error("protocol {scheme}: not served by Prisme yet")]
    NotServed {
        /// Protocol typed.
        scheme: String,
    },
    /// A protocol Prisme does not know.
    #[error("protocol {scheme}: unknown")]
    UnknownScheme {
        /// Protocol typed.
        scheme: String,
    },
    /// The location exists already, where an empty database was to be
    /// created.
    #[error("\"{address}\" exists already")]
    AlreadyExists {
        /// Location.
        address: String,
    },
    /// A remote file larger than the user's preferences allow.
    #[error("file larger than {max_bytes} bytes")]
    TooLarge {
        /// Bytes at most.
        #[ts(type = "number")]
        max_bytes: u64,
    },
    /// The protocol does not create databases (a web server).
    #[error("protocol {scheme}: Prisme cannot create a database there")]
    NotCreatable {
        /// Protocol of the location.
        scheme: Scheme,
    },
    /// The location could not be reached: missing file, refused access,
    /// network...
    #[error("\"{address}\": {message}")]
    Unreachable {
        /// Location.
        address: String,
        /// Cause, as the system or the server gave it.
        message: String,
    },
}

impl Location {
    /// Location typed by the user: `:memory:`, a URL whose protocol is
    /// named (`file:///`, `https://`...), or an absolute path, with `\` or
    /// `/` (a drive letter is not a protocol). Spaces around are ignored.
    pub fn parse(input: &str) -> Result<Self, LocationError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(LocationError::Empty);
        }
        if input.chars().count() > ADDRESS_MAX_LENGTH {
            return Err(LocationError::TooLong {
                max: u32::try_from(ADDRESS_MAX_LENGTH).unwrap_or(u32::MAX),
            });
        }
        if input == MEMORY_INPUT {
            return Ok(Self {
                scheme: Scheme::Memory,
                address: String::new(),
            });
        }
        match scheme_of(input) {
            Some(scheme) => Self::parse_url(input, &scheme),
            None => Self::file(input),
        }
    }

    /// Location of a URL whose protocol is `scheme`, in lowercase.
    fn parse_url(input: &str, scheme: &str) -> Result<Self, LocationError> {
        let scheme = match scheme {
            "file" => Scheme::File,
            "http" => Scheme::Http,
            "https" => Scheme::Https,
            other if KNOWN_SCHEMES.contains(&other) => {
                return Err(LocationError::NotServed {
                    scheme: other.to_string(),
                });
            }
            other => {
                return Err(LocationError::UnknownScheme {
                    scheme: other.to_string(),
                });
            }
        };
        let invalid = || LocationError::InvalidUrl {
            address: input.to_string(),
        };
        let mut url = url::Url::parse(input).map_err(|_| invalid())?;
        if scheme == Scheme::File {
            let path = url.to_file_path().map_err(|()| invalid())?;
            return Self::file(&path.to_string_lossy());
        }
        if url.host_str().is_none_or(str::is_empty) {
            return Err(invalid());
        }
        if !url.username().is_empty() || url.password().is_some() {
            let _ = url.set_password(None);
            return Err(LocationError::Credentials {
                address: url.to_string(),
            });
        }
        Ok(Self {
            scheme,
            address: url.to_string(),
        })
    }

    /// Location of the file at the absolute path `path`, as typed.
    fn file(path: &str) -> Result<Self, LocationError> {
        if !Path::new(path).is_absolute() {
            return Err(LocationError::RelativePath {
                address: path.to_string(),
            });
        }
        Ok(Self {
            scheme: Scheme::File,
            address: path.to_string(),
        })
    }

    /// Text of the location, as the user types it again: the path, the
    /// URL, or `:memory:`.
    pub fn input(&self) -> &str {
        match self.scheme {
            Scheme::Memory => MEMORY_INPUT,
            Scheme::File | Scheme::Http | Scheme::Https => &self.address,
        }
    }
}

/// Protocol named at the start of `input`, in lowercase: the letters
/// before `:` (RFC 3986, section 3.1). One letter is a drive (`D:\data`),
/// not a protocol.
fn scheme_of(input: &str) -> Option<String> {
    let (scheme, _) = input.split_once(':')?;
    let mut characters = scheme.chars();
    let first = characters.next()?;
    let valid = first.is_ascii_alphabetic()
        && characters.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    (valid && scheme.len() > 1).then(|| scheme.to_ascii_lowercase())
}

/// What a provider can do with its locations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ProviderCapabilities {
    /// Can an engine write to a database there?
    pub writable: bool,
    /// Can Prisme create an empty database there?
    pub creatable: bool,
    /// Is the database read where it is, without a copy in the cache?
    pub local: bool,
}

/// State of a location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct LocationReport {
    /// Is there a database (or at least a file) there?
    pub exists: bool,
    /// Size of the file, in bytes, when known.
    pub size: Option<u64>,
    /// Date of its last change, in UTC, in ISO 8601 format, when known.
    pub modified_at: Option<String>,
    /// Tag of its version on a web server (`ETag`), when it gives one.
    pub version_tag: Option<String>,
    /// Can it only be read: a file the process cannot write, a remote file.
    pub read_only: bool,
}

/// Form of a database an engine opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Local {
    /// A file on the disk of Prisme.
    File {
        /// Path of the file.
        path: PathBuf,
        /// Only to be read: a copy of a remote file.
        read_only: bool,
    },
    /// A database in memory.
    Memory,
}

/// Limits of the reading of a remote location, set by the preferences of
/// the user (`docs/TODO-SQLITE.md`, decision 15).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteLimits {
    /// Longest wait for the connection to the server.
    pub connect_timeout: Duration,
    /// Longest wait for a whole download.
    pub download_timeout: Duration,
    /// Largest file downloaded, in bytes.
    pub max_bytes: u64,
}

impl Default for RemoteLimits {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            download_timeout: Duration::from_secs(5 * 60),
            max_bytes: 1 << 30,
        }
    }
}

/// Where and how the remote locations of a user are read: the folder of
/// their copies in the cache, and the limits of a download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote {
    /// Folder of the copies (`cache/locations/<user>` in the data folder).
    pub cache_dir: PathBuf,
    /// Limits of a download.
    pub limits: RemoteLimits,
}

/// Provider of the locations of a protocol.
pub trait LocationProvider: Send + Sync {
    /// What the provider can do.
    fn capabilities(&self) -> ProviderCapabilities;

    /// State of the location `location`, reached within the limits of
    /// `remote`; a location that does not exist is a report, not an error.
    fn probe<'a>(
        &'a self,
        location: &'a Location,
        remote: &'a Remote,
    ) -> BoxFuture<'a, Result<LocationReport, LocationError>>;

    /// Creates an empty database at `location`, which must not exist yet.
    fn create<'a>(&'a self, location: &'a Location) -> BoxFuture<'a, Result<(), LocationError>>;

    /// Local form of the database at `location`, for an engine that does not
    /// read the protocol natively: a remote file is copied into the cache of
    /// `remote` first, or found there unchanged.
    fn materialize<'a>(
        &'a self,
        location: &'a Location,
        remote: &'a Remote,
    ) -> BoxFuture<'a, Result<Local, LocationError>>;
}

/// Registry of the providers, by protocol.
pub mod locations {
    use super::{LocationProvider, Scheme, file, http, memory};

    /// Provider of the protocol `scheme`.
    pub fn of(scheme: Scheme) -> &'static dyn LocationProvider {
        match scheme {
            Scheme::File => &file::PROVIDER,
            Scheme::Http | Scheme::Https => &http::PROVIDER,
            Scheme::Memory => &memory::PROVIDER,
        }
    }
}

/// Date `time` in UTC, in ISO 8601 format with milliseconds, as the internal
/// database writes its dates (`2026-11-22T14:54:42.810Z`).
fn iso_8601(time: SystemTime) -> String {
    let since = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let seconds = since.as_secs();
    let (year, month, day) = civil_from_days(seconds / 86_400);
    let rest = seconds % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60,
        since.subsec_millis()
    )
}

/// Date of the day `days` after 1970-01-01: year, month, day (H. Hinnant,
/// "chrono-Compatible Low-Level Date Algorithms", `civil_from_days`).
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let day_of_era = z % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(input: &str) -> Location {
        Location::parse(input).unwrap()
    }

    #[test]
    fn reads_memory_and_web_locations() {
        assert_eq!(parsed(" :memory: ").scheme, Scheme::Memory);
        assert_eq!(parsed(":memory:").input(), ":memory:");
        let web = parsed("HTTPS://data.example.org/files/v.db?raw=1");
        assert_eq!(
            (web.scheme, web.address.as_str()),
            (Scheme::Https, "https://data.example.org/files/v.db?raw=1")
        );
        assert_eq!(parsed("http://example.org/v.db").scheme, Scheme::Http);
    }

    #[cfg(windows)]
    #[test]
    fn reads_the_paths_of_windows_with_both_separators() {
        for path in [r"D:\data\v.db", "D:/data/v.db", r"\\server\share\v.db"] {
            assert_eq!(
                parsed(path),
                Location {
                    scheme: Scheme::File,
                    address: path.to_string()
                }
            );
        }
        assert_eq!(parsed("file:///D:/data/v.db").address, r"D:\data\v.db");
        for relative in ["data/v.db", r"data\v.db", "/srv/v.db", "v.db"] {
            assert!(
                matches!(
                    Location::parse(relative),
                    Err(LocationError::RelativePath { .. })
                ),
                "{relative}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn reads_the_paths_of_unix() {
        assert_eq!(parsed("/srv/v.db").address, "/srv/v.db");
        assert_eq!(parsed("file:///srv/v.db").address, "/srv/v.db");
        assert!(matches!(
            Location::parse("srv/v.db"),
            Err(LocationError::RelativePath { .. })
        ));
    }

    #[test]
    fn refuses_the_protocols_not_served_and_says_which() {
        assert_eq!(
            Location::parse("s3://bucket/v.db"),
            Err(LocationError::NotServed {
                scheme: "s3".to_string()
            })
        );
        assert_eq!(
            Location::parse("FTP://host/v.db"),
            Err(LocationError::NotServed {
                scheme: "ftp".to_string()
            })
        );
        assert_eq!(
            Location::parse("gopher://host/v.db"),
            Err(LocationError::UnknownScheme {
                scheme: "gopher".to_string()
            })
        );
    }

    #[test]
    fn refuses_empty_long_and_private_web_locations() {
        assert_eq!(Location::parse("  "), Err(LocationError::Empty));
        let long = format!("https://example.org/{}", "a".repeat(ADDRESS_MAX_LENGTH));
        assert!(matches!(
            Location::parse(&long),
            Err(LocationError::TooLong { .. })
        ));
        assert_eq!(
            Location::parse("https://ada:secret@example.org/v.db"),
            Err(LocationError::Credentials {
                address: "https://ada@example.org/v.db".to_string()
            })
        );
        assert!(matches!(
            Location::parse("https://"),
            Err(LocationError::InvalidUrl { .. })
        ));
    }

    #[test]
    fn writes_dates_as_the_internal_database() {
        let at = |seconds: u64, millis: u64| {
            iso_8601(UNIX_EPOCH + Duration::from_millis(seconds * 1_000 + millis))
        };
        assert_eq!(at(0, 0), "1970-01-01T00:00:00.000Z");
        assert_eq!(at(951_782_400, 5), "2000-02-29T00:00:00.005Z");
        assert_eq!(at(1_795_359_282, 810), "2026-11-22T14:54:42.810Z");
    }

    #[test]
    fn keys_of_the_protocols() {
        for scheme in Scheme::ALL {
            assert_eq!(Scheme::from_key(scheme.key()), Some(scheme));
        }
    }
}
