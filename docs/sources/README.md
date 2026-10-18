# Sources

Excerpts of the documents consulted for the dialects: release notes, tickets,
source code of the servers. They are kept here to be read again without
downloading them, and are cited by the reference files
(`crates/dialects/reference/*.yaml`) and by `docs/DEV-DIALECTS.md`.

Each file keeps only what matters for the dialects of Prisme, in Markdown, with
the URL of the document it comes from. A page is never kept raw.

- `mysql/<version>.md`: release notes of a MySQL version, the entries about SQL
  (syntax, optimizer, functions, types, character sets, JSON, deprecations,
  bugs fixed that change a result or an error). Extracted by
  `tools/relnotes.py`, which leaves out the entries about storage engines,
  replication, packaging, plugins and internal failures (assertions, crashes):

  ```
  python -I docs/sources/tools/relnotes.py <page.html> <url> docs/sources/mysql/<version>.md
  ```

  The page is downloaded into a temporary folder, outside the repository.
- `mysql/<subject>.md`: excerpt of the source code of MySQL, with the tag and
  the line of each version.
- `mariadb/MDEV-<n>.md`: ticket of MariaDB, its fix versions and what it was
  consulted for.
