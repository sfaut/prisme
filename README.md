# Prisme

[![CI](https://img.shields.io/github/actions/workflow/status/sfaut/prisme/ci.yaml?event=release&label=CI)](https://github.com/sfaut/prisme/actions/workflows/ci.yaml)
[![Release](https://img.shields.io/github/v/release/sfaut/prisme)](https://github.com/sfaut/prisme/releases/latest)
[![License](https://img.shields.io/github/license/sfaut/prisme)](LICENSE)
![Platforms](https://img.shields.io/badge/platforms-Windows%20%7C%20Linux%20%7C%20macOS-informational)
![Databases](https://img.shields.io/badge/databases-MySQL%20%7C%20MariaDB-informational)

Prisme is a multi-database web client shaped like a SQL notebook: several sources, one view. It runs on your computer as a single executable and opens in your browser.

Prisme is at the start of its first version (v1). Today it queries **MySQL and MariaDB**, in **read-only** mode. PostgreSQL, SQLite, DuckDB and translation between SQL dialects come next.

> The interface is currently in French. This guide quotes its labels as they appear on screen, with a translation, for example **Exécuter** (Run).

## Contents

- [Quick start](#quick-start)
- [Installation](#installation)
- [Starting Prisme](#starting-prisme)
  - [First launch](#first-launch)
  - [Command-line options](#command-line-options)
- [Using Prisme](#using-prisme)
  - [Connections](#connections)
  - [Object navigator](#object-navigator)
  - [Editors](#editors)
  - [Running queries](#running-queries)
  - [Transactions](#transactions)
  - [Writing SQL](#writing-sql)
  - [Result tabs](#result-tabs)
  - [Result grid](#result-grid)
  - [Searching the data explorer](#searching-the-data-explorer)
    - [JSONPath](#jsonpath)
    - [XPath](#xpath)
  - [Console](#console)
  - [Top bar](#top-bar)
- [Your data](#your-data)
- [Developers](#developers)
- [License](#license)

## Quick start

1. **Download** the executable for your system from the [latest release](https://github.com/sfaut/prisme/releases/latest): Windows (`.exe`), Linux, or macOS (Apple Silicon and Intel). Nothing else to install.
2. **Run it** from a terminal. On Linux and macOS, first make it executable (see [Installation](#installation)).
3. **Answer the two setup questions**: listening port and your email address. Press Enter to keep the defaults (`8484`, `me@local`).
4. **Open** <http://127.0.0.1:8484> in your browser.
5. **Add a connection**: in the left panel, open the **⋮** menu of **Sources de données** (Data sources), choose **Ajouter une connexion** (Add a connection), fill in host, port, user and password, click **Tester** (Test), then save.
6. **Query**: **Ctrl+click** the new source (**Cmd+click** on macOS), type a query in the editor, press **Ctrl+Enter**.

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
| `--verbose` | Writes the use of the API in the log: each request (method, path, status, duration) and each message of the query sessions, summed up by its type and counts. SQL texts, values and passwords stay out of it. |
| `-v`, `--version` | Prints the version. Prisme also writes it on the first line of its log at each launch. |

Options given on the command line replace the saved settings for that launch, without changing them.

## Using Prisme

The screen has these areas:

- **Top bar**: the name and version of Prisme, the synchronisation state, the theme and your account menu (see [Top bar](#top-bar)).
- **Left panel**: the object navigator, **Navigateur d’objets** (Object navigator), with your connections and their databases.
- **Editor tabs**: several SQL editors, each one on its own connection.
- **SQL cell and results**: the editor, its buttons, and one result tab per query.
- **Data explorer**, on the right when it is open: the JSON or XML values selected in the grid (see [Result grid](#result-grid)).
- **Console**, under the editors: the history of the queries sent to the servers, and the debug log (see [Console](#console)).

The side panels go from the top bar to the bottom of the window. The fold button of a panel's header (left of the title for the data explorer and the console, at the right for the object navigator) folds it towards its edge, where it leaves a narrow rail with its name; a click on the rail opens it again. The object navigator and the console are open by default, the data explorer closed; your choice for the object navigator and the console is remembered.

The version of Prisme shows next to its name, at the top left.

### Connections

Connections are managed from the navigator:

- **Add**: **⋮** menu of **Sources de données** (Data sources) → **Ajouter une connexion** (Add a connection). **Tester** (Test) tries the settings before saving.
- **Edit, refresh, test, delete**: **⋮** menu of each source. A blank password when editing keeps the current one. Deleting asks for confirmation.
- **Session settings**: the **À la connexion** (On connect) field runs queries each time a connection opens, separated by `;`, for example `SET NAMES utf8mb4; SET SESSION sql_mode = ...`. Only session settings are allowed.
- **System databases** (`information_schema`, `mysql`, `performance_schema`, `sys`) can be hidden per source.
- **Supported versions**: MySQL from 8.0.11, MariaDB from 10.6.3. An older server still works, without guarantee: the connection test and the editor connected to it show a warning.

Passwords are encrypted on disk and never sent back to the browser.

### Object navigator

The tree shows, for each source:

- its **databases**, then their **tables**, **views**, **procedures**, **functions** and **events** (**Événements** folder; a disabled event is marked **désactivé**);
- for each table, its **columns**, **indexes**, **foreign keys** and **triggers**.

How it behaves:

- A **click** on a row opens or closes it.
- A **Ctrl+click** (**Cmd+click** on macOS), or **Enter** on the focused row, acts on it without opening or closing it: on a source, makes it the connection of the active editor; on a database, makes it the current database (like `USE`); on a table or view, shows its content: its `SELECT` goes after the last query of the editor, without overwriting anything, and runs; on an index, writes the operations on it after the last query, commented out: adding a primary key, a unique index or an index, then making the index visible or invisible (ignored in MariaDB), removing or renaming it, as far as the version of the server allows (a primary key can only be removed). Uncomment the line to run and fill in the names left as placeholders (`column_name`, `new_index_name`).
- On hovering a row, a button on its right does the same: the **plug** of a database makes it the current database, the **eye** of a table or view shows its content, the **pencil and ruler** of an index writes the operations on it.
- **Déconnecter** (Disconnect), the **unplugged plug** on hovering a source or the current database, or the **⋮** menu of the source, closes the session of the active editor at once: its connection to the server closes, and the editor shows **Session fermée** (Session closed) with **Reconnecter** (Reconnect). It is only available on the source of the active editor, once its session is open, and stays disabled while queries run: stop them first. Afterwards, choosing the source again, making one of its databases current or opening one of its tables also opens a new session; a database made current opens it on that database directly.
- The catalog loads progressively, and the selected connection opens by itself. **Tout rafraîchir** (Refresh all) in the root menu reloads the catalogs already read.
- The current database is in **bold**.
- While the tree scrolls, the source, the database and the table of the rows on screen stay pinned at the top; they open, close and act like the rows of the tree.

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

Each editor has its own connection, its own session (and therefore its own transactions, see [Transactions](#transactions)) and its own results. Everything is saved as you type: texts, editors, result tabs and rows. A page reload or a browser restart brings everything back.

### Running queries

| Action | Key | Button |
|---|---|---|
| Run the query under the cursor | Ctrl+Enter | **Exécuter** (Run) |
| Run every query of the editor: in parallel if they only read, one after another as soon as one writes or a transaction is open | Ctrl+Shift+Enter | **Exécuter tout** (Run all) |
| Run the queries touched by the selected text, the same way | Ctrl+Shift+Enter, with text selected | **Exécuter tout**, with text selected |
| Run only the selected text, the same way: the part of a query outside the selection does not run | | **Exécuter précisément** (Run exactly), with text selected |
| Stop every running query | | The button that started them, **Exécuter**, **Exécuter précisément** or **Exécuter tout**, again; the others wait, disabled |

- When the editor holds several queries, the one Ctrl+Enter would run is **highlighted**.
- **Exécuter précisément** (Run exactly) runs the selected text as it is. With `SELECT ALL id, name, email FROM user WHERE email LIKE '%.fr';` selected from `SELECT` to `user`, only `SELECT ALL id, name, email FROM user` runs. A selection over several queries runs each of their selected parts. Without a selection, the button is disabled.
- **Exécuter tout** (Run all) and **Exécuter précisément** open one result tab per query. How the queries run depends on what they do:
  - When they all read, they run **in parallel**, each on its own connection, opened on the current database, eight at a time at most. A `USE` among them changes neither the other queries nor the editor's session.
  - As soon as one of them writes, sets the session (`SET`), controls a transaction (`START TRANSACTION`, `COMMIT`...) or cannot be parsed by Prisme, or while a transaction is open on the session, they all run **one after another**, in the order of the text, on the editor's session, inside its transaction. A temporary table, a variable or a `USE` is then seen by the queries that follow. The first query that fails stops the batch: the next ones are not run and say so.
- Syntax errors are **underlined in red** as you type, with the message on hover.
- **Row limit**: when a query has no `LIMIT`, Prisme shows at most 1,000 rows (adjustable up to 50,000 with the list next to the buttons) and reports a truncated result. A query that has its own `LIMIT` runs as written.
- **Writes**: reads are always allowed (`SELECT`, `SHOW`, `DESCRIBE`, `EXPLAIN`, `USE`). Each write is allowed by its own switch of the settings (**Écritures**, see [Top bar](#top-bar)), all off by default: `INSERT`, `UPDATE`, `DELETE`, `TRUNCATE`, then `CREATE`, `ALTER`, `RENAME`, `DROP` of the tables, `CREATE`, `ALTER`, `RENAME`, `DROP` of the temporary tables, `CREATE`, `DROP` of the views, functions and procedures, `CREATE`, `ALTER`, `DROP` of the events. `REPLACE` needs `INSERT` and `DELETE`, `INSERT ... ON DUPLICATE KEY UPDATE` needs `INSERT` and `UPDATE`, `CREATE INDEX` / `DROP INDEX` need `ALTER TABLE`, `ALTER VIEW` needs `CREATE VIEW`. The switches of the **Tables temporaires** (Temporary tables) group allow changing the temporary tables without touching the permanent ones: Prisme follows the temporary tables created by the editor's session, and an `ALTER`, `RENAME` or `DROP` that names only them needs these switches; `CREATE TEMPORARY TABLE` and `DROP TEMPORARY TABLE` always do. Their data stays under the switches of the data (`INSERT`, `UPDATE`, `DELETE`, `TRUNCATE`). A table name written in another case than at its creation is taken for a permanent table. The other writes are always refused: `CALL`, databases, triggers, `LOAD DATA`, accounts, global `SET`, `SELECT ... INTO`. A setting of the session (`SET @v = 1`, `SET SESSION sql_mode = ...`, `SET NAMES`) is always allowed, and so are transactions (see [Transactions](#transactions)). While **Sessions en lecture seule** (Read-only sessions), on by default, is on, the database itself refuses the writes, and Prisme refuses them all first. A procedure, function or event whose body is a `BEGIN ... END` block is written after a `DELIMITER` command (`DELIMITER //`, the body, `//`).

### Transactions

A transaction lives on the session of its editor: its queries run on the session's connection, one after another, and each editor has its own transactions.

- **Allowed statements**, always, without a switch, read-only sessions included: `START TRANSACTION` (`READ ONLY`, `WITH CONSISTENT SNAPSHOT`), `BEGIN`, `COMMIT`, `ROLLBACK` (`AND [NO] CHAIN`, `[NO] RELEASE`), `SAVEPOINT`, `ROLLBACK TO SAVEPOINT`, `RELEASE SAVEPOINT`, `SET [SESSION] TRANSACTION` (isolation level, access mode), `SET autocommit`. The writes inside a transaction stay under their own switches (see [Running queries](#running-queries)).
- **Read-only sessions**: while **Sessions en lecture seule** (Read-only sessions) is on, Prisme refuses what would lift the read-only mode, which the servers would otherwise apply: `START TRANSACTION READ WRITE`, `SET [SESSION] TRANSACTION READ WRITE`, and the variables `transaction_read_only` and `tx_read_only`. A plain `START TRANSACTION` and `SET autocommit = 0` keep the session read-only.
- **Always refused**: `SET GLOBAL TRANSACTION`, which changes the default of every session of the server, and XA (distributed) transactions (`XA START`, `XA PREPARE`, `XA COMMIT`...): a prepared XA transaction outlives its session and keeps its locks.
- **Indicator**: while a transaction is open on the session, a **Transaction ouverte** (Transaction open) badge shows at the end of the editor's buttons, until its `COMMIT` or `ROLLBACK`. With `SET autocommit = 0`, a transaction opens again with the first query after each `COMMIT`, and the badge with it. A page that comes back during the transaction shows the badge again.
- **Run all**: while a transaction is open, **Exécuter tout** (Run all) runs the queries one after another on the session, reads included, so that they see the transaction; without one, reads run in parallel on their own connections, outside any transaction.
- **End of the session**: closing the session rolls its open transaction back. While one is open, **Déconnecter** (Disconnect), closing the editor and choosing another source for it (a source, a table or a database of another source in the navigator, a connection just created) ask for a confirmation first; the windows that change or delete the connection warn of it, and their button says the transaction will be rolled back. Without an attached page for the delay **Session gardée** (Session kept) of the connection, the session closes and its transaction is rolled back without asking.
- **Closing by the server**: `COMMIT RELEASE` and `ROLLBACK RELEASE` end the transaction, then the server closes the connection; `SET completion_type = 2` does the same at each `COMMIT` or `ROLLBACK`. The session then ends with a message, as it does after a `KILL` or a restart of the server, and **Reconnecter** (Reconnect) opens a new one.

### Writing SQL

- **Completion** (Ctrl+Space, or while typing): databases, tables, views, functions and procedures of the current database, and columns after `table.` or `alias.`. Only names are suggested, never keywords.
- **Tab** moves to the next tab stop (every 4 spaces, set by the **Indentation** field of the settings), or indents the selected lines. **Shift+Tab** unindents them, like **Backspace** in the indentation.
- **Enter** starts a new line at the indentation of the current line.
- When the cursor leaves a line, the spaces at its end are removed, except inside a string or a quoted identifier that goes on to the next line.
- **Ctrl+Up** / **Ctrl+Down** scroll the editor by one line, without moving the cursor.
- To leave the editor with the keyboard: **Esc** then **Tab**, or **Ctrl+M** (Tab then moves the focus again).
- **Enjoliver** (Beautify) rewrites the query in a consistent style: keywords and function names in capitals, table and column names as written in the catalog, `AS` before aliases, one clause per line, indented by the **Indentation** of the settings, spaces around operators. Stored procedures and `DELIMITER` blocks are supported. Ctrl+Z undoes it. The button is active once the connection's catalog is loaded.

Drag the bar under the editor to share the height between editor and results; double-click it to restore the default.

### Result tabs

Each query gets its own tab:

- A **dot** shows its progress: grey while waiting its turn, blue and pulsing while running and receiving rows, green when done, red on failure.
- A **progress bar** on its bottom edge fills while rows arrive.
- **Double-click** the label to rename the tab. The **pin** keeps the tab when queries are run again. The **cross** (or a middle click) closes it and stops its query. Drag a tab to move it.
- The bar under the grid shows the number of rows, the execution time, the transfer time, the data received, the throughput, and the date and time (to the second) the execution ended: the instant the result reflects.

### Result grid

| Action | How |
|---|---|
| Sort | ⇅ button of a column header: ascending, descending, none |
| Refresh | Refresh button, at the far left of the bar under the grid: the query runs again in its tab |
| Filter | Filter button, left of the bar under the grid, on all columns or on one |
| Move a column | Drag its name |
| Resize a column | Drag the right edge of its header; double-click the edge to fit the longest value |
| Select | Click a cell, a row number or a column name; drag for a range; click the top-left corner for everything |
| Extend / add to the selection | Shift+click / Ctrl+click |
| Select all / clear | Ctrl+A / Esc |
| Copy | Ctrl+C |
| Download as CSV | Download button, right of the bar under the grid |
| Export the full result as CSV | Download button, then **Résultat complet en CSV (requête relancée)**; followed in the exports manager of the top bar |
| Explore JSON or XML values | Rail of the data explorer, on the right edge of the window |

- **Refresh** keeps the tab, its name and its pin, and, while the columns stay the same, the sort and the filter. The current result stays displayed until the new one starts to arrive. While other queries run, the refresh waits for their end. During the refresh, or while it waits, the same button (crossed out) stops it: the result stays as it was. If the query fails, the result stays too, with the error above it.
- **Copy** is a raw copy: values as they are, separated by tabs, with column names on the first line, ready to paste into a spreadsheet. A single cell is copied as its value alone, without its column name. `NULL` is copied as an empty cell. Control characters are written literally (`\t`, `\n`, `\r`, `\0`, `\b`, `\f`…, or `\xHH`), so each value stays in one cell.
- **Download** saves a CSV file that Excel in French opens with a double click: UTF-8 with a BOM, `;` as separator. With a selection, it holds the selected cells only, an unselected cell among the selected rows and columns left empty; without one, the displayed result, sorted and filtered as on screen. The menu says which one it downloads, with its size. Columns come in the displayed order, with their names on the first line. `NULL` is an empty field and an empty string is `""`, which Excel both shows as an empty cell. Decimal numbers take the decimal comma (`3,14`), so that Excel reads them as numbers; other values are written as stored, line breaks included. Columns sharing a name refuse the download. Excel itself opens at most 1,048,576 rows, rounds integers beyond 15 digits and drops the leading zeros of codes such as `007`: the file keeps them all.
- **Full export** (**Résultat complet en CSV (requête relancée)** in the download menu) runs the query again on the server, as written, without Prisme's row limit (a `LIMIT` you wrote still applies), and writes the same CSV file in the `exports` folder of [your data](#your-data), however many rows. Columns keep their displayed order; the sort and the filter of the grid do not apply: write them in SQL (`ORDER BY`, `WHERE`). Only reads can be exported. The export runs on a read-only connection of its own, on the current database of the editor: it sees neither the temporary tables nor the variables of the editor's session, nor its open transaction. The [exports manager](#top-bar) follows it, and a message tells when its file is ready, with a **Télécharger** (Download) button.
- **Display**: the header shows each column's type (hover for the full name). Numbers and dates are right-aligned, `CHAR` columns use a monospaced font, web addresses are clickable, `NULL` is shown in small grey capitals.
- **Data explorer**: a panel on the right of the screen shows the values selected in a column of type `JSON`, or in a text column that holds JSON or XML: a cell, the whole column, or ranges of it. The values are indented with 4 spaces and highlighted; numbers and texts stay exactly as stored. Several values are gathered into one document: a `[…]` array for JSON (`NULL` becomes `null`), a `<data>…</data>` element for XML (`NULL` is left out, `<?xml?>` declarations are removed). Each level folds with the arrows in the margin; the buttons fold or unfold everything, or every node at the depth of the cursor. The line wrap button wraps long lines to the width of the panel; it is off by default and your choice is remembered. The search button finds values with JSONPath or XPath (see [Searching the data explorer](#searching-the-data-explorer)). The copy button, or Ctrl+A then Ctrl+C, copies valid JSON or XML, folded parts included. The panel follows the selection, and the grid of the active editor; drag its left edge to resize it (double-click to reset), the width is remembered.

### Searching the data explorer

The search button (magnifying glass) of the data explorer opens a field above the document. Type a JSONPath expression for JSON, an XPath expression for XML: the results follow as you type.

- **Selection mode**, the default: the results are highlighted in the document. Enter and Shift+Enter, or the arrows beside the field, go to the next and previous result, unfolding what hides it. The counter shows the current result and the total.
- **Filter mode**, with the funnel button: the document shows only the results, and the arrows and the counter go through them, the current one highlighted. For JSON, an array of the values found. For XML, a `<data>…</data>` element with the nodes found, attributes and texts as text. An XPath expression that computes a number, a string or a boolean (`count(//item)`) shows that value.
- The search runs on the document shown. With several values, that is the gathered `[…]` array or `<data>` element: `$[*].name`, `/data/client/name`.
- While the expression is invalid, the error shows under the field and the previous results stay. Esc closes the field.
- The copy button copies the document shown: in filter mode, the results.

#### JSONPath

JSONPath follows its standard, [RFC 9535](https://www.rfc-editor.org/rfc/rfc9535) (2024), and only it: the extensions of other tools (`^`, `~`, `@parent`, JavaScript in filters) are refused.

| Expression | Finds |
|---|---|
| `$` | The whole document |
| `$.name`, `$['first name']` | Member `name` of the root object; brackets for a name with spaces or dashes |
| `$.items[0]`, `$.items[-1]` | First, last element of the array `items` |
| `$.items[0:3]`, `$.items[::2]` | Slices: the first three elements, every other element |
| `$.items[*]`, `$.*` | All the elements of `items`, all the members of the root |
| `$..name` | Every member `name`, at any depth |
| `$.items[?@.price > 10]` | Filter: the elements whose `price` is above 10 |
| `$.items[?@.tag == 'new' && @.stock]` | Conditions with `&&`, `\|\|`, `!` and parentheses; `@.stock` alone: the member exists |
| `$.items[?length(@.tags) > 2]` | Functions: `length()`, `count()`, `match()` (the whole value matches a regular expression), `search()` (a part matches), `value()` |

Comparisons: `==`, `!=`, `<`, `<=`, `>`, `>=`; strings between single or double quotes. In a filter, numbers are compared as JavaScript numbers: an integer beyond 2⁵³ is approximated. It is still shown as stored.

#### XPath

XPath is the version built into browsers, [XPath 1.0](https://www.w3.org/TR/1999/REC-xpath-19991116/) (W3C, 1999). XPath 2.0 and later (sequences, `if`, date functions…) are not available.

| Expression | Finds |
|---|---|
| `/order` | The root element `order` |
| `/order/item` | The children `item` of the root |
| `//item` | Every `item`, at any depth |
| `//item[1]`, `//item[last()]` | The first, the last `item` of each parent |
| `//item/@id`, `//@*` | The attributes `id` of the `item`, all the attributes |
| `//item[@id = '42']` | Filter on an attribute |
| `//item[price > 10]` | Filter on the value of a child |
| `//item[contains(name, 'blue')]` | Functions: `contains()`, `starts-with()`, `normalize-space()`, `string-length()`, `not()`… |
| `//item/text()`, `//comment()` | Texts, comments |
| `//a \| //b` | Union |
| `count(//item)`, `sum(//price)` | A value: number, string or boolean |

- **Namespaces**: the prefixes declared in the document can be used (`//p:item`). XPath 1.0 has no default namespace: an element in a namespace without prefix is found with `//*[local-name() = 'item']`.
- **Blanks**: the indentation the explorer adds between elements is not part of the document, so `//text()` finds only the texts of the values.

### Console

The console, under the editors, shows what Prisme does besides the results. Drag its top edge to change its height; double-click the edge to restore its default height.

The **Historique** (History) tab lists every query sent to your servers, the oldest at the top: those of your editors, and those Prisme sends itself (opening of a session, object navigator, connection test, interruption). New queries show as they are sent. Passwords of the server's own syntaxes are masked (`'***'`): `IDENTIFIED BY`, `PASSWORD('…')`, `SET PASSWORD`, `MASTER_PASSWORD`, `OPTIONS (PASSWORD …)` of `CREATE SERVER`…; the data of your databases stay as written, and the text of your editors keeps everything as typed. Each line gives:

| Column | Content |
|---|---|
| **#** | Number of the query in the history |
| **Heure** (Time) | Time it was sent; the date shows in its tooltip |
| **Action** | What sent it: **Session**, **Exécuter** (Run), **Exécuter tout** (Run all), **Exécuter précisément** (Run exactly), **Rafraîchir** (Refresh), **Base courante** (Current database), **Interruption**, **Navigateur** (Navigator), **Test de connexion** (Connection test), **Export** (full export); the connection shows in its tooltip |
| **Message** | The query on one line, then its error if it failed; the full query shows in its tooltip |
| **Lignes** (Rows) | Rows read, preceded by "+" when the result was truncated ("+1 000"), or rows changed by a write; "—" otherwise |
| **Exécution** (Execution) | Execution time, until the result, or until the columns for a result read row by row |
| **Transmission** | Transfer time of the rows of a result read row by row; "—" when not measured |

The **Debug** tab lists the messages of Prisme: those its terminal shows (the same level, see [Command-line options](#command-line-options)), and the notifications of the interface, such as **Sélection copiée** (Selection copied). Each line gives its number, time, level (**Erreur**, **Avertissement**, **Info**, **Debug**, **Trace**), source (**Serveur** or **Interface**; the module of the server shows in its tooltip) and message.

Both tabs are kept in the internal database, 30 days by default: the **Rétention historique** and **Rétention debug** fields of the settings change it, from 1 to 3,650 days. Older lines are deleted when Prisme starts, then every hour. The interface shows their last 2,000 lines; scrolling up to the top of the list reads the older ones. The button at the right of the tabs clears the list of the tab shown: **Effacer l’historique** (Clear the history) deletes your history, **Effacer le debug** (Clear the debug log) the whole debug log.

### Top bar

- **Exports**: the file icon opens the exports manager, the list of your [full exports](#result-grid), the most recent first; a blue dot pulses beside the icon while an export runs. Each export shows its file name (the query in its tooltip), its state, its connection, the rows written, the size of its file, its duration and its throughput; while it runs, the completed share too, when the total is known in advance: that of a displayed result that Prisme's limit did not cut. Its buttons stop a running export (its partial file stays), download a ready one, and **Retirer** (Remove) an export with its file. The folder is never cleaned automatically: the head of the manager shows the room the files take. Warnings tell of a file deleted outside Prisme, of more rows than Excel opens, or of the cause of a failure. Files of the folder that no export lists (copied there, internal database replaced) are counted apart, with a **Retirer** button that deletes them. An export running when Prisme stops shows as **interrompu** (interrupted) at the next start, its partial file kept, with the rows it holds.
- **Synchronisation**: Prisme saves every change as you go in its local database, and reads it back from there. A database icon shows the state, explained by its tooltip: a check mark when everything is saved, a down arrow while reading, an up arrow while saving, a cross when the last save was refused (plain database icon: nothing exchanged yet).
- **Theme**: the **Système / Clair / Sombre** (System / Light / Dark) list sets the theme of the whole interface. **Système** follows your computer's setting.
- **Account** menu, the round avatar with your initials (those of your first and last names, or the first letter of your email address):
  - **Profil** (Profile): your last and first names, and your email address (read-only);
  - **Paramètres** (Settings): the language of the interface (French only for now); the default row limit; **Sessions en lecture seule** (Read-only sessions), on by default: each query session asks the database to refuse its writes, on top of Prisme's own check; **Écritures** (Writes), the switches of the writes allowed, greyed out while the sessions are read-only; the **Indentation** of the SQL editor and of **Enjoliver**, from 1 to 8 spaces (4 by default); and how many days the [console](#console) keeps its history and its debug log.

## Your data

Prisme keeps its data in a folder of your user account:

| System | Default folder |
|---|---|
| Windows | `%APPDATA%\Prisme` |
| Linux | `~/.local/share/Prisme` |

It holds two files that go together:

- **`prisme.db`**: the internal database (settings, connections, editors, results, history of the queries, debug log).
- **`prisme.key`**: the master key, created on first launch. It encrypts the passwords of your connections.

The **`exports`** folder holds the files of the full exports, until you remove them from the exports manager.

- **Back up** both files. Without the key, saved passwords cannot be read and must be typed again; everything else is kept.
- **Protect** the folder: whoever has both files can read the saved passwords. By default, only your user account can read it.
- **Never edit `prisme.key`**: Prisme refuses to start with an invalid key, and never overwrites it.

## Developers

Building from source, tests, internal API, continuous integration and releases: see [docs/DEVELOPER.md](docs/DEVELOPER.md).

## License

MIT, see [LICENSE](LICENSE).
