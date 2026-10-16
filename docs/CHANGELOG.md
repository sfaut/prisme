# Changelog

Changelogs of the published versions of Prisme, most recent first. Each entry is also the notes of its GitHub release. The interface is in French: its labels are quoted as they appear, with a translation.

## v0.2.0 (2026-10-16)

### 💥 Breaking changes

- The short option that prints the version becomes `-v`; `-V` is no longer recognised, `--version` is unchanged ([#7](https://github.com/sfaut/prisme/issues/7)).

### ✨ New features

- First-launch setup: Prisme asks in the terminal for the listening port (8484 by default) and the user's email address (`me@local` by default), saves them in its internal database, and does not ask again. `prisme --setup` asks the questions again; `--listen` and `--user-email` only apply to one launch. An existing installation keeps its settings, without any question ([#6](https://github.com/sfaut/prisme/issues/6)).
- SQL editor: Tab moves to the next tab stop (every four spaces) or indents the selected lines, Shift+Tab unindents them; Esc then Tab, or Ctrl+M, leave the editor with the keyboard ([#2](https://github.com/sfaut/prisme/issues/2)).
- Result grid: `CHAR` columns are displayed in a monospaced font ([#4](https://github.com/sfaut/prisme/issues/4)), and the `NULL` marker in small capitals, more discreet ([#1](https://github.com/sfaut/prisme/issues/1)).

### 🐞 Bug fixes

- SQL editor: Enter no longer adds two spaces at the start of the line; the cursor goes to the first column ([#5](https://github.com/sfaut/prisme/issues/5)).
- Grid copy (Ctrl+C): values are copied as they are, without added quotes; control characters in a value are written literally (`\t`, `\n`, `\r`…), to keep one value per cell ([#3](https://github.com/sfaut/prisme/issues/3)).

### ⚙️ Internal improvements

- Up-to-date dependencies: Rust 1.99.1, Tokio 1.53.3, Vitest 5, Oxlint 1.88, Prettier 3.9.10, and the GitHub actions of the continuous integration.

### 📝 Documentation

- README in English, for the end user, with a quick start; separate developer guide, in `docs/DEVELOPER.md`.
- MIT license: copyright holder specified.

## v0.1.0 (2026-10-15)

### 🚀 Launch

First public version of Prisme, a multi-database web client shaped like a SQL notebook. This v0.1.0 queries MySQL and MariaDB; PostgreSQL, SQLite, DuckDB and translation between dialects will come next.

- A single executable per platform, interface included: Windows (x86_64), Linux (x86_64) and macOS (universal, Apple Silicon and Intel). Nothing else to install.
- Prisme runs locally and opens in the browser, at <http://127.0.0.1:8484>.
- The executables are not signed yet: Windows shows a SmartScreen warning on first launch, and macOS asks to lift the file's quarantine (procedure in the README).

### 🔒 Security

- Connection passwords encrypted in the internal database (AES-256-GCM), never written to the logs nor sent back by the API.
- Listening limited to the local machine: without authentication, Prisme refuses to be reachable from the network.
- Read-only: query sessions refuse writes (`UPDATE`, `DELETE`, DDL, `CALL`…), detected by analysing the query, until the write switches exist.

### ✨ New features

**Connections**
- Adding, editing, testing and deleting MySQL / MariaDB connections, with detection of the server flavor and version, shown as a badge in the engine's colors.
- Queries run when each connection opens (`SET NAMES`, `sql_mode`…).
- Clear messages when a connection fails.

**Object explorer**
- Sources, databases, tables, views, procedures, functions; columns, indexes, foreign keys and triggers of each table.
- Catalog displayed progressively, columns aligned with their type, primary key, foreign keys, indexed columns, `UNIQUE` and `AUTO_INCREMENT` marked.
- Double-click: choose a source, change the current database, show the content of a table.
- System databases can be hidden, resizable panel.

**SQL editor**
- Several editors in tabs, each with its connection and its session, saved with their results and reopenable for 24 hours after closing.
- Ctrl+Enter runs the query under the cursor, highlighted in the editor; Ctrl+Shift+Enter runs all the queries in parallel, one per result tab.
- Completion of databases, tables and columns, syntax errors underlined before running.
- **Enjoliver** (Beautify) button: keywords and functions in capitals, names as written in the catalog, `AS` before aliases, layout by clauses, invisible whitespace cleaned up, stored procedures and `DELIMITER` supported.
- **Stopper** (Stop) button to stop the running queries.

**Results**
- Streaming reads, with a default limit of 1,000 rows (adjustable up to 50,000) when the query specifies none, and a "truncated result" notice.
- Result tabs that can be pinned, renamed and moved, with a progress dot, execution and transfer times, data received and throughput.
- Compact grid: sorting, filtering, movable and resizable columns, type of each column in the header, numbers and dates right-aligned, clickable web links, JSON and XML recognised.
- Selection with the mouse or the keyboard, copy with Ctrl+C with the column names, ready to paste into a spreadsheet.

**Interface**
- Light, dark or system theme, editor included.
- Split bar between the editor and the results.

### 📝 Documentation

- README: installation, launch, use of the interface and first launch of the unsigned executables.
