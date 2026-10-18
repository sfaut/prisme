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
- `mysql/<subject>.md`: excerpts on a subject: source code of MySQL, with the
  tag and the line of each version, tickets of bugs.mysql.com, pages of the
  manual.
- `mariadb/<version>.md`: release notes of a MariaDB version, from 10.6.0, and
  `mariadb/<series>.md` (`mariadb/10.6.md`): changes and improvements of a
  series. The entries about SQL, as for MySQL, extracted by
  `tools/mariadb-relnotes.py` from the Markdown form of the page (its URL
  followed by `.md`), which leaves out the sections about storage engines,
  replication, Galera, backups, packaging, platforms, plugins, clients and
  security, and the entries about internal failures:

  ```
  python -I docs/sources/tools/mariadb-relnotes.py <page.md> <url> docs/sources/mariadb/<version>.md
  ```

  The list of the pages is in the index of the site,
  `https://mariadb.com/docs/llms.txt`.
- `mariadb/MDEV-<n>.md`: ticket of MariaDB, its fix versions and what it was
  consulted for.
- `mariadb/<subject>.md`: excerpts on a subject: tickets of MariaDB, pages of
  the documentation.
