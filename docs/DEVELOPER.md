# Prisme: developer guide

[![Rust](https://img.shields.io/badge/dynamic/toml?url=https%3A%2F%2Fraw.githubusercontent.com%2Fsfaut%2Fprisme%2Fmaster%2Frust-toolchain.toml&query=%24.toolchain.channel&label=rust)](../rust-toolchain.toml)
[![vue](https://img.shields.io/github/package-json/dependency-version/sfaut/prisme/vue?filename=web%2Fpackage.json&label=vue)](../web/package.json)
[![typescript](https://img.shields.io/github/package-json/dependency-version/sfaut/prisme/dev/typescript?filename=web%2Fpackage.json&label=typescript)](../web/package.json)
[![axum](https://img.shields.io/badge/dynamic/toml?url=https%3A%2F%2Fraw.githubusercontent.com%2Fsfaut%2Fprisme%2Fmaster%2Fcrates%2Fserver%2FCargo.toml&query=%24.dependencies.axum.version&label=axum)](../crates/server/Cargo.toml)
[![tokio](https://img.shields.io/badge/dynamic/toml?url=https%3A%2F%2Fraw.githubusercontent.com%2Fsfaut%2Fprisme%2Fmaster%2Fcrates%2Fserver%2FCargo.toml&query=%24.dependencies.tokio.version&label=tokio)](../crates/server/Cargo.toml)
[![sqlparser](https://img.shields.io/badge/dynamic/toml?url=https%3A%2F%2Fraw.githubusercontent.com%2Fsfaut%2Fprisme%2Fmaster%2Fcrates%2Fdialects%2FCargo.toml&query=%24.dependencies.sqlparser.version&label=sqlparser)](../crates/dialects/Cargo.toml)
[![mysql_async](https://img.shields.io/badge/dynamic/toml?url=https%3A%2F%2Fraw.githubusercontent.com%2Fsfaut%2Fprisme%2Fmaster%2Fcrates%2Fconnectors%2FCargo.toml&query=%24.dependencies.mysql_async&label=mysql_async)](../crates/connectors/Cargo.toml)
[![rusqlite](https://img.shields.io/badge/dynamic/toml?url=https%3A%2F%2Fraw.githubusercontent.com%2Fsfaut%2Fprisme%2Fmaster%2Fcrates%2Fserver%2FCargo.toml&query=%24.dependencies.rusqlite.version&label=rusqlite)](../crates/server/Cargo.toml)
[![vite](https://img.shields.io/github/package-json/dependency-version/sfaut/prisme/dev/vite?filename=web%2Fpackage.json&label=vite)](../web/package.json)
[![tailwindcss](https://img.shields.io/github/package-json/dependency-version/sfaut/prisme/tailwindcss?filename=web%2Fpackage.json&label=tailwindcss)](../web/package.json)
[![reka-ui](https://img.shields.io/github/package-json/dependency-version/sfaut/prisme/reka-ui?filename=web%2Fpackage.json&label=reka-ui)](../web/package.json)
[![codemirror](https://img.shields.io/github/package-json/dependency-version/sfaut/prisme/codemirror?filename=web%2Fpackage.json&label=codemirror)](../web/package.json)
[![pinia](https://img.shields.io/github/package-json/dependency-version/sfaut/prisme/pinia?filename=web%2Fpackage.json&label=pinia)](../web/package.json)
[![lucide](https://img.shields.io/github/package-json/dependency-version/sfaut/prisme/@lucide/vue?filename=web%2Fpackage.json&label=lucide)](../web/package.json)

How to build Prisme from source, run it in development, test it, and how it is delivered. For using Prisme, see the [README](../README.md).

The backend is written in Rust (`tokio`, `axum`), the interface in Vue 3 and TypeScript (Vite). Both are delivered as a single executable that embeds the compiled interface. Design decisions are in the [other documents](#design-documents) of this folder.

## Prerequisites

| Tool | Role | Windows installation |
|---|---|---|
| Rust (through `rustup`) | Backend build. The exact version is pinned by `rust-toolchain.toml` and installed on the first `cargo` command. | `winget install Rustlang.Rustup` |
| Visual Studio Build Tools (C++) | MSVC linker, and build of the embedded SQLite. | `winget install Microsoft.VisualStudio.2022.BuildTools`, with the "Desktop development with C++" workload |
| Node.js and npm | Vue interface. | `winget install OpenJS.NodeJS` |
| Docker Desktop | Real MySQL and MariaDB servers for the integration tests. The engine must be running. | `winget install Docker.DockerDesktop` |

## Running from source

From the repository root:

```
cargo run
```

Prisme then serves the API and the compiled interface on <http://127.0.0.1:8484> (or the port chosen on first launch, see the [README](../README.md#first-launch)). If the interface has never been compiled, the home page explains how to compile it.

- Options go after `--`, for example `cargo run -- --data-dir ./tmp` for a throwaway data folder (ignored by Git). `cargo run -- --help` lists them.
- Use of the API: `cargo run -- --verbose` writes each request (method, path, status, duration) and each message of the query sessions, without their content (`crates/server/src/api_log.rs`).
- More detailed logs: set `RUST_LOG` first, `$env:RUST_LOG="debug"` in PowerShell.
- `./rebuild.ps1` stops the running Prisme, compiles the interface (`npm run build`) and the backend (`cargo build`), then starts Prisme again in the terminal. `-Release` uses the release build, `-Force` also stops the other Prisme executables (a downloaded release, `prisme-v0.3.0-windows-x86_64.exe`), `-Verbose` starts Prisme with `--verbose`; Prisme's other options go after `--` (`./rebuild.ps1 -- --data-dir ./tmp`).

## Working on the interface

There are two ways to see the interface, at two different addresses:

| Address | Serves | Updated |
|---|---|---|
| <http://localhost:5173> (`npm run dev`) | the sources, live, through Vite, which forwards the API to Prisme | on every save, without reloading |
| <http://127.0.0.1:8484> (`cargo run`) | the compiled interface, `web/dist` | only after `npm run build`, then a page reload |

For interface work, use Vite, with two terminals at the repository root:

```
cargo run
```

```
cd web
npm install          # the first time, or after a dependency change
npm run dev
```

Then open <http://localhost:5173>. The page on port 8484 keeps showing the last `npm run build` until the next one.

### Icons and actions

The icons come from Lucide (`@lucide/vue`). An action keeps the same icon wherever it appears, and an icon keeps the same meaning: before adding a button, look for its action below, and add any new icon to these tables in the same change.

Rules:

- **Size**: `size-icon` (16 px) for the buttons, the messages and the lines of the navigator, `size-icon-sm` (12 px) for the chevrons of the tree and the marks of the grid's headers, never a size of the spacing scale (`size-4`). The key marks of the navigator's columns are sized in `em` (`size-[1em]`), like the slots that align them.
- **Icon-only button**: `UiButton` with `variant="ghost"` and `small` in a bar, an `aria-label` and a `v-tooltip`. The tooltip says the action, and may add what it does after a colon ("Rafraîchir : relance la requête").
- **Toggle button**: `aria-pressed`. The accent color (`tone="accent"`) means the state is in effect: a filter keeps only some elements, a tab is pinned. The raised background (`bg-surface-elevated`) means the button shows its panel or form (the console's filters, the grid's filter).
- **Place**: the actions of a bar stand at its right end, side by side, then the fold button, last; for the panel on the right, the fold button comes first, left of the title. The rail of a folded panel shows its unfold icon at the same place.
- **Folded panel**: it keeps only its tabs, if any, and its unfold button; its other actions hide.
- **Running action**: the button that starts a long action turns into its stop button while it runs, with the stop icon and the error color for queries.

#### Panels and folding

| Icon | Action | Where |
|---|---|---|
| `PanelLeftClose` / `PanelLeftOpen` | Fold / unfold the object navigator | its header / its rail (`PanelHeader`, `PanelRail`) |
| `PanelRightClose` / `PanelRightOpen` | Fold / unfold the data explorer | its header / its rail |
| `PanelBottomClose` / `PanelBottomOpen` | Fold / unfold the console; fold / unfold the results | console's header / its rail; bar of the result tabs (`QueryCell`) |
| `PanelTopClose` / `PanelTopOpen` | Fold / unfold the active editor, with its buttons | editors' bar, right of the reopen button (`EditorTabBar`, sent by the `QueryCell`) |
| `ChevronRight` / `ChevronDown` | Open / close a node of the tree | object navigator (`NavigatorLine`) |
| `ChevronsDownUp` / `ChevronsUpDown` | Fold / unfold the whole document | data explorer |
| `FoldVertical` / `UnfoldVertical` | Fold / unfold the cursor's level | data explorer |

#### Queries and results

| Icon | Action | Where |
|---|---|---|
| `Play` | Run the query under the cursor ("Exécuter") | editor's buttons |
| `ScanText` | Run the selected text only ("Exécuter précisément") | editor's buttons |
| `FastForward` | Run all the queries ("Exécuter tout") | editor's buttons |
| `Square` | Stop the running queries (the run button, while they run) | editor's buttons |
| `WandSparkles` | Rewrite as "beautiful SQL" ("Enjoliver") | editor's buttons |
| `RefreshCw` / `RefreshCwOff` | Run the result's query again / stop that refresh | bar of the result tabs (`QueryTabView`) |
| `Funnel` | Show / hide the filter of a list; in the data explorer, keep only the search results | bar of the result tabs (`ResultGrid`), console's header, data explorer's search |
| `Search` | Mark of a search or filter field; open the search | grid's filter field; data explorer |
| `ChevronUp` / `ChevronDown` | Previous / next search result | data explorer |
| `Pin` | Pin / unpin a result tab | result tab (`ResultTab`) |
| `Copy` | Copy the document shown | data explorer |
| `TextWrap` | Wrap the long lines | data explorer |
| `ExternalLink` | Open a web address of a cell in a new tab | grid's cells |
| `MessageCircleOff` | Clear the list of the console's tab | console's header |

#### Editors and tabs

| Icon | Action | Where |
|---|---|---|
| `Plus` | New editor; add a connection | editors' bar (`EditorTabBar`); navigator's menu |
| `History` | Reopen a closed editor | editors' bar |
| `X` | Close a tab, a dialog or a message; empty a field | tabs (`DocumentTab`), `UiDialog`, `UiAlert`, grid's filter field |

#### Object navigator

| Icon | Meaning or action | Where |
|---|---|---|
| `Server` | Root of the data sources | tree |
| `Cable` | A data source (connection) | tree |
| `Database` | A database | tree; also the empty state of an editor (`App.vue`) |
| `Notebook` | Root of the notebooks | tree |
| `NotebookPen` | Create a notebook (not available yet) | notebooks' menu |
| `Folder` / `FolderOpen` | A closed / open folder (tables, views...) | tree |
| `Table` | A table | tree |
| `Eye` | A view | tree |
| `Rows3` | Show the content of a table or a view (`SELECT`) | line's button |
| `ScrollText` | A procedure | tree |
| `SquareFunction` | A function | tree |
| `CalendarClock` | An event | tree |
| `Columns3` | A column | tree |
| `ListOrdered` | An index | tree |
| `Link` | A foreign key; column of a foreign key | tree; column's line |
| `Zap` | A trigger | tree |
| `KeyRound` | Column of the primary key | column's line |
| `DatabaseZap` | Use this database (`USE`) | line's button |
| `PlugZap` | Test a connection | source's menu |
| `Unplug` | Close the session ("Déconnecter") | line's button; source's menu |
| `PencilRuler` | Write the operations on an index | line's button |
| `Pencil` | Edit a connection | source's menu |
| `RefreshCw` | Reload the catalog of a connection, of all of them | source's menu; sources' menu |
| `Trash2` | Delete a connection | source's menu |
| `EllipsisVertical` | Menu of a line | line's button |

#### Types of the grid's columns (`typeIcons.ts`)

| Mark | Type family or content |
|---|---|
| `123` (text) | integer |
| `.00` (text) | decimal |
| `Hash` | float |
| `CaseSensitive` | text |
| `Binary` | binary |
| `Calendar` / `Clock` / `CalendarClock` / `CalendarDays` | date / time / date and time / year |
| `Braces` | JSON, as a type or as the content of a text column |
| `CodeXml` | XML content of a text column |
| `Link` | web addresses in a text column |
| `ToggleRight` | bit |
| `List` / `ListChecks` | enum / set |
| `Shapes` | geometry |
| `CircleDashed` | other types |

#### States and messages

| Icon | Meaning | Where |
|---|---|---|
| `LoaderCircle` (spinning) | Loading, action in progress | `UiButton` (`loading`), results, tree |
| `Database`, `DatabaseArrowDown`, `DatabaseArrowUp`, `DatabaseCheck`, `DatabaseX` | Exchanges with the internal database: none, reading, writing, synced, failed | top bar (`SyncStatus`) |
| `Info`, `CircleCheckBig`, `CircleAlert`, `CircleX` | Message: information, success, warning, error | `UiAlert` |
| `CircleCheckBig`, `CircleX` | Transient message: success, error, as in `UiAlert` | `UiToaster` |
| `Eye` / `EyeOff` | Show / hide the password | connection dialog |
| `ChevronDown`, `Check` | Open a list; chosen item | `UiSelect` |
| `Monitor`, `Sun`, `Moon` | Color mode: system, light, dark | top bar |
| `User`, `Settings` | Profile, settings | account menu |

### Lengths in pixels

The interface is built in lengths relative to the root, `:root` being fixed: multiples of `--space-unit`, in `rem`, and `ch` or `em` of a font (rule 10 of [CLAUDE.md](../CLAUDE.md)). Only the strokes of `:root` stay in pixels. A few APIs of dependencies accept nothing but a number of pixels, though. They receive a theme variable converted by `cssPixels` (`web/src/cssLength.ts`): the theme stays the only source of the length, read once, on loading. As `:root` is fixed, and the zoom of the browser does not change the ratio of a `rem` to a CSS pixel, the value stays exact for the whole session; only a change of the browser's default font size, with the page open, needs a reload.

| Dependency | API in pixels | Variables | Where |
|---|---|---|---|
| `reka-ui` 2.11.0 | `SplitterPanel`: `default-size`, `min-size`, `max-size`, `collapsed-size`, with `size-unit="px"`. The other unit, `%`, is relative to the group, not to the root: a theme length in `%` would have to be measured in pixels and computed again on each resize of the window. | `--navigator-width`, `--explorer-width`, `--editors-min-width`, `--console-height`, `--rail-size`, `--editor-height`, `--results-height`… | `App.vue`, `QueryCell.vue` |
| `reka-ui` 2.11.0 | `side-offset` of the menus and lists (a number, passed to Floating UI). | `--popup-offset` | `UiSelect.vue`, `UiDropdownMenu.vue` |
| `reka-ui` 2.11.0 | `estimate-size` of `TreeVirtualizer` (built on `@tanstack/vue-virtual`): height of a line of the tree, to turn the scroll position (in pixels) into the lines to draw. | `--row-height` | `ObjectNavigator.vue` |
| `@tanstack/vue-virtual` 3.13 | `estimateSize`: height of a row, to turn the scroll position (in pixels) into the rows to draw. The page itself places the rows of the result grid in CSS (`calc(var(--grid-row-height) * n)`). | `--grid-row-height` | `ResultGrid.vue` |

The pointer also gives pixels: a column of the grid resized with the mouse converts its width into `rem` once, at the time of the drag (`resizedWidth` in `resultGrid.ts`).

In the code, each of these places carries a comment that starts with "Fixed size" and refers to this section.

These are workarounds too (see "Workarounds"). Before each release, after the update of the dependencies, each API is checked again (process in [CLAUDE.md](../CLAUDE.md), section "Publishing a version"): once one accepts a CSS length or a relative unit, the theme variable goes to it directly, and its `cssPixels` goes away, with its row here.

## Building the single executable

Compile the interface, then Prisme:

```
cd web
npm run build        # writes the compiled interface to web/dist
cd ..
cargo build --release
```

`target/release/prisme.exe` then contains `web/dist` and runs on its own, without Node.js. On Windows, the C runtime is linked statically (`.cargo/config.toml`): the executable does not need the Visual C++ Redistributable.

## Tests

### Backend

```
cargo test                           # all fast tests, without Docker
cargo test -p prisme-server          # the tests of one crate
cargo test connections               # the tests whose name contains "connections"
cargo test -- --include-ignored      # also the tests on real MySQL and MariaDB servers
cargo clippy --all-targets           # static analysis, must stay free of warnings
cargo fmt                            # formatting
```

- **Unit tests** sit at the bottom of each `.rs` file, in a `#[cfg(test)] mod tests` block. They also cover the API: HTTP requests go to the application in memory, on an in-memory internal database, without opening a port.
- **Integration tests** sit in the `tests/` folder of each crate. Those of the MySQL / MariaDB connector start real servers in Docker. They are ignored by default: start Docker Desktop, then add `-- --include-ignored`. The first run downloads the server images.

`cargo test` also regenerates the TypeScript types of `web/src/api/generated/` from the Rust structures (`ts-rs`). After changing a structure exchanged with the interface, run `cargo test`, then `npm run type-check`, and commit the generated types with the Rust change.

### Interface

In `web/`:

```
npm run test:unit -- --run           # Vitest tests (without --run: run again on every change)
npm run type-check                   # type checking, templates included (vue-tsc)
npm run lint                         # Oxlint and ESLint
npm run format                       # formatting (Prettier)
```

## Testing the API by hand

Start Prisme on a throwaway data folder, to delete to start from scratch:

```
cargo run -- --data-dir ./tmp
```

Then, in a second PowerShell terminal:

```powershell
$api = "http://127.0.0.1:8484/api"

# Is the server up?
Invoke-RestMethod "$api/health"

# Save a connection
$connection = @{
    name = "local"; host = "127.0.0.1"; port = 3306
    username = "root"; password = "password"; database_name = ""
} | ConvertTo-Json
Invoke-RestMethod "$api/connections" -Method Post -ContentType "application/json" -Body $connection

# List the connections (the password never appears)
Invoke-RestMethod "$api/connections"

# Connect: returns the flavor (mysql or mariadb) and the server version
Invoke-RestMethod "$api/connections/1/probe" -Method Post

# Settings: read, then change (row limit 1000, 2000, 5000, 10000 or 50000; retentions 1 to 3650 days; indentation 1 to 8 spaces)
Invoke-RestMethod "$api/settings"
Invoke-RestMethod "$api/settings" -Method Put -ContentType "application/json" -Body '{"row_limit": 5000, "read_only_session": false, "allowed_writes": ["insert", "update"], "history_retention_days": 30, "debug_retention_days": 30, "indent_size": 4}'
```

On error (status 4xx or 5xx), PowerShell raises an exception, and the JSON body holds Prisme's message in English and its detail: `{"error": "...", "detail": {"code": "...", ...}}`. The interface writes its own message from the detail (`web/src/api/messages.ts`); the texts written outside Prisme (the target server's answer, the driver's message) stay in their own fields, quoted as they are.

| Route | Role | Errors |
|---|---|---|
| `GET /api/health` | Answers `ok`. | — |
| `GET /api/version` | Version of the executable: `{"version": "0.2.0"}`. The interface shows it next to the name. | — |
| `GET /api/connections` | Lists the connections, without passwords. | — |
| `POST /api/connections` | Saves a connection, returns `{"id": ...}` (201). | 400 invalid input, 409 name already taken |
| `POST /api/connections/{id}/probe` | Connects, returns `{"flavor": ..., "version": ...}`. | 404 unknown connection, 502 server unreachable or access denied, 504 no answer within 10 s |
| `GET /api/settings` | Settings: `{"row_limit": 1000, "row_limit_choices": [...], "read_only_session": true, "allowed_writes": [], "history_retention_days": 30, "debug_retention_days": 30, "indent_size": 4}`. `allowed_writes`: the kinds of write allowed (`insert`, `create_table`...), in the order of `WriteKind::ALL`. | — |
| `PUT /api/settings` | Changes the default limit, the read-only sessions, the allowed writes, the retention of the console (1 to 3,650 days) and the indentation of the SQL editor (1 to 8 spaces), returns the updated settings. | 400 limit not among the choices, retention or indentation out of bounds; 422 unknown kind of write |
| `GET /api/profile` | Profile: `{"email": "...", "first_name": "", "last_name": ""}`. | — |
| `PUT /api/profile` | Changes the names (trimmed, 100 characters at most), returns the profile. | 400 name too long |
| `GET /api/console/history` | Page of the history of the queries, in chronological order: `?before=` (id of the oldest query already read), `?limit=` (500 by default, 2,000 at most), `?action=` (an action kept, the key repeated for several: `?action=run&action=run_all`; all when absent). | 400 unknown action |
| `DELETE /api/console/history` | Clears the history of the user (204). | — |
| `GET /api/console/debug` | Page of the debug log (lines of the server's log, notifications of the interface): `?before=` and `?limit=` as for the history, `?level=` and `?source=` (a level or a source kept, the key repeated for several: `?level=error&level=warn&source=interface`; all when absent). | 400 unknown level or source |
| `DELETE /api/console/debug` | Clears the debug log (204). | — |
| `POST /api/console/notifications` | Adds a notification of the interface to the debug log: `{"level": "info", "message": "..."}` (204). | 400 invalid level |
| `GET /api/console/stream` | Server-sent events: `history` with each new query, `debug` with each new line of the debug log, `history-cleared` / `debug-cleared` when a list is cleared, `lagged` when events were dropped for a slow client. | — |

### Running queries over WebSocket

Queries go through a WebSocket, one session per saved connection: `ws://127.0.0.1:8484/api/connections/{id}/query`. The simplest way to try it is `wscat`, through Node.js:

```
npx wscat -c ws://127.0.0.1:8484/api/connections/1/query
```

Prisme answers `{"type":"ready",...}` once connected to the target server. Then each line typed is a JSON message:

- `run` runs `sql`, one or more queries. With `cursor` (a position in the text), only the query under the cursor runs. Without it, the queries touched by the passages of `selection` (`start` and `end` positions, `end` excluded) run in parallel; without either, all of them. With `exact` set to `true`, only the selected text runs: of each query the passages touch, the part inside them; without a selection, nothing runs. `row_limit` overrides the default limit for this run. In the session of an editor, `tab` refreshes a saved result tab: `sql` is its query, run alone, and the tab is kept; only a complete result replaces the saved one.
- `cancel` stops every running query; `cancel_query` stops one, given its number in the batch.

```
{"type": "run", "sql": "SELECT * FROM information_schema.COLUMNS", "cursor": 0}
{"type": "run", "sql": "SELECT * FROM information_schema.COLUMNS", "cursor": 0, "row_limit": 2000}
{"type": "run", "sql": "SELECT 1; SELECT 2"}
{"type": "run", "sql": "SELECT 1; SELECT 2; SELECT 3", "selection": [{"start": 11, "end": 18}]}
{"type": "run", "sql": "SELECT 1 FROM DUAL WHERE 1 = 0", "selection": [{"start": 0, "end": 18}], "exact": true}
{"type": "cancel"}
{"type": "cancel_query", "query": 1}
```

Prisme first announces the queries of the batch (`batch`, each one numbered), then, for each query, messages carrying its number (`query`), in this order:

1. `started`: the query starts (in parallel, it may wait for its turn).
2. `columns`: column names and types, columns right-aligned by the grid (numbers, dates and times), the text actually run, how the limit was handled (`added` if Prisme added `LIMIT N + 1`, `user_limit` if the query had one, `not_applicable` for `SHOW` or `EXPLAIN`), the maximum row count (`expected_rows`, `null` if unknown), the execution time measured by the server (`execution_us`, in microseconds) and the end of the execution (`executed_at`, in milliseconds since the Unix epoch).
3. `rows`: the rows, a first batch of 50 that shows without waiting for a full one, then batches of 500, each cell as text or `null`.
4. `done`: row count, `truncated` (rows beyond the limit were dropped), affected rows, duration.

On failure or cancellation, an `error` message replaces the rest of that query, and the session goes on. `batch_done` ends the batch. In parallel, messages of different queries interleave.

Only reads pass: `SELECT`, `SHOW`, `DESCRIBE`, `EXPLAIN` and `USE`. Writes (`UPDATE`, `DELETE`, `DROP`, `CALL`...) are refused until the write switches exist, as are queries Prisme cannot analyse.

### Throwaway database servers

To get servers without touching your usual databases (Docker Desktop running):

```
docker run --rm -d --name prisme-mariadb -p 3307:3306 -e MARIADB_ROOT_PASSWORD=secret mariadb:11
docker run --rm -d --name prisme-mysql   -p 3308:3306 -e MYSQL_ROOT_PASSWORD=secret   mysql:8
```

Then save a connection to `127.0.0.1`, port `3307` or `3308`, user `root`, password `secret`. `docker stop prisme-mariadb prisme-mysql` removes them.

To look at the internal database, open `./tmp/prisme.db` with any SQLite client (DBeaver, DB Browser for SQLite, `sqlite3`). The `password_secret` column only holds encrypted bytes.

## SQL keywords

Syntax highlighting and the beautifier share the keywords of each dialect, one file per dialect, edited directly: `crates/dialects/keywords/mysql.yaml` and `mariadb.yaml`. They list reserved words, non-reserved keywords, types and values, with, when needed, the version a word appeared in (`since`). The editor only highlights the words of the server's version. The beautifier rules are described in section 2.9 of [V1.md](V1.md).

## Continuous integration and releases

GitHub Actions (`.github/workflows/ci.yaml`) checks every push to `master` and every pull request, in about two minutes:

- interface: formatting, lint, types and tests;
- backend: formatting, `clippy` and all tests, Docker ones included.

A change that only touches documentation (`*.md`, `docs/`, `.claude/`, `LICENSE`) runs nothing. The Windows, Linux and macOS executables are only built for a release, or when the workflow is run by hand ("Actions" tab, "CI", "Run workflow"): they are then downloadable from the run's artifacts (GitHub sign-in required, kept 90 days).

A version is published through a GitHub release, whose changelog is written by hand (full process in [CLAUDE.md](../CLAUDE.md), section "Publishing a version"). The versions in `Cargo.toml` and `web/package.json` must already match the tag, and the target commit must be on GitHub:

```
git tag --annotate v0.1.0 --message "Prisme v0.1.0" origin/master
git push origin v0.1.0
gh release create v0.1.0 --verify-tag --title v0.1.0 --notes-file changelog.md
```

The published release runs the CI again on that commit. It checks the versions, then attaches `prisme-v0.1.0-windows-x86_64.exe`, `prisme-v0.1.0-linux-x86_64` and `prisme-v0.1.0-macos-universal` to the release, about twenty minutes after publication. Past changelogs are in [CHANGELOG.md](CHANGELOG.md).

## Pinned versions

Most dependencies accept any compatible version (`^` in `web/package.json`, the default in `Cargo.toml`). The ones below are fixed on purpose, to an exact version or to a minor version (`~`). They are not left behind: the update of the dependencies before each release moves them too, deliberately, under all the checks (see "Held-back dependencies").

| Dependency | Constraint | Reason |
|---|---|---|
| Rust (`rust-toolchain.toml`) | `1.99.1` | Every machine and the CI build with the same compiler, `clippy` and `rustfmt`: each Rust version brings new `clippy` lints, which would otherwise break the "no warning" rule on one machine and not on another. |
| `sqlparser` | `=0.63.0` | `crates/dialects/src/mysql_parser.rs` takes over methods of `MySqlDialect` written for this version, to parse the compound time units of MySQL and MariaDB (`YEAR_MONTH`...). An upgrade compares them first (see its module comment and [V1.md](V1.md), section 3.2). |
| `json-p3` | `2.3.1` | 2.3.2 is published without its TypeScript declarations, and 3.x requires TypeScript 7 (see "Held-back dependencies"). |
| `prettier` | `3.9.10` | Exact version, as Prettier recommends: a new version, even a patch, can format differently, and the CI checks the formatting. |
| `typescript` | `~6.0.0` | TypeScript does not follow semantic versioning: a minor version can add type errors. `typescript-eslint` also requires a version below 6.1. |
| `oxlint`, `eslint-plugin-oxlint` | `~1.89.0` | Both are released together and must keep the same version: the plugin turns off in ESLint the rules that Oxlint checks. A minor version adds rules. |
| `eslint-plugin-vue` | `~10.11.0` | A minor version can report more errors (new rules, rule fixes, as ESLint's versioning policy allows). |
| `@types/node` | `^24` | The types follow Node 24, the version of the TypeScript configuration (`@tsconfig/node24`) and of `engines` (`web/package.json`): the types of Node 26 would offer APIs that Node 24 lacks. |

The constraints with `~` and the exact Prettier version come from the Vue project template (`create-vue`).

## Held-back dependencies

Before each release, the dependencies go to their latest version, major versions included (process in [CLAUDE.md](../CLAUDE.md), section "Publishing a version"). A dependency whose upgrade breaks the checks and cannot be fixed simply goes back to its previous version, and is listed here, so that the next release does not try the same version again: it is only tried again once a newer version is out, or once the cause is gone.

| Dependency | Version kept | Version tried | Cause | Date |
|---|---|---|---|---|
| `typescript` | 6.0 | 7.0.2 | TypeScript 7, rewritten in Go, no longer provides the JavaScript API of the compiler. `vue-tsc` 3.3.11 crashes on start (`ERR_PACKAGE_PATH_NOT_EXPORTED: ./lib/tsc`), its support comes with Vue Language Tools v4 ([language-tools #5381](https://github.com/vuejs/language-tools/issues/5381)). `typescript-eslint` requires TypeScript `>=4.8.4 <6.1.0` ([typescript-eslint #12518](https://github.com/typescript-eslint/typescript-eslint/issues/12518)). Both expect TypeScript 7.1. | 2026-10-20 |
| `json-p3` | 2.3.1 | 3.x | 3.x requires TypeScript 7 as a peer dependency (see `typescript`), and 2.3.2 is published without its TypeScript declarations. Its limits are in [V1.md](V1.md), section 3.1. | 2026-10-20 |

The versions fixed on purpose, for another reason than a failed upgrade, are in "Pinned versions".

## Known advisories

Security advisories reported by `npm audit` that no update can fix, as no patched version exists. Before each release, they are checked again (process in [CLAUDE.md](../CLAUDE.md), section "Publishing a version"): an entry is removed once a patched version is installed; an advisory missing from this table is new, and is reported before the release.

| Package | Advisory | Path | Exposure | Date |
|---|---|---|---|---|
| `braces` 3.0.3 | [GHSA-vfj7-8cjw-p6xm](https://github.com/advisories/GHSA-vfj7-8cjw-p6xm) (CVE-2026-93687, high): a deeply nested brace pattern exhausts the call stack and stops the Node.js process. Affects every version up to 3.0.3, the latest; no patched version. | `@vue/eslint-config-typescript` → `fast-glob` → `micromatch` → `braces` | None. Development only, when linting: nothing of it gets into the executable. The only pattern it receives is that of the configuration (`**/*.vue`), never outside data. | 2026-10-22 |

## Workarounds

Code of Prisme that makes up for a bug or a missing capability of a dependency. In the code, each one carries a comment that starts with "Workaround" and refers to this section. Before each release, after the update of the dependencies, each entry is checked again against the updated version of its dependency (for a bug, the reproduction of its ticket draft): once the dependency fixes the bug or offers the capability, the workaround is removed from the code, with its entry and its draft, under all the checks (process in [CLAUDE.md](../CLAUDE.md), section "Publishing a version"). The drafts of the upstream tickets are in `docs/upstream/`.

The APIs that only take pixels are workarounds too, listed apart, with their theme variables: see "Lengths in pixels".

| Dependency | Nature | Limitation | Workaround | Removable when | Upstream | Date |
|---|---|---|---|---|---|---|
| `sqlparser` 0.63.0 | Bug | `ALTER TABLE t DROP INDEX IF EXISTS i` (MariaDB) is read as the index `IF`, then fails on `EXISTS`; `DROP KEY i` fails too. | `crates/dialects/src/parse.rs`: after the failure, `mysql_statements.rs` parses the `ALTER TABLE` itself. | `sqlparser` parses `IF EXISTS` and `DROP KEY` there. | Ticket drafted, not posted yet: [sqlparser-alter-table-drop-index-if-exists.md](upstream/sqlparser-alter-table-drop-index-if-exists.md). | 2026-10-25 |
| `sqlparser` 0.63.0 | Bug | `SET GLOBAL TRANSACTION ...` and `SET SESSION TRANSACTION ...` give the tree of `SET TRANSACTION ...`: the scope is dropped, so a setting of the whole server would read as one of the session. | `crates/dialects/src/parse.rs`: before `sqlparser`, `mysql_statements.rs` parses `SET [GLOBAL | SESSION | LOCAL] TRANSACTION` itself, with its scope; `classify.rs` forbids the global one. | `sqlparser` carries the scope of `SET TRANSACTION`. | Ticket drafted, not posted yet: [sqlparser-set-transaction-scope.md](upstream/sqlparser-set-transaction-scope.md). | 2026-10-25 |
| `sqlparser` 0.63.0 | Missing capability | The compound time units of MySQL and MariaDB (`YEAR_MONTH`, `DAY_HOUR`...) in `EXTRACT` and `INTERVAL`. | `crates/dialects/src/mysql_parser.rs`: a dialect that wraps `MySqlDialect` and parses them; the methods it delegates are those of 0.63, hence the pin (see "Pinned versions"). | `sqlparser` parses the compound units: the dialect and the pin go away. | [sqlparser #2693](https://github.com/apache/datafusion-sqlparser-rs/issues/2693); fix proposed by [sqlparser PR #2700](https://github.com/apache/datafusion-sqlparser-rs/pull/2700), open (2026-10-25). | 2026-10-25 |
| `sqlparser` 0.63.0 | Missing capability | The MySQL and MariaDB syntax of stored routines (`CREATE PROCEDURE`, `CREATE FUNCTION`), of events (`CREATE`, `ALTER`, `DROP EVENT`) of the operations of `ALTER TABLE` on an existing index (`ALTER INDEX i VISIBLE`, `IGNORED`, `RENAME KEY`), and `START TRANSACTION WITH CONSISTENT SNAPSHOT`. | `crates/dialects/src/mysql_statements.rs` parses them with the primitives of `sqlparser`, into Prisme's own nodes; `temporary.rs` then only knows the table of such an `ALTER TABLE`. | `sqlparser` gives them nodes of its tree. | None | 2026-10-25 |
| `sqlparser` 0.63.0 | Missing capability | `ParserError` is a text: the position of the error is only written at its end, in English (` at Line: 1, Column: 29`). | `crates/dialects/src/syntax.rs` (`with_location`) reads it from the text; a change of the wording gives line 0, without error. | `ParserError` carries its position. | None | 2026-10-25 |
| `sqlparser` 0.63.0 | Missing capability | No MariaDB dialect. `sqlparser` reads a syntax specific to one database in that database's dialect and in `GenericDialect` (rule of its README), and accepts queries the database rejects: `MySqlDialect` serves MySQL and MariaDB, and no ticket asks for a MariaDB dialect. | `crates/dialects/src/lib.rs`: MariaDB is parsed with Prisme's MySQL dialect (`mysql_parser.rs`). Telling MySQL from MariaDB stays with Prisme. | Lasting: no MariaDB dialect is planned. Checked again if one appears. | None | 2026-10-25 |
| `reka-ui` 2.11.0 | Bug | A `SplitterPanel` sized in pixels (`size-unit="px"`), folded or unfolded by dragging its handle, emits neither `collapse` nor `expand`: `callPanelCallbacks` compares the size converted back to pixels (28.000000000095 for 28) with `collapsedSize` at 10 decimals. `isPanelCollapsed` (`data-state`, exposed `isCollapsed`) was fixed in 2.11.0 ([reka-ui #2742](https://github.com/unovue/reka-ui/issues/2742)), the events were not. | `web/src/App.vue`: the state of the console and of the side panels follows the panel's `isCollapsed` on each `resize`. | Reka emits `collapse` and `expand` for these panels. | Ticket drafted, not posted yet: [reka-ui-splitter-px-collapse-events.md](upstream/reka-ui-splitter-px-collapse-events.md). | 2026-10-25 |
| `reka-ui` 2.11.0 | Missing capability | No API to forget a layout saved by a splitter (`autoSaveId`). | `web/src/components/cellLayout.ts` removes the entry from the local storage itself, under Reka's own prefix `reka:`; a renamed prefix would leave the layouts of the closed editors behind, without error. | Reka can forget a saved layout, or takes a storage of Prisme. | None | 2026-10-25 |
| `codemirror` 6.0.2 | Missing capability | `basicSetup` takes no options: its fold gutter cannot be left out. | `web/src/components/sqlSetup.ts`: a copy of `basicSetup` without `foldGutter()`, as CodeMirror's documentation advises; the copy follows `basicSetup` when it changes. | `basicSetup` takes options. | None | 2026-10-25 |
| `@codemirror/lang-sql` 6.10.0 | Missing capability | `sql()` always adds the completion of the keywords. | `web/src/components/SqlEditor.vue` (`currentLanguage`) builds the language by hand, with the completion of the catalog's names only. | `sql()` can leave the keywords out. | None | 2026-10-25 |
| `@codemirror/lang-sql` 6.10.0 | Missing capability | `schemaCompletionSource` only quotes the names given as plain strings, not as `Completion` objects (which carry a type and a detail). | `web/src/components/sqlCompletion.ts` (`nameCompletion`) quotes them itself. | `lang-sql` quotes the `Completion` objects too. | None | 2026-10-25 |

## Repository layout

```
crates/
  connectors/      access to target databases (MySQL / MariaDB for now)
    tests/         integration tests on real servers (Docker)
  dialects/        query analysis: default limit, read / write, beautifier, keywords
  splitter/        splitting a text into queries, per dialect
  server/          the prisme executable: configuration, master key, internal database, API
    migrations/    numbered SQL migrations of the internal database
web/               Vue interface
  src/api/         API calls and query session
    generated/     TypeScript types generated from Rust (do not edit)
  src/components/  interface components
  src/stores/      shared state (connections, settings)
docs/              design decisions and notes
  upstream/        drafts of tickets for the dependencies
```

## Design documents

- [V1.md](V1.md): scope, decisions, detailed Rust stack, development order of the v1.
- [FEDERATED.md](FEDERATED.md): the federated engine, after the v1.
- [QUESTIONS.md](QUESTIONS.md): ideas kept or dropped, open questions.
- [PERF.md](PERF.md): performance problems met, their cause and fix.
- [IDEAS-EXPORTS.md](IDEAS-EXPORTS.md): ideas for copying and exporting results.
- [IDEAS-VIZ.md](IDEAS-VIZ.md): ideas for drawing a database and its relations as a graph.
- [CHANGELOG.md](CHANGELOG.md): changelogs of the published versions.
- [CLAUDE.md](../CLAUDE.md): working rules of the repository.
