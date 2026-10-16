# Copying and exporting results

This document gathers the ideas kept for copying the grid selection in several formats and for exporting a result to a file. Nothing is implemented. Settled points are marked **[decided]**, the others stay open (section 7).

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
| CSV | empty, adjustable (`\N`, `NULL`) | RFC 4180. `,` or `;` separator (Excel in French). |
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
- **Scope**: the selection or the loaded rows. Browsers cap the canvas (about 16,000 pixels per side, 268 million pixels of area depending on the browser), that is a few hundred rows: beyond that, an explicit error states the cap. The full export by the server does not apply to images.
- **Clipboard**: it only accepts `image/png` (`ClipboardItem`). "Copy as image" therefore produces PNG; WebP and SVG go through the file export.

## 3. Copy interface

Prisme has not used the right click so far, and it is better to keep it that way. Leads:

- **Ctrl+C** copies in the default format (tabular at first). This default format joins the profile setting already planned for headers (`docs/QUESTIONS.md`, point 27).
- **Copy button** in the bottom bar of the grid, with a dropdown menu (Reka UI `DropdownMenu`): one format per entry, plus "without headers".
- **Ctrl+Shift+C** copies again in the last format chosen from this menu.

## 4. Export to a file

Two scopes:

- **Loaded rows**: everything happens in the browser, with the copy's serialisers, a `Blob` and a download. Immediate.
- **Full result**: the server runs the query again without the `LIMIT` added by Prisme; a limit written by the user goes through as is (rule 3 of `CLAUDE.md`). Only reads are accepted, with the AST classification already in place.

Export dialog, opened by a button of the bottom bar:

- scope (loaded rows, or full result with the warning "the query will be run again");
- format (those of section 2, images included for the loaded rows);
- options: headers, NULL, separator, UTF-8 BOM for Excel;
- proposed file name: `<tab>-YYYYMMDD-HHMMSS.<extension>`.

### 4.1 Grid state **[decided]**

The export follows the grid's state: column order, sorting and filtering. For the loaded rows, it is direct (the view already exists). For the full result, the server must reproduce the same view on all the rows (see section 7, question 2).

### 4.2 Export manager

The full export runs as a background task, and an export manager tells when the file is ready to download. Ideas:

- the server reads the driver's stream and writes directly to a temporary file of the data folder, with constant memory;
- the manager lists the exports (running, ready, failed, cancelled), with the number of rows written and the size;
- a notification reports the end, then the file is downloaded through `GET /api/exports/{id}` (`Content-Disposition: attachment`);
- cancelling an export stops the query (`KILL QUERY`, like `cancel`) and deletes the partial file;
- ready files are deleted after download or after a delay;
- this mechanism foreshadows the `Export` node of the federated engine (`docs/FEDERATED.md`, section 8).

### 4.3 Duplicate serialisers

The full export requires serialisers in Rust, whereas those of the copy are in TypeScript. For both to stay identical, common reference files (`tests/fixtures/export/`: an input in JSON, the expected output per format) are read by both test suites, in the spirit of the conformance suite.

## 5. Planned order

1. TypeScript serialisers and their tests.
2. Copy button, Ctrl+Shift+C and `text/html` deposit of the tabular format.
3. Export of the loaded rows.
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
| Grid state | The export follows the column order, sorting and filtering. |
| Right click | To avoid. |

## 7. Open questions

1. **Connection of the full export**: a dedicated connection leaves the session free, but sees neither temporary tables, nor session variables, nor a future running transaction. Running in the session sees all of that, but keeps it busy during the export. The export manager leans towards the dedicated connection; to think about.
2. **Sorting and filtering the full result**: sorting can go into the SQL (`SELECT * FROM (…) AS t ORDER BY …`), at the risk of an order different from the grid's (server collation versus `compareCells()` comparison), which rule 2 of `CLAUDE.md` forbids doing silently. Filtering can be applied on the server side on the stream, with a Rust reimplementation of the grid's filter. Sorting on the server outside the database requires keeping everything in memory or on disk.
3. **NULL in Box-Unicode**: `NULL` shown as is is confused with the string `'NULL'`; a symbol (`∅`) or a distinct style is to be chosen.
4. **Width in Box-Unicode**: alignment requires the display width of characters (East Asian wide characters, emojis), different from their count.
5. **Duplicate names and copying**: does the refusal also apply to copying to the clipboard, including tabular, where overwriting does not exist?
6. **Invalid JSON column**: does a value of a JSON column that does not parse stay a string (the type then changes from one row to another), or does the export fail?
7. **Clipboard volume**: should copying be capped (number of cells), with the export offered beyond?
8. **Theme of the images**: the screen's, or always the light theme, more readable in a printed document or an email? A choice in the export dialog is possible.
