//! Files named by SQLite: the URI that opens a file in the default file
//! system of the platform (VFS `win32` on Windows, `unix` elsewhere), which
//! the connector of SQLite opens every file through, and the names an
//! `ATTACH` written in SQL may give.
//!
//! An `ATTACH` names a literal absolute path, `':memory:'`, or the URI of a
//! file in the default file system and nothing else: naming the file system
//! makes the file open on the disk whatever the file system of the main
//! database. Any other URI is refused: it could name a database in memory
//! of another session (`vfs=memdb`) or change the mode of the opening
//! (`mode=rwc`, `cache=shared`).

use std::path::Path;

/// Default file system of SQLite on this platform, named by the URI of
/// every file.
pub const FILE_VFS: &str = if cfg!(windows) { "win32" } else { "unix" };

/// URI of the file at the absolute path `path`: `file:///D:/data/v.db`,
/// `file:///srv/v.db`, `file:////server/share/v.db` for a path of a server
/// of Windows (an empty authority, which SQLite requires), in the default
/// file system, read-only with `read_only`. `%`, `?`, `#` and the spaces
/// are encoded, the separators of Windows written `/`.
pub fn file_uri(path: &str, read_only: bool) -> String {
    let path = path.replace('\\', "/");
    let mut uri = String::from(if path.starts_with('/') {
        "file://"
    } else {
        "file:///"
    });
    for character in path.chars() {
        match character {
            '%' => uri.push_str("%25"),
            '?' => uri.push_str("%3F"),
            '#' => uri.push_str("%23"),
            ' ' => uri.push_str("%20"),
            other => uri.push(other),
        }
    }
    uri.push_str("?vfs=");
    uri.push_str(FILE_VFS);
    if read_only {
        uri.push_str("&mode=ro");
    }
    uri
}

/// File that an `ATTACH` of SQLite names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachedFile {
    /// `':memory:'`: a database in memory of its connection alone.
    Memory,
    /// Absolute path of a file, as written.
    Path(String),
    /// URI of a file in the default file system (`FILE_VFS`), with the
    /// absolute path it names, decoded.
    Uri(String),
}

/// Reads the name `name` that an `ATTACH` gives its file: `':memory:'`, an
/// absolute path (with `\` or `/` on Windows), or the URI of a file in the
/// default file system, its only parameter `vfs` (`file:///D:/data/v.db?vfs=win32`,
/// `file:/D:/data/v.db?vfs=win32`, `file:///srv/v.db?vfs=unix`); `None` for
/// any other name: a relative path, read from the folder where Prisme runs,
/// or another URI.
pub fn attached_file(name: &str) -> Option<AttachedFile> {
    if name == ":memory:" {
        return Some(AttachedFile::Memory);
    }
    match name.strip_prefix("file:") {
        Some(uri) => uri_path(uri).map(AttachedFile::Uri),
        None => Path::new(name)
            .is_absolute()
            .then(|| AttachedFile::Path(name.to_string())),
    }
}

/// Absolute path that the URI `uri`, without its `file:`, names in the
/// default file system; `None` if it names another file system, takes
/// another parameter or a fragment, or names a relative path.
fn uri_path(uri: &str) -> Option<String> {
    // A fragment would end the URI before its parameters.
    if uri.contains('#') {
        return None;
    }
    let (path, query) = uri.split_once('?')?;
    if query != format!("vfs={FILE_VFS}") {
        return None;
    }
    // An authority, empty or `localhost`, as SQLite reads it.
    let path = match path.strip_prefix("//") {
        Some(rest) => {
            let start = rest.find('/')?;
            if !matches!(&rest[..start], "" | "localhost") {
                return None;
            }
            &rest[start..]
        }
        None => path,
    };
    let path = decoded(path)?;
    // `/D:/data/v.db` names `D:/data/v.db` on Windows.
    let path = match path.strip_prefix('/') {
        Some(rest) if cfg!(windows) && has_drive(rest) => rest.to_string(),
        _ => path,
    };
    Path::new(&path).is_absolute().then_some(path)
}

