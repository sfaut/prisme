# Changelog

Changelogs of the published versions of Prisme, most recent first. Each entry is also the notes of its GitHub release. The interface is in French: its labels are quoted as they appear, with a translation.

## v0.8.0 (2026-10-29)

### 🚀 Launch

We are proud to announce Prisme v0.8.0. This version downloads results as CSV files that Excel in French opens with a double click: the selection or the displayed result, built in the page, and the full result of a query, run again by the server without Prisme's row limit, however many rows. An exports manager in the top bar follows the full exports, tells when a file is ready, and keeps the files until you remove them. The interface is redrawn on Tailwind's default scales and palette, and every keyword of the transaction statements is highlighted.

### ✨ New features

- Download as CSV for Excel in French (UTF-8 with a BOM, `;` as separator), with the download button right of the bar under the grid:
  - the selection, or the displayed result without one, sorted and filtered as on screen, its columns in the displayed order; the menu says which one it downloads, with its size;
  - `NULL` is an empty field and an empty string `""`; decimal numbers take the decimal comma, so that Excel reads them as numbers; columns sharing a name refuse the download;
  - the file is built in the page by chunks of rows: a large result does not freeze the interface.
- Full export, **Résultat complet en CSV (requête relancée)** (Full result as CSV, query run again) in the same menu:
  - the server runs the query again as written, without Prisme's row limit (a `LIMIT` you wrote still applies), and writes its rows to a file of the `exports` folder of your data as they arrive, however many rows;
  - only reads can be exported, on a read-only connection of its own, on the current database of the editor; the columns keep their displayed order;
  - the query shows in the history of the console, under the **Export** action.
- Exports manager, a button of the top bar, with a dot that pulses while an export runs:
  - each export shows its state in a colored badge (**en cours**, **prêt**, **échoué**, **arrêté**, **interrompu**: running, ready, failed, stopped, interrupted), its rows, the size of its file, its duration and its throughput, followed live; the completed share too, when the total is known in advance;
  - a message tells when a file is ready, with a **Télécharger** (Download) button; a failure gets an error message;
  - a running export can be stopped: its query stops on the server at once; the file of an ended export can be downloaded, partial for a stopped, failed or interrupted one;
  - **Retirer** (Remove) deletes an export with its file; nothing is deleted automatically, and the head of the manager shows the room the files take; a file deleted outside Prisme gets a **fichier manquant** (missing file) badge, and the files no export references are counted apart, with a **Retirer** button of their own;
  - an export running when Prisme stops shows as **interrompu** (interrupted) at the next start, with the rows of its partial file;
  - warnings tell of more rows than Excel opens (1,048,576) and of the cause of a failure.
