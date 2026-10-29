//! Exports folder: the names of its files, their reading against the table
//! of the exports, and the name offered on download.
//!
//! Prisme names its files `<id>-<instant in ms>.csv`. Only the files with a
//! name of this form are its own: the other files of the folder are never
//! listed nor deleted.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::Path;

/// Extension of the files.
const EXTENSION: &str = ".csv";

/// Name of the file of export `id`, created at instant `millis` (ms since
/// the Unix epoch): two exports never share a file.
pub fn file_name(id: i64, millis: u128) -> String {
    format!("{id}-{millis}{EXTENSION}")
}

/// Is `name` the name of a file of Prisme?
fn is_export_file(name: &str) -> bool {
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
    name.strip_suffix(EXTENSION)
        .and_then(|stem| stem.split_once('-'))
        .is_some_and(|(id, instant)| digits(id) && digits(instant))
}

/// Files of Prisme in `folder`, with their size in bytes. A folder not
/// created yet holds none.
pub fn scan(folder: &Path) -> io::Result<HashMap<String, u64>> {
    let entries = match std::fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(error) => return Err(error),
    };
    let mut files = HashMap::new();
    for entry in entries {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if let Some(name) = entry.file_name().to_str()
            && metadata.is_file()
            && is_export_file(name)
        {
            files.insert(name.to_string(), metadata.len());
        }
    }
    Ok(files)
}

/// Deletes file `file` of `folder`. A file already gone is not an error.
pub fn remove(folder: &Path, file: &str) -> io::Result<()> {
    match std::fs::remove_file(folder.join(file)) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// Deletes the files of Prisme of `folder` that are not in `referenced`.
/// Goes on after a failure, and returns the first one.
pub fn remove_orphans(folder: &Path, referenced: &HashSet<String>) -> io::Result<()> {
    let mut failure = None;
    for file in scan(folder)?.into_keys() {
        if !referenced.contains(&file)
            && let Err(error) = remove(folder, &file)
        {
            failure.get_or_insert(error);
        }
    }
    failure.map_or(Ok(()), Err)
}

/// Value of the `Content-Disposition` header that offers to save the file as
/// `name` (RFC 6266): the name in UTF-8, percent-encoded, and an ASCII
/// version for the clients that do not read it.
pub fn content_disposition(name: &str) -> String {
    let ascii: String = name
        .chars()
        .map(|character| match character {
            ' '..='~' if character != '"' && character != '\\' => character,
            _ => '_',
        })
        .collect();
    let mut encoded = String::with_capacity(name.len());
    for byte in name.bytes() {
        if byte.is_ascii_alphanumeric() || b"!#$&+-.^_`|~".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{encoded}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_its_own_files() {
        assert!(is_export_file(&file_name(12, 1_792_500_000_000)));
        for name in [
            "notes.txt",
            "12.csv",
            "-1.csv",
            "a-1.csv",
            "12-1.csv.tmp",
            "12-.csv",
        ] {
            assert!(!is_export_file(name), "{name}");
        }
    }

    #[test]
    fn reads_and_cleans_the_folder() {
        let folder = tempfile::tempdir().unwrap();
        assert!(scan(&folder.path().join("absent")).unwrap().is_empty());
        for (name, content) in [("1-10.csv", "a"), ("2-20.csv", "bc"), ("notes.txt", "d")] {
            std::fs::write(folder.path().join(name), content).unwrap();
        }
        let files = scan(folder.path()).unwrap();
        assert_eq!(
            files,
            HashMap::from([("1-10.csv".into(), 1), ("2-20.csv".into(), 2)])
        );

        remove_orphans(folder.path(), &HashSet::from(["1-10.csv".to_string()])).unwrap();
        assert!(folder.path().join("1-10.csv").exists());
        assert!(!folder.path().join("2-20.csv").exists());
        assert!(folder.path().join("notes.txt").exists());

        remove(folder.path(), "1-10.csv").unwrap();
        remove(folder.path(), "1-10.csv").unwrap();
        assert!(scan(folder.path()).unwrap().is_empty());
    }

    #[test]
    fn offers_the_name_in_utf_8() {
        assert_eq!(
            content_disposition("requête 1 \"a\".csv"),
            "attachment; filename=\"requ_te 1 _a_.csv\"; \
             filename*=UTF-8''requ%C3%AAte%201%20%22a%22.csv"
        );
    }
}