/// Does `path` start with the letter of a drive of Windows (`D:`)?
fn has_drive(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

/// Text of the path `path` of a URI, its `%HH` decoded; `None` if an
/// escape is incomplete or the text is not UTF-8.
fn decoded(path: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(path.len());
    let mut rest = path.as_bytes();
    while let Some((&byte, tail)) = rest.split_first() {
        if byte == b'%' {
            let hex = std::str::from_utf8(tail.get(..2)?).ok()?;
            bytes.push(u8::from_str_radix(hex, 16).ok()?);
            rest = &tail[2..];
        } else {
            bytes.push(byte);
            rest = tail;
        }
    }
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_files_an_attach_names() {
        assert_eq!(attached_file(":memory:"), Some(AttachedFile::Memory));
        let accepted: &[(&str, &str)] = if cfg!(windows) {
            &[
                (r"D:\data\v.db", ""),
                (
                    "file:///D:/data/a%20b%231%25.db?vfs=win32",
                    "D:/data/a b#1%.db",
                ),
                ("file:/D:/data/v.db?vfs=win32", "D:/data/v.db"),
                ("file:D:/data/v.db?vfs=win32", "D:/data/v.db"),
                ("file://localhost/D:/data/v.db?vfs=win32", "D:/data/v.db"),
                (
                    "file:////server/share/v.db?vfs=win32",
                    "//server/share/v.db",
                ),
            ]
        } else {
            &[
                ("/srv/v.db", ""),
                ("file:///srv/a%20b.db?vfs=unix", "/srv/a b.db"),
                ("file:/srv/v.db?vfs=unix", "/srv/v.db"),
                ("file://localhost/srv/v.db?vfs=unix", "/srv/v.db"),
            ]
        };
        for (name, path) in accepted {
            let expected = if path.is_empty() {
                AttachedFile::Path((*name).to_string())
            } else {
                AttachedFile::Uri((*path).to_string())
            };
            assert_eq!(attached_file(name), Some(expected), "name: {name}");
        }
        // The URI of every file the connector opens is accepted.
        let path = if cfg!(windows) {
            r"D:\data\a b.db"
        } else {
            "/srv/a b.db"
        };
        assert!(matches!(
            attached_file(&file_uri(path, false)),
            Some(AttachedFile::Uri(_))
        ));
        let other_vfs = if cfg!(windows) { "unix" } else { "win32" };
        let refused = [
            "v.db".to_string(),
            String::new(),
            "file:v.db?vfs=win32".to_string(),
            "file:v.db?vfs=unix".to_string(),
            "file:///D:/data/v.db".to_string(),
            "file:///srv/v.db".to_string(),
            "file:/prisme/session-1/main?vfs=memdb".to_string(),
            format!("file:///srv/v.db?vfs={other_vfs}"),
            format!("file:///srv/v.db?vfs={FILE_VFS}&mode=rwc"),
            format!("file:///srv/v.db?mode=ro&vfs={FILE_VFS}"),
            format!("file:///srv/v.db?vfs={FILE_VFS}#part"),
            format!("file:///srv/v#part.db?vfs={FILE_VFS}"),
            format!("file://server/srv/v.db?vfs={FILE_VFS}"),
            format!("file:///srv/v%2.db?vfs={FILE_VFS}"),
        ];
        for name in &refused {
            assert_eq!(attached_file(name), None, "name: {name}");
        }
    }

    #[test]
    fn names_a_file_in_the_default_file_system() {
        if cfg!(windows) {
            assert_eq!(
                file_uri(r"D:\data\a b#1%.db", false),
                "file:///D:/data/a%20b%231%25.db?vfs=win32"
            );
            assert_eq!(
                file_uri(r"\\server\share\v.db", true),
                "file:////server/share/v.db?vfs=win32&mode=ro"
            );
        } else {
            assert_eq!(
                file_uri("/data/a b#1%.db", false),
                "file:///data/a%20b%231%25.db?vfs=unix"
            );
            assert_eq!(file_uri("/v.db", true), "file:///v.db?vfs=unix&mode=ro");
        }
    }
}