- SQL editor: every keyword of the transaction statements is highlighted, and **Enjoliver** (Beautify) puts it in capitals: `START TRANSACTION ... WITH CONSISTENT SNAPSHOT`, `READ ONLY`, `COMMIT WORK AND NO CHAIN`, the savepoints, `SET GLOBAL | SESSION TRANSACTION ISOLATION LEVEL ...` and the XA statements ([#28](https://github.com/sfaut/prisme/issues/28)).
- Results: the user filter belongs to its result tab, kept by a refresh and when a session resumes; a query in error can be refreshed.
- Object navigator: each column has a single icon that tells its key: a key for the primary key, a link for a foreign key.
- Look:
  - the interface follows Tailwind's default scales, at 75% of the browser's text size, and its palette, in light and dark;
  - each icon keeps one size wherever it appears, and the scrollbars are a little wider;
  - the split lines are one screen pixel thick, whatever the scale of the screen and the size of the window;
  - the status under the grid is a bar of items; the rows of the console take the height of their text; the top bar has more room;
  - the data explorer is wide enough, at its smallest, for its header on one line.

### 🐞 Bug fixes

- The data explorer, and the editor and the results of a cell, no longer reopen at their smallest size after a reload. The layouts saved by the previous version start again once from their default sizes.
- **Enjoliver** (Beautify) keeps the space after an operator that follows a non-reserved keyword used as a name: `start = 1`, `temporary = 1`, `END + 1` ([#28](https://github.com/sfaut/prisme/issues/28)).
- The JSONPath search of the data explorer finds a very large number of nodes (a million measured), where it reported too many results beyond about 200,000; `$.a-b` is refused, as RFC 9535 writes it `$['a-b']`.

### ⚙️ Internal improvements

- Dependencies:
  - `tokio-util` 0.7.20 added, with the `fs` and `io-util` features of `tokio`, to write the files of the full exports and send them;
  - `json-p3` 2.3.1 → 3.0.2: 3.0.2 no longer requires TypeScript 7;
  - `@lucide/vue` 1.61.0 → 1.64.0, `@tanstack/vue-virtual` 3.13.48 → 3.13.49, `oxlint` and `eslint-plugin-oxlint` 1.89.0 → 1.90.0, `eslint-plugin-vue` 10.11.1 → 10.12.0;
  - `rusqlite` stays at 0.40.2: `rusqlite_migration` 2.6.0 does not accept 0.41.1 yet; `typescript` stays at 6.0: `vue-tsc` and `typescript-eslint` do not support TypeScript 7 yet.
- The CSV of the browser and that of the server are checked against the same reference files.
- The colors of the interface are roles declared in the theme, each with its light and dark values; the sizes are Tailwind's own classes.

### 📝 Documentation

- README: the download as CSV, the full export and the exports manager, with the limits of Excel (rows, digits, leading zeros, formulas); the refresh and filter buttons placed right of the result tabs.
- Design brief of the interface, with its screenshots and the scripts that take them again (`design/`).
- Developer guide: the routes of the exports; the lines drawn as borders.
- Design notes: the decisions on copying and exporting results (`docs/IDEAS-EXPORTS.md`); the trial of the object navigator without virtual scrolling (`docs/PERFORMANCES.md`, formerly `docs/PERF.md`); the idea of reading the details of a table on opening (`docs/QUESTIONS.md`).

## v0.7.0 (2026-10-26)

### 🚀 Launch

We are proud to announce Prisme v0.7.0. This version brings transactions: `START TRANSACTION`, `COMMIT`, `ROLLBACK` and savepoints are always allowed, a **Transaction ouverte** (Transaction open) badge follows the transaction of each editor, and Prisme asks before closing a session whose transaction would be rolled back. The temporary tables get their own write switches, **Exécuter précisément** (Run exactly) runs only the selected text, and the object navigator closes and reopens the editor's session. The interface is laid out with splitters: the side panels, the editors, the results and the console share the room, fold into their rails and remember their sizes.

### ✨ New features

- Transactions:
  - always allowed, without a switch, read-only sessions included: `START TRANSACTION` (`READ ONLY`, `WITH CONSISTENT SNAPSHOT`), `BEGIN`, `COMMIT`, `ROLLBACK` (`AND [NO] CHAIN`, `[NO] RELEASE`), savepoints, `SET [SESSION] TRANSACTION`, `SET autocommit`. The writes they hold stay under their switches. A read-only session only refuses what would lift its mode: `START TRANSACTION READ WRITE`, `SET TRANSACTION READ WRITE`, and the variables `transaction_read_only` and `tx_read_only`;
  - a **Transaction ouverte** (Transaction open) badge shows at the end of the editor's buttons while a transaction is open, until its `COMMIT` or `ROLLBACK`, also after a page reload;
  - while a transaction is open, **Exécuter tout** (Run all) runs every query one after another on the editor's session, reads included, so that they see the transaction;
  - **Déconnecter** (Disconnect), closing the editor and choosing another source for it ask for a confirmation while a transaction is open, as closing the session rolls it back; the windows that change or delete the connection warn of the transactions open on it;
  - when the server closes the session's connection (`COMMIT RELEASE`, `completion_type = 2`, `KILL`, restart of the server), the session ends with a message, and **Reconnecter** (Reconnect) opens a new one;
  - `SET GLOBAL TRANSACTION` is refused, as it changes every session of the server, and so are XA transactions (`XA START`, `XA PREPARE`...), with their own message.
- Temporary tables: a **Tables temporaires** (Temporary tables) group of write switches allows `CREATE`, `ALTER`, `RENAME` and `DROP` of the temporary tables created by the editor's session, without touching the permanent ones; their data stays under the switches of the data, which now hold `TRUNCATE` too ([#24](https://github.com/sfaut/prisme/issues/24)).
- SQL editor:
  - **Exécuter précisément** (Run exactly) runs only the selected text: the part of a query outside the selection does not run ([#23](https://github.com/sfaut/prisme/issues/23));
  - the `TEMPORARY` keyword is highlighted ([#26](https://github.com/sfaut/prisme/issues/26));
  - the completion quotes the names that are reserved words (`order`) or start with a digit (`1e5`), which the server would otherwise read as a keyword or a number;
  - the line numbers stand alone in their gutter, as wide as the row numbers of the grid.
- Object navigator:
  - the object explorer is renamed **Navigateur d'objets** (Object navigator) ([#27](https://github.com/sfaut/prisme/issues/27));
  - **Déconnecter** (Disconnect) closes the editor's session; choosing its source again, or one of its databases or tables, reopens it, directly on the chosen database ([#25](https://github.com/sfaut/prisme/issues/25));
  - its own icons for showing the content of a table, using a database and the columns of a foreign key.
- Layout: the object navigator, the editors and the data explorer share the width, and the editors, the results and the console share the height, with splitters that remember their sizes. Each panel folds into its rail by its button, by a drag below its smallest size or by a double click on its line; the console no longer covers the result grid. The fold buttons of the editor, the results and the console stand at the end of their bars, and the refresh and filter buttons of a result move next to its tabs.
- Console: filters by action, level and source, shown with the funnel button; while hidden, they still apply, and the button says so.
- Result grid: Ctrl+C on a single cell copies its value alone, without its column name.
- Servers: each engine has an oldest supported version (MySQL 8.0.11, MariaDB 10.6.3, PostgreSQL 14.0). An older server still works, with a warning in the connection test and in the editor.
- Look: the badges of the sources are larger, and the status under the grid shows whole, in the small size.

### 🐞 Bug fixes

- After **Reconnecter** (Reconnect), the object navigator no longer keeps the previous session's current database in bold.
- Choosing a database of a closed session no longer shows the connection's database in bold for a moment ([#25](https://github.com/sfaut/prisme/issues/25)).
- The fields and grids of a cell keep their left border, and the grip of the editor's line stays whole when the editor is folded.

### 🔒 Security

- The advisory [GHSA-vfj7-8cjw-p6xm](https://github.com/advisories/GHSA-vfj7-8cjw-p6xm) (`braces`), reported in v0.6.0, was withdrawn: `npm audit` reports no advisory.

### ⚙️ Internal improvements

- Dependencies:
  - `axum-extra` 0.12.6 added, to read the repeated keys of the query strings (console filters);
  - `@codemirror/commands` 6.11.1 and `@codemirror/search` 6.7.2 become direct dependencies, for the copy of CodeMirror's `basicSetup` without its fold gutter;
  - the constraints of `web/package.json` follow the installed versions, unchanged;
  - `rusqlite` stays at 0.40.2: `rusqlite_migration` 2.6.0 does not accept 0.41.0 yet.
- The interface is sized in lengths relative to the root, the grid's columns and rows included; the few APIs that only take pixels are listed and checked at each release.
- Every workaround of a dependency is marked in the code and listed in the developer guide, with the drafts of its upstream tickets.

### 📝 Documentation

- README: a section on the transactions, the switches of the temporary tables, and the oldest supported versions.
- Developer guide: the icons of the interface and their actions, the lengths in pixels and the workarounds.
- Design notes: `sqlparser` is kept, extended by Prisme for MariaDB, after weighing a parser of its own.
- Ideas for a graph of the database relations, and the prototype of an object visualizer.

## v0.6.0 (2026-10-23)

### 🚀 Launch

We are proud to announce Prisme v0.6.0. This version opens the door to writes: each kind of write (`INSERT`, `UPDATE`, `CREATE TABLE`...) has its own switch in the settings, all off by default, and **Exécuter tout** (Run all) runs the batches that write one query after another, in the order of the text. The object explorer writes the operations on an index, the indentation of the SQL editor becomes a setting that the beautifier follows, and the interface gets a more even look: editor and result tabs alike, row numbers that stay visible in the result grid, and a focus ring kept for the text fields.

### ✨ New features

- Writes ([#20](https://github.com/sfaut/prisme/issues/20)):
  - in the **Paramètres** (Settings) window, one switch per kind of write: data (`INSERT`, `UPDATE`, `DELETE`), tables (`CREATE`, `ALTER`, `RENAME`, `TRUNCATE`, `DROP`), views, functions, procedures and events. All are off by default, and greyed out while the sessions are read-only;
  - a write runs when each switch it needs is on: `REPLACE` needs `INSERT` and `DELETE`, `INSERT ... ON DUPLICATE KEY UPDATE` needs `INSERT` and `UPDATE`, `CREATE INDEX` and `DROP INDEX` need `ALTER TABLE`. A refusal names the missing switches;
  - the settings of the session (`SET @v`, `SET SESSION`, `SET NAMES`) run like a read, even in a read-only session; `SET TRANSACTION`, `autocommit` and the read-only variables stay refused;
  - **Exécuter tout** (Run all) runs its queries in parallel only when they all read. A batch that writes, sets the session or holds a query Prisme cannot parse runs one query after another, in the order of the text, on the editor's session: a temporary table or a variable is seen by the next queries. The first failure stops the batch, and the next queries say they were not run;
  - the stored procedures and functions, the events (`CREATE`, `ALTER`, `DROP EVENT`) and the index operations of MySQL and MariaDB (`ALTER INDEX`, `RENAME KEY`, `DROP KEY`) are now recognised, and no longer refused as queries Prisme cannot check.
- Object explorer: on the line of an index, a button (or Ctrl+click) appends after the last query the `ALTER TABLE` statements that act on it, commented out: add an index, change its visibility (`VISIBLE` / `INVISIBLE` in MySQL from 8.0, `NOT IGNORED` / `IGNORED` in MariaDB from 10.6), remove or rename it. A primary key can only be removed ([#21](https://github.com/sfaut/prisme/issues/21)).
- SQL editor: the indentation size (Tab, Shift+Tab, Backspace in the indentation, Ctrl+[ and Ctrl+]) is a setting of the **Paramètres** (Settings) window, from 1 to 8 spaces, 4 by default. **Enjoliver** (Beautify) indents by the same size.
- Console: the older lines are read by scrolling to the top of the list, which opens on the most recent lines; the duration column of the history is renamed **Exécution** (Execution) ([#22](https://github.com/sfaut/prisme/issues/22)).
- Result grid: the row numbers stay visible on the left when the grid scrolls sideways, set apart from the cells by a line.
- Look:
  - the editor tabs and the result tabs look and behave the same: the active tab is filled, the tabs are a little taller, their buttons side by side after the label;
  - the focus ring only shows on the text fields, thinner; buttons, lists, menus and tabs no longer show one.

### 💥 Breaking changes

- Writes: in a session that is not read-only, a write now runs only if its switches are on in the settings, all off by default. The writes no switch covers are refused: `CALL`, databases, triggers, accounts, server settings...

### 🐞 Bug fixes

- SQL editor: Backspace in the indentation and Ctrl+[ remove a whole tab stop, and no longer two of its four spaces.
- **Enjoliver** (Beautify) no longer indents by 4 spaces whatever the indentation setting.
- Console: no more blank space between the tabs and the list, left by the button that read the older lines once it had lost its label ([#22](https://github.com/sfaut/prisme/issues/22)).

### 🔒 Security

- `npm audit` still reports [GHSA-vfj7-8cjw-p6xm](https://github.com/advisories/GHSA-vfj7-8cjw-p6xm) (`braces`, high), which has no patched version. No impact on Prisme: `braces` only serves the linting of the interface's sources, during development, and never gets into the executable.

### ⚙️ Internal improvements

- Dependencies:
  - `ts-rs` 12.0.1 added to the `prisme-dialects` crate, with its `format` feature in the tests;
  - `@tanstack/vue-virtual` 3.13.47 → 3.13.48;
  - `@vitest/eslint-plugin` 1.6.27 → 1.6.28.
- The editor and result tabs share one component, and the focus ring is set by utility classes from the theme's variables.
- The interface tests run on four workers: the tests that mount CodeMirror no longer exceed their time.

### 📝 Documentation

- README: how **Exécuter tout** (Run all) runs its queries, in parallel or one after another.
- Export ideas: the three scopes of an export to a file, the full export following the column order of the grid.

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
