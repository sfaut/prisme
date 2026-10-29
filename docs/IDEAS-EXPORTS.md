# Copying and exporting results

This document gathers the ideas kept for copying the grid selection in several formats and for exporting a result to a file. Only the CSV for Excel in French is implemented, as a download (section 4.4) and as a full export (section 4.2). Settled points are marked **[decided]**, the others stay open (section 7).

## 1. Starting point

- Ctrl+C copies the grid selection as raw tab-separated text, with the column names on the first line (`selectionToText()` in `web/src/components/resultGrid.ts`): values as they are, control characters written literally (`\t`, `\n`…), `NULL` as an empty cell.
- Cells arrive as text (`string | null`), with the type of each column (`family`, native name).
- The server does not keep the rows: the interface only has the first N (default limit), the rest was never read.

## 2. Formats

Each format is a pure serialiser (columns, types, rows, options → text), tested separately. `selectionToText()` is limited to extracting the selection (covered rows and columns, in the displayed order) and handing it to the chosen serialiser.

| Format | NULL | Rules |
|---|---|---|
| Tabular | empty | Current behaviour, to paste into a spreadsheet **[decided]**. A `text/html` version is put in the clipboard at the same time (`ClipboardItem`), so that Excel or an email receives a real table. |
| Box-Unicode | to set | Table drawn with box characters (`┌─┬─┐`, `│`, `├─┼─┤`), in the spirit of the `mysql` client's output **[decided]**. Numbers, dates and times right-aligned, as in the grid. ASCII variant (`+---+`) possible. |
| CSV | empty, adjustable (`\N`, `NULL`) | RFC 4180. `,` or `;` separator (Excel in French). The variant for Excel in French is implemented (section 4.4). |
| TSV | `\N` | Strict TSV: `\t`, `\n` and `\\` escaped, like `LOAD DATA` and `COPY`, to load back into a database without loss. |
| JSON | `null` | Array of objects. |
| NDJSON | `null` | One object per line. |
| HTML | empty cell, `null` class | `<table>` with `<thead>`, escaped entities. |
| XML | `xsi:nil="true"` | Form of `mysql --xml`: `<row><field name="…">`, which accepts any column name. |

Common rules:

- **Numbers in JSON** **[decided]**: integer, floating and decimal columns are written as JSON numbers, copied as the server sent them (never converted by `Number`, hence without loss on BIGINT and DECIMAL when writing). To adjust if reading them back in JavaScript causes problems.
- **JSON columns** **[decided]**: a value recognised as JSON (`contentKind.ts`) is inserted as a nested object in JSON and NDJSON.
- **Duplicate column names** (`SELECT a.id, b.id`) **[decided]**: the export is refused, whatever the format, with an explicit error naming the columns involved. The risk ruled out is one column silently overwriting another.
- **Non-contiguous selection** (Ctrl+click): grid formats (tabular, Box-Unicode, CSV, TSV, HTML, images) leave the unselected cell empty; object formats (JSON, NDJSON, XML) omit its key. An unselected cell is thus never confused with a NULL.

### 2.1 Images: PNG, WebP, SVG

The table can also be exported as an image **[decided]**: PNG and WebP, plus SVG as a vector variant.

- **Rendering in the browser**: the table is drawn directly on a `canvas` (`fillText`, strokes), then `canvas.toBlob('image/png')` or `toBlob('image/webp')` produces the file. Both encodings are native, without dependencies.
- **Common layout**: a pure module computes the position of each cell (widths, right alignment of numbers, dates and times, displayed column order). The canvas and the SVG reuse this same computation; the SVG keeps selectable text and stays sharp at any scale.
- **Theme**: colors, font and spacing come from the theme's CSS variables (`web/src/assets/main.css`).
- **Sharpness**: dimensions multiplied by `devicePixelRatio`; drawing after `document.fonts.ready`, so that the theme's font is loaded.
- **DOM capture ruled out** (`html-to-image`): the grid is virtual, only the visible rows exist in the page; it would have to be rebuilt off screen.
- **Scope**: the selection or the displayed result. Browsers cap the canvas (about 16,000 pixels per side, 268 million pixels of area depending on the browser), that is a few hundred rows: beyond that, an explicit error states the cap. The full export by the server does not apply to images.
- **Clipboard**: it only accepts `image/png` (`ClipboardItem`). "Copy as image" therefore produces PNG; WebP and SVG go through the file export.

## 3. Copy interface

Prisme has not used the right click so far, and it is better to keep it that way. Leads:

- **Ctrl+C** copies in the default format (tabular at first). This default format joins the profile setting already planned for headers (`docs/QUESTIONS.md`, point 27).
- **Copy button** in the bottom bar of the grid, with a dropdown menu (Reka UI `DropdownMenu`): one format per entry, plus "without headers".
- **Ctrl+Shift+C** copies again in the last format chosen from this menu.

## 4. Export to a file

Three scopes **[decided]**:

- **Displayed result**, when the grid has no selection: the rows the grid holds, that is the N rows of Prisme's limit, or all the rows of a limit written by the user (rule 3 of `CLAUDE.md`). Everything happens in the browser, with the copy's serialisers, a `Blob` and a download. Immediate.
- **Selection**, when the grid has one: the selected cells only, in the displayed order, with the rules of a non-contiguous selection (section 2). Also in the browser.
- **Full result**: the server runs the query again without the `LIMIT` added by Prisme; a limit written by the user goes through as is. Only reads are accepted, with the AST classification already in place. This scope stays offered when the displayed result is not truncated: a large result then avoids going through the browser.

The presence of a selection chooses between the displayed result and the selection. The export dialog states the scope ("Sélection : 3 lignes × 2 colonnes"), so that a forgotten selection shows. Postponed alternative: the dialog always offers "Displayed result" and "Selection", the selection being only the default choice when there is one.

Export dialog, opened by a button of the bottom bar:

- scope (displayed result or selection, or full result with the warning "the query will be run again, without the sorting and filtering of the grid");
- format (those of section 2, images included except for the full result);
- options: headers, NULL, separator, UTF-8 BOM for Excel;
- proposed file name: `<tab>-YYYYMMDD-HHMMSS.<extension>`.

### 4.1 Grid state **[decided]**

The export of the displayed result and of the selection follows the grid's state: column order, sorting and filtering. It is direct, the view already exists.

