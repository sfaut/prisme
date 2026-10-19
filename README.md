# Prisme

Prisme is a multi-database web client shaped like a SQL notebook: several sources, one view. It runs on your computer as a single executable and opens in your browser.

Prisme is at the start of its first version (v1). Today it queries **MySQL and MariaDB**, in **read-only** mode. PostgreSQL, SQLite, DuckDB and translation between SQL dialects come next.

> The interface is currently in French. This guide quotes its labels as they appear on screen, with a translation, for example **Exécuter** (Run).

## Quick start

1. **Download** the executable for your system from the [latest release](https://github.com/sfaut/prisme/releases/latest): Windows (`.exe`), Linux, or macOS (Apple Silicon and Intel). Nothing else to install.
2. **Run it** from a terminal. On Linux and macOS, first make it executable (see [Installation](#installation)).
3. **Answer the two setup questions**: listening port and your email address. Press Enter to keep the defaults (`8484`, `me@local`).
4. **Open** <http://127.0.0.1:8484> in your browser.
5. **Add a connection**: in the left panel, open the **⋮** menu of **Sources de données** (Data sources), choose **Ajouter une connexion** (Add a connection), fill in host, port, user and password, click **Tester** (Test), then save.
6. **Query**: double-click the new source, type a query in the editor, press **Ctrl+Enter**.

## Installation

Each release provides three files:

| System | File |
|---|---|
| Windows (x86_64) | `prisme-vX.Y.Z-windows-x86_64.exe` |
| Linux (x86_64) | `prisme-vX.Y.Z-linux-x86_64` |
| macOS (Apple Silicon and Intel) | `prisme-vX.Y.Z-macos-universal` |

The executable contains the whole application, interface included: no runtime, database or service to install.

The executables are not signed yet, so the system warns on first launch:

- **Windows**: SmartScreen shows a warning. Click **More info**, then **Run anyway**.
- **Linux**: make the file executable: `chmod +x prisme-vX.Y.Z-linux-x86_64`.
- **macOS**: make the file executable and lift the download quarantine, once:

  ```
  chmod +x prisme-vX.Y.Z-macos-universal
  xattr -d com.apple.quarantine prisme-vX.Y.Z-macos-universal
  ./prisme-vX.Y.Z-macos-universal
  ```

## Starting Prisme

### First launch

On first launch, Prisme asks two questions in the terminal:

```
Prisme setup. Press Enter to keep the value in brackets.
Listening port [8484]:
Email address [me@local]:
```

- **Listening port**: from 1 to 65535. A port already in use is rejected and asked again. Prisme only listens on `127.0.0.1`, so no other computer can reach it.
- **Email address**: identifies you in Prisme. A malformed address is asked again.

The answers are saved, and later launches start without asking. To change them, run `prisme --setup`. Without an interactive terminal (script, service), the defaults apply.

### Command-line options

`prisme --help` lists them.

| Option | Use |
|---|---|
| `--listen <address>` | Listening address for this launch only, for example `127.0.0.1:9000`. It must be local. |
| `--data-dir <folder>` | Data folder (see [Your data](#your-data)). |
| `--user-email <email>` | User for this launch only, created if needed. |
| `--setup` | Asks the first-launch questions again and saves the answers. |
| `-v`, `--version` | Prints the version. Prisme also writes it on the first line of its log at each launch. |

Options given on the command line replace the saved settings for that launch, without changing them.

## Using Prisme

The screen has three areas:

- **Left panel**: the object explorer, with your connections and their databases.
- **Editor tabs**: several SQL editors, each one on its own connection.
- **SQL cell and results**: the editor, its buttons, and one result tab per query.

The version of Prisme shows under its name, at the top left.

### Connections

Connections are managed from the explorer:

- **Add**: **⋮** menu of **Sources de données** (Data sources) → **Ajouter une connexion** (Add a connection). **Tester** (Test) tries the settings before saving.
- **Edit, refresh, test, delete**: **⋮** menu of each source. A blank password when editing keeps the current one. Deleting asks for confirmation.
- **Session settings**: the **À la connexion** (On connect) field runs queries each time a connection opens, separated by `;`, for example `SET NAMES utf8mb4; SET SESSION sql_mode = ...`. Only session settings are allowed.
- **System databases** (`information_schema`, `mysql`, `performance_schema`, `sys`) can be hidden per source.

Passwords are encrypted on disk and never sent back to the browser.

### Object explorer

The tree shows, for each source:

- its **databases**, then their **tables**, **views**, **procedures** and **functions**;
- for each table, its **columns**, **indexes**, **foreign keys** and **triggers**.

How it behaves:

- A **click** on a row opens or closes it.
- A **double-click** on a source makes it the connection of the active editor; on a database, makes it the current database (like `USE`); on a table or view, shows its content: its `SELECT` goes after the last query of the editor, without overwriting anything, and runs.
- The catalog loads progressively, and the selected connection opens by itself. **Tout rafraîchir** (Refresh all) in the root menu reloads the catalogs already read.
- The current database is in **bold**.

Columns show, on three aligned columns: a symbol, the name, then the type, nullability and `AUTO_INCREMENT`.

- A **key** marks the primary key, a **hash** marks foreign keys.
- Indexed columns are in **bold**. Indexes show `PRIMARY KEY` or `UNIQUE`.
- Long labels are shortened (…); hover to read them in full.

Drag the right edge of the panel to resize it; double-click the edge to restore its default width.

### Editors

The tab bar above the SQL cell holds several editors:

- **+** opens an editor on the connection of the active one.
- **Double-click** a tab to rename it. Drag it to move it.
- The **cross** (or a middle click) closes it. A closed editor can be reopened for 24 hours from the **Rouvrir** (Reopen) button, the clock icon.

Each editor has its own connection, its own session (and therefore its own transactions) and its own results. Everything is saved as you type: texts, editors, result tabs and rows. A page reload or a browser restart brings everything back.

### Running queries

| Action | Key | Button |
|---|---|---|
| Run the query under the cursor | Ctrl+Enter | **Exécuter** (Run) |
| Run every query of the editor, in parallel | Ctrl+Shift+Enter | **Tout exécuter** (Run all) |
| Run the queries touched by the selected text, in parallel | Ctrl+Shift+Enter, with text selected | |
| Stop every running query | | **Stopper** (Stop) |

- When the editor holds several queries, the one Ctrl+Enter would run is **highlighted**.
- Syntax errors are **underlined in red** as you type, with the message on hover.
- **Row limit**: when a query has no `LIMIT`, Prisme shows at most 1,000 rows (adjustable up to 50,000 with the list next to the buttons) and reports a truncated result. A query that has its own `LIMIT` runs as written.
- **Read-only**: only reads are allowed for now (`SELECT`, `SHOW`, `DESCRIBE`, `EXPLAIN`, `USE`). Writes are refused.

### Writing SQL

- **Completion** (Ctrl+Space, or while typing): databases, tables, views, functions and procedures of the current database, and columns after `table.` or `alias.`. Only names are suggested, never keywords.
- **Tab** moves to the next tab stop (every 4 spaces), or indents the selected lines. **Shift+Tab** unindents them.
- **Enter** starts a new line at the indentation of the current line.
- **Ctrl+Up** / **Ctrl+Down** scroll the editor by one line, without moving the cursor.
- To leave the editor with the keyboard: **Esc** then **Tab**, or **Ctrl+M** (Tab then moves the focus again).
- **Enjoliver** (Beautify) rewrites the query in a consistent style: keywords and function names in capitals, table and column names as written in the catalog, `AS` before aliases, one clause per line, spaces around operators. Stored procedures and `DELIMITER` blocks are supported. Ctrl+Z undoes it. The button is active once the connection's catalog is loaded.

Drag the bar under the editor to share the height between editor and results; double-click it to restore the default.

### Result tabs

Each query gets its own tab:

- A **dot** shows its progress: grey while waiting its turn, blue and pulsing while running and receiving rows, green when done, red on failure.
- A **progress bar** on its bottom edge fills while rows arrive.
- **Double-click** the label to rename the tab. The **pin** keeps the tab when queries are run again. The **cross** (or a middle click) closes it and stops its query. Drag a tab to move it.
- The bar under the grid shows the number of rows, the execution time, the transfer time, the data received and the throughput.

### Result grid

| Action | How |
|---|---|
| Sort | ⇅ button of a column header: ascending, descending, none |
| Filter | Filter button, left of the bar under the grid, on all columns or on one |
| Move a column | Drag its name |
| Resize a column | Drag the right edge of its header; double-click the edge to fit the longest value |
| Select | Click a cell, a row number or a column name; drag for a range; click the top-left corner for everything |
| Extend / add to the selection | Shift+click / Ctrl+click |
| Select all / clear | Ctrl+A / Esc |
| Copy | Ctrl+C |

- **Copy** is a raw copy: values as they are, separated by tabs, with column names on the first line, ready to paste into a spreadsheet. `NULL` is copied as an empty cell. Control characters are written literally (`\t`, `\n`, `\r`, `\0`, `\b`, `\f`…, or `\xHH`), so each value stays in one cell.
- **Display**: the header shows each column's type (hover for the full name). Numbers and dates are right-aligned, `CHAR` columns use a monospaced font, web addresses are clickable, `NULL` is shown in small grey capitals.

### Theme

The **Système / Clair / Sombre** (System / Light / Dark) list at the top of the left panel sets the theme of the whole interface. **Système** follows your computer's setting.

## Your data

Prisme keeps its data in a folder of your user account:

| System | Default folder |
|---|---|
| Windows | `%APPDATA%\Prisme` |
| Linux | `~/.local/share/Prisme` |

It holds two files that go together:

- **`prisme.db`**: the internal database (settings, connections, editors, results).
- **`prisme.key`**: the master key, created on first launch. It encrypts the passwords of your connections.

- **Back up** both files. Without the key, saved passwords cannot be read and must be typed again; everything else is kept.
- **Protect** the folder: whoever has both files can read the saved passwords. By default, only your user account can read it.
- **Never edit `prisme.key`**: Prisme refuses to start with an invalid key, and never overwrites it.

## Developers

Building from source, tests, internal API, continuous integration and releases: see [docs/DEVELOPER.md](docs/DEVELOPER.md).

## License

MIT, see [LICENSE](LICENSE).
