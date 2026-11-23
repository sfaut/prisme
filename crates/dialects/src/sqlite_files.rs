//! Files named by SQLite: the URI that opens a file in the default file
//! system of the platform (VFS `win32` on Windows, `unix` elsewhere), which
//! the connector of SQLite opens every file through.

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

#[cfg(test)]
mod tests {
    use super::*;

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