The full result follows the column order of the grid: the export request carries it (indexes of the `SELECT`'s columns, in the displayed order), and the server reorders each row of the stream when writing it, without touching the query. Its rows are the raw result of the query: the user writes the sorting and filtering in SQL (`ORDER BY`, `WHERE`), and the export dialog says that those of the grid are not applied. Reproducing them on the server would give an order different from the grid's (server collation versus `compareCells()` comparison), which rule 2 of `CLAUDE.md` forbids doing silently, and would require a Rust reimplementation of the grid's filter.

### 4.2 Export manager **[decided]**

The full export runs as a background task, and an export manager tells when the file is ready to download.

- The full export is the entry "Résultat complet en CSV (requête relancée)" of the download menu, offered with or without a selection, whether the displayed result is truncated or not.
- It runs on a dedicated connection, read-only (section 7, question 1).
- The server reads the driver's stream and writes directly to a file of `<data-dir>/exports/`, with constant memory. The exports are kept in the internal database (`exports` table), so that their list survives a restart.
- The manager is a button of the top bar, with a progress dot while exports run, which opens the list of the exports (running, ready, failed, stopped, interrupted): rows written, size, duration, throughput, and the cause of a failure. Its head gives the room the files take.
- The completed share shows only when the total is known in advance, sent with the export request: the number of rows of a displayed result that the limit of Prisme did not cut. Otherwise, the total only comes with the last row: the MySQL / MariaDB protocol sends the columns, then the rows as they are produced, then an OK packet. A `COUNT(*)` alongside would give it in advance, at the price of a second execution of the query: not kept.
- A toast reports the end, with a "Télécharger" action: the file is downloaded through `GET /api/exports/{id}/file` (`Content-Disposition: attachment`, `Content-Length`, so that the browser shows an exact progress). A failure gets an error toast.
- Stopping an export closes its connection on the server (`KILL CONNECTION`): a `KILL QUERY` leaves the rows already on their way to be read, nearly a million rows through the ports of Docker Desktop. The partial file stays, and the manager offers to download it, as that of a failed or interrupted export.
- The folder is cleaned from the manager only, never automatically: "Retirer" stops a running export, deletes its file, then its row. An export left running by a stop of Prisme becomes "interrupted" at the next start, its partial file kept. Its counters are saved every half second while it runs: at the next start, the rows written after the last save are counted in its file, and its last write gives its end.
- The folder and the table are compared on each reading of the list. Only the files named by Prisme (`<id>-<milliseconds>.csv`) count. An export whose file is missing is listed as such, its download refused, its removal allowed. A file that no export references (copied into the folder, internal database replaced) is counted apart, with a removal of its own.
- This mechanism foreshadows the `Export` node of the federated engine (`docs/FEDERATED.md`, section 8).

### 4.3 Duplicate serialisers

The full export requires serialisers in Rust, whereas those of the copy are in TypeScript. For both to stay identical, common reference files (`tests/fixtures/export/`: an input in JSON, the expected output per format) are read by both test suites, in the spirit of the conformance suite.

### 4.4 CSV for Excel in French **[decided]**

The first format implemented, through the download button of the bar under the grid, without the export dialog. Its menu says what it downloads: "Sélection en CSV (3 lignes × 2 colonnes)", or "Résultat affiché en CSV (1 000 lignes)" without a selection.

- UTF-8 with a BOM, `;` as separator, CRLF line endings, the column names on the first line: Excel in French opens the file with a double click.
- RFC 4180 quoting: a field that holds `;`, `"`, CR or LF goes between quotes, its quotes doubled. Control characters stay raw, the quotes carrying them.
- `NULL` is an empty field, an empty string is `""`, as in PostgreSQL's `COPY ... CSV`: Excel shows both as an empty cell, a CSV reader tells them apart. An unselected cell is an empty field.
- Numbers of the `decimal` and `float` families take the decimal comma: with a dot, Excel in French reads `3.14` as text and `1.5` as the 1st of May. The other values go as the server sent them, dates included (`2026-10-12 09:05:00`, which Excel recognises).
- Excel's own limits are documented in the `README.md`, without a workaround, which would change the values: 1,048,576 rows at most, integers rounded beyond 15 digits, leading zeros of a code dropped, a value starting with `=`, `+`, `-` or `@` read as a formula (section 7, question 8).
- The file is built in the page by chunks of 2,000 rows, with a pause between two chunks: a large result of a `LIMIT` written by the user does not freeze the interface.
- Serialisers: `web/src/components/csv.ts` in the browser, `crates/server/src/exports/csv.rs` for the full export (section 4.2); reference files: `tests/fixtures/export/csv-excel-fr/`, read by both test suites (section 4.3).

## 5. Planned order

1. TypeScript serialisers and their tests.
2. Copy button, Ctrl+Shift+C and `text/html` deposit of the tabular format.
3. Export of the displayed result and of the selection.
4. Images: common layout, PNG and WebP through the canvas, SVG.
5. Rust serialisers and common reference files.
6. Export manager and full export.

## 6. Decision table

| Subject | Decision |
|---|---|
| Tabular and TSV | Two formats: tabular for spreadsheets, strict TSV to load back into a database. |
| Box-Unicode | Added to the list. |
| Images | PNG and WebP through the browser's canvas, SVG as a vector variant; PNG only in the clipboard. |
| Numbers in JSON | JSON numbers, to adjust if needed. |
| JSON columns | Nested objects. |
| Duplicate column names | Export refused, whatever the format. |
| Export scopes | Displayed result or selection, depending on the presence of a selection, in the browser; full result run again by the server, offered even when the result is not truncated. |
| Full export | Dedicated connection, read-only; followed by the export manager of the top bar, a toast when the file is ready; completed share only with a total known in advance, without `COUNT(*)`. |
| Exports folder | Cleaned from the manager only, never automatically; a missing file and a file no export references are both shown. |
| Formulas in a CSV | Values written as they are, without a prefix. |
| Grid state | The export of the displayed result and of the selection follows the column order, sorting and filtering; the full result follows the column order, its rows being the raw result of the query. |
| Right click | To avoid. |

## 7. Open questions

1. **Connection of the full export** **[decided]**: a dedicated connection, read-only. It leaves the session free, but sees neither temporary tables, nor session variables, nor a running transaction; the `README.md` says so. Running in the session would see all of that, but keep it busy during the export.
2. **NULL in Box-Unicode**: `NULL` shown as is is confused with the string `'NULL'`; a symbol (`∅`) or a distinct style is to be chosen.
3. **Width in Box-Unicode**: alignment requires the display width of characters (East Asian wide characters, emojis), different from their count.
4. **Duplicate names and copying**: does the refusal also apply to copying to the clipboard, including tabular, where overwriting does not exist?
5. **Invalid JSON column**: does a value of a JSON column that does not parse stay a string (the type then changes from one row to another), or does the export fail?
6. **Clipboard volume**: should copying be capped (number of cells), with the export offered beyond?
7. **Theme of the images**: the screen's, or always the light theme, more readable in a printed document or an email? A choice in the export dialog is possible.
8. **Formulas in a CSV** **[decided]**: values are written as they are. Excel reads a field that starts with `=`, `+`, `-` or `@` as a formula ("CSV injection"); prefixing such a value with `'` would protect the reader, but would change the value, and `-3` is a number. The `README.md` says so.
