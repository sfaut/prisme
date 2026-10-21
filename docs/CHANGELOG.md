# Changelog

Changelogs of the published versions of Prisme, most recent first. Each entry is also the notes of its GitHub release. The interface is in French: its labels are quoted as they appear, with a translation.

## v0.5.0 (2026-10-22)

### 🚀 Launch

We are proud to announce Prisme v0.5.0. This version speaks French in its errors: the messages of Prisme are written by the interface, and the answers of the servers are quoted with who wrote them, a first step towards a multilingual interface. The object explorer acts on its lines with Ctrl+click, Enter or a button of the line, and no longer on a double click. The run buttons stop the queries they started, the settings get their own window with a save button, and every tooltip shows in a single themed bubble, above its element.

### ✨ New features

- Object explorer ([#19](https://github.com/sfaut/prisme/issues/19)):
  - **Ctrl+click** (**Cmd+click** on macOS), or **Enter** on the focused line, acts on it without opening or closing it: chooses the source for the active editor, makes the database the current one (like `USE`), or shows the content of the table or view;
  - on hovering a line, a button on its right does the same: the plug of a database, the eye of a table or view;
  - the tooltip of the flavor badge of a source gives the name and version of its server, as `SELECT VERSION()` returns it (`MariaDB 10.11.11-MariaDB`, `MySQL 8.4.11`), even before its catalog is opened.
- Errors: the errors of Prisme are written in French by the interface, from codes that the server sends with their parameters. This is a first step towards a multilingual interface: each message of Prisme is written in a single place, ready to be translated. The texts written outside Prisme are quoted as they are, introduced by who wrote them: the answer of the server with its error number and SQLSTATE, the message of the driver, of the system or of the SQL parser. When the Prisme server is out of reach, a message says so instead of the browser's text (`Failed to fetch`).
- Run buttons: while queries run, the button that started them, **Exécuter** (Run) or **Exécuter tout** (Run all), turns into a red stop button; the other one stays disabled until the end. They replace the **Stopper** (Stop) button.
- Settings: the read-only sessions and the retention of the console's history and debug log move from the profile to the **Paramètres** (Settings) window, ordered by subject; the window saves with **Enregistrer** (Save) and closes without saving with **Annuler** (Cancel).
- Tooltips: every tooltip of the interface shows in a single themed bubble, above its element so that the pointer does not hide it, also on keyboard focus.
- Top bar: the state of the synchronisation with the internal database shows as an icon, with its text in a tooltip.
- Message banners: an icon per tone (information, success, warning, error), and every banner can be closed.
- Look: thinner scrollbars, the result grid attached to its tabs, and the spacing of the windows and panels made even.

### 💥 Breaking changes

- Object explorer: a double click on a line no longer acts on it: it opens and closes the line. Use Ctrl+click, Enter or the button of the line.
- API: the read-only sessions and the retentions of the console move from `/api/profile` to `/api/settings`.

### 🗑️ Removed features

- The tooltips of the values of the result grid.
- The connection and server label next to the row limit of the query cell.

### 🐞 Bug fixes

- A target server out of reach no longer shows as a failed save in the top bar.
- No more vertical scrollbar beside the editors' tabs.
- Object explorer: typing a letter on a line, to move to the line that starts with it, no longer throws an error.

### 🔒 Security

- `npm audit` reports [GHSA-vfj7-8cjw-p6xm](https://github.com/advisories/GHSA-vfj7-8cjw-p6xm) (`braces`, high), which has no patched version. No impact on Prisme: `braces` only serves the linting of the interface's sources, during development, and never gets into the executable.

### ⚙️ Internal improvements

- Dependencies:
  - `lucide-vue-next` 1.0.0 replaced by `@lucide/vue` 1.61.0: Lucide now publishes its Vue icons under this name, and `lucide-vue-next` stopped at 1.0.0, without the new icons;
  - `tokio` 1.53.3 → 1.54.1;
  - `ts-rs` 12.0.1: `format` feature in the tests, which format the generated TypeScript types;
  - `vite` 8.3.4 → 8.3.5;
  - `@codemirror/autocomplete` 6.20.3 → 6.20.4, `@codemirror/view` 6.43.14 → 6.43.15;
  - `@tanstack/vue-virtual` 3.13.46 → 3.13.47;
  - `eslint` 10.12.0 → 10.13.0, `oxlint` and `eslint-plugin-oxlint` 1.88.0 → 1.89.0, `npm-run-all2` 9.0.3 → 9.0.4.
- The generated TypeScript types are formatted, and readable.
- The lengths of the interface are written on Tailwind's spacing scale, with its canonical classes, and containers space their children without external margins.
- Release process: the dependencies are updated to their latest version and checked before each version, the security advisories are compared with the known ones, the changelog gives the reason of a replaced dependency, the merged branches are deleted once the version is published, and the local commits leave only once everything has been checked.

### 📝 Documentation

- Developer guide: badges of the main dependencies, and sections on the pinned versions, the held-back dependencies and the known security advisories.
- README: the new gestures of the object explorer.

## v0.4.0 (2026-10-20)

### 🚀 Launch

We are proud to announce Prisme v0.4.0. This version brings a console under the editors, with the history of every query sent to the servers and a debug log, side panels that fold on a rail of their edge, and a JSONPath / XPath search in the data explorer. A result tab can now run its query again in place, and the queries that use the compound time units of MySQL and MariaDB are no longer refused.

### ✨ New features

- Console, under the editors, open by default:
  - the **Historique** (History) tab lists every query sent to the servers, by the editors and by Prisme itself (opening of a session, object explorer, connection test, interruption): its time, action, SQL, error if any, number of rows (`+1 000` for a truncated result), duration and transfer time of the rows. The passwords of the server's own syntaxes (`IDENTIFIED BY`, `SET PASSWORD`, `MASTER_PASSWORD`…) are masked; the data of your databases stay as written;
  - the **Debug** tab lists the messages that the terminal of Prisme shows, and the notifications of the interface;
  - both are kept 30 days by default, from 1 to 3,650 days with the **Historique (jours)** and **Debug (jours)** fields of the profile; a button at the right of the tabs clears the list of the tab shown;
  - its height is set by dragging its top edge, and it folds on a rail of the bottom edge.
- Side panels: the object explorer and the data explorer fold towards their edge, where they leave a rail with their name that opens them again. The data explorer now opens from its rail on the right, instead of the button of the bar under the grid.
- Result tabs: a button of the bar under the grid runs the tab's query again, in the same tab, and stops this refresh; the bar shows when the execution ended ([#18](https://github.com/sfaut/prisme/issues/18)).
- Data explorer: a search field runs a JSONPath expression on JSON or an XPath expression on XML, highlights the results and moves from one to the next (Enter, Shift+Enter), or shows only the results; a button toggles line wrapping.
- Top bar: the account menu opens from an avatar with your initials, with **Profil** (Profile) and **Paramètres** (Settings); the version shows next to the name of Prisme, in smaller type. The left panel is titled **Explorateur d’objets** (Object explorer), and the profile shows the email address right under the first name.
- `--verbose` writes the use of the API in the log: each request (method, path, status, duration) and each message of the query sessions, without SQL texts, values or passwords.

### 🐞 Bug fixes

- The queries that use the compound time units of MySQL and MariaDB, such as `EXTRACT(YEAR_MONTH FROM d)` or `INTERVAL '1-2' YEAR_MONTH`, are no longer refused as not understood.

### ⚙️ Internal improvements

- A query session reads its current database again only after a query that can change it (`USE`, `DROP DATABASE`), instead of after every query; the queries run in parallel open their connection directly on it.
- Development: `rebuild.ps1 -Force` stops any Prisme executable, `-Verbose` starts it with `--verbose`, and the script stops with its reason when Prisme cannot be stopped.

### 📝 Documentation

- README: table of contents, badges for the state of the CI (release runs), the last release and the license; the developer guide shows the pinned Rust version in a badge.

## v0.3.0 (2026-10-19)

### 🚀 Launch

We are proud to announce Prisme v0.3.0. This version brings a data explorer that shows the JSON and XML values of the grid indented and foldable, a top bar with the settings and the profile, and a SQL editor that keeps the indentation, scrolls without moving the cursor and runs the queries a selection touches. Query sessions are now read-only on the database side by default, and the local server refuses the requests of other sites. The first rows of a result show sooner, and large results no longer freeze the interface.

### ✨ New features

- Data explorer: a panel on the right of the screen, opened from the bar under the grid, shows the values selected in a JSON or XML column (a cell, the whole column, ranges of it), indented with four spaces and highlighted, numbers and texts exactly as stored. Several values are gathered into one document, `[…]` for JSON, `<data>…</data>` for XML. Each level folds; buttons fold or unfold everything, or the level of the cursor; the copy gives valid JSON or XML. The panel follows the selection and the active editor; its width is set by dragging its left edge, and remembered ([#11](https://github.com/sfaut/prisme/issues/11)).
- Top bar, across the screen: the theme choice moves there, and its **Options** menu opens the settings (default row limit; interface language, French only for now) and the profile (first and last names). A marker tells whether every change is saved by the server, pending or refused.
- The version of Prisme shows beneath its name, and in the terminal at launch ([#14](https://github.com/sfaut/prisme/issues/14)).
- SQL editor:
  - Enter starts the new line at the indentation of the current line ([#13](https://github.com/sfaut/prisme/issues/13));
  - Ctrl+Up and Ctrl+Down scroll by one line without moving the cursor ([#17](https://github.com/sfaut/prisme/issues/17));
  - the spaces at the end of a line are removed when the cursor leaves it, except inside a string or a quoted identifier ([#15](https://github.com/sfaut/prisme/issues/15));
  - with text selected, Ctrl+Shift+Enter and the **Tout exécuter** (Run all) button run only the queries the selection touches; without a selection, all of them ([#16](https://github.com/sfaut/prisme/issues/16)).
- Results: the first 50 rows show without waiting for a full batch of 500 ([#9](https://github.com/sfaut/prisme/issues/9)).
- Object explorer: the source, the database and the table stay pinned at the top while their content scrolls ([#8](https://github.com/sfaut/prisme/issues/8)); the events of each database show in an **Événements** (Events) folder.
- Result grid: after a reconnection, a result with the same columns keeps its column order, widths, sort and filter; a result with other columns keeps the filter text.

### 🐞 Bug fixes

- Opening a table from the explorer no longer replaces the text of the editor: its `SELECT` goes after the last query, which gets its missing separator, and runs ([#10](https://github.com/sfaut/prisme/issues/10)).
- The server no longer fails on a text with an accented character outside a string, such as `è` in a name ([#12](https://github.com/sfaut/prisme/issues/12)).
- **Stopper** (Stop) pressed right after a run stops it: the whole batch could still run.
- A text typed less than half a second before the page closes is saved.
- A pin or a rename of a result tab that the server refuses is undone, with an error message.
- The keyboard focus of the result grid is visible.
- **Enjoliver** (Beautify): a column named like a time unit (`day`) no longer goes to capitals after `EXTRACT`, and `INTERVAL f(x) DAY` keeps its unit in capitals.
- A source whose connection was deleted no longer offers a menu that opened the wrong window; two quick changes of dialect no longer mix their keywords; an unreadable message of the session shows an error instead of stopping its updates.

### 🔒 Security

- The local server only accepts the requests of this machine: a page of another site can no longer reach it, by DNS rebinding, to run queries on the saved connections.
- Query sessions are read-only on the database side by default: the profile setting **Sessions en lecture seule** (Read-only sessions), on by default, makes each connection run `SET SESSION TRANSACTION READ ONLY`, so that the server refuses a write Prisme could not classify.

### ⚙️ Internal improvements

- Performance: a sorted grid merges the new rows instead of sorting the whole result again (15 s of main thread for 50,000 rows before), and computes a large result frame by frame; beautify and analysis run outside the server's async threads, and go faster on long texts.
- The continuous integration builds the executables for the releases only, and skips the changes of documentation alone.
- The query session is split into modules; panics are logged with their location.
- The repository is in English (code, comments, commits, tickets); the interface stays in French.
- For developers, `rebuild.ps1` rebuilds and restarts Prisme.

### 📝 Documentation

- Design documents, developer guide and changelog in English; the README describes every new feature.

## v0.2.0 (2026-10-16)

### 🚀 Launch

We are proud to announce Prisme v0.2.0. This version asks for its settings on first launch, makes the SQL editor handle Tab and Enter like a code editor, and refines the result grid: monospaced `CHAR` columns, a discreet `NULL` marker, and a raw copy that keeps one value per cell. The README is now in English, with a quick start.

### ✨ New features

- First-launch setup: Prisme asks in the terminal for the listening port (8484 by default) and the user's email address (`me@local` by default), saves them in its internal database, and does not ask again. `prisme --setup` asks the questions again; `--listen` and `--user-email` only apply to one launch. An existing installation keeps its settings, without any question ([#6](https://github.com/sfaut/prisme/issues/6)).
- SQL editor: Tab moves to the next tab stop (every four spaces) or indents the selected lines, Shift+Tab unindents them; Esc then Tab, or Ctrl+M, leave the editor with the keyboard ([#2](https://github.com/sfaut/prisme/issues/2)).
- Result grid: `CHAR` columns are displayed in a monospaced font ([#4](https://github.com/sfaut/prisme/issues/4)), and the `NULL` marker in small capitals, more discreet ([#1](https://github.com/sfaut/prisme/issues/1)).

### 💥 Breaking changes

- The short option that prints the version becomes `-v`; `-V` is no longer recognised, `--version` is unchanged ([#7](https://github.com/sfaut/prisme/issues/7)).

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

We are proud to announce Prisme v0.1.0, the first public version of Prisme, a multi-database web client shaped like a SQL notebook. This v0.1.0 queries MySQL and MariaDB; PostgreSQL, SQLite, DuckDB and translation between dialects will come next.

- A single executable per platform, interface included: Windows (x86_64), Linux (x86_64) and macOS (universal, Apple Silicon and Intel). Nothing else to install.
- Prisme runs locally and opens in the browser, at <http://127.0.0.1:8484>.
- The executables are not signed yet: Windows shows a SmartScreen warning on first launch, and macOS asks to lift the file's quarantine (procedure in the README).

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

### 🔒 Security

- Connection passwords encrypted in the internal database (AES-256-GCM), never written to the logs nor sent back by the API.
- Listening limited to the local machine: without authentication, Prisme refuses to be reachable from the network.
- Read-only: query sessions refuse writes (`UPDATE`, `DELETE`, DDL, `CALL`…), detected by analysing the query, until the write switches exist.

### 📝 Documentation

- README: installation, launch, use of the interface and first launch of the unsigned executables.
