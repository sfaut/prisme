# Prisme: developer guide

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
- More detailed logs: set `RUST_LOG` first, `$env:RUST_LOG="debug"` in PowerShell.

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
- **Conformance suite of the dialects** (`crates/dialects/tests/conformance.rs`): each query of the files `crates/dialects/tests/conformance/*.yaml`, written in a dialect, runs on each server of that dialect, and its transpilation on each server of the other dialect. Both give the same result (same columns, same type families, same rows in the same order), or Prisme refuses the query with the rule the case expects. The servers are MySQL 8.0.11 and 8.4, MariaDB 10.6 and 11.8; the variable `PRISME_CONFORMANCE_SERVERS` lists others, to check a new version against the same cases. The pairs of servers are checked in parallel: a run takes about four minutes once the images are downloaded.

```
cargo test -p prisme-dialects --test conformance -- --include-ignored
$env:PRISME_CONFORMANCE_SERVERS="mysql:9.3.0,mariadb:11.8"; cargo test -p prisme-dialects --test conformance -- --include-ignored
```

A file of cases gives the statements that create its tables (`setup`, run once on each server, after the table `sample` of `probe`), then its cases:

```yaml
setup:
  - CREATE TABLE unix_time (id INT NOT NULL PRIMARY KEY, dt DATETIME(1) NULL)
cases:
  - from: mysql                       # dialect the query is written in
    session: ["SET time_zone = '+05:00'"]
    sql: SELECT id, UNIX_TIMESTAMP(dt) AS a FROM unix_time ORDER BY id
  - from: mariadb
    outputs: mysql < 8.0.31           # servers of the transpilation (inputs: servers of the query)
    sql: SELECT g FROM grammar INTERSECT SELECT 'b'
    refused: set.operator             # rule of the refusal expected
  - from: mysql
    compare: types                    # a value that changes at each run: columns and number of rows only
    sql: SELECT NOW() AS a
```

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

# Default row limit: read, then change (1000, 2000, 5000, 10000 or 50000)
Invoke-RestMethod "$api/settings"
Invoke-RestMethod "$api/settings" -Method Put -ContentType "application/json" -Body '{"row_limit": 5000}'
```

On error (status 4xx or 5xx), PowerShell raises an exception, and Prisme's message is in the JSON body `{"error": "..."}`.

| Route | Role | Errors |
|---|---|---|
| `GET /api/health` | Answers `ok`. | — |
| `GET /api/connections` | Lists the connections, without passwords. | — |
| `POST /api/connections` | Saves a connection, returns `{"id": ...}` (201). | 400 invalid input, 409 name already taken |
| `POST /api/connections/{id}/probe` | Connects, returns `{"flavor": ..., "version": ...}`. | 404 unknown connection, 502 server unreachable or access denied, 504 no answer within 10 s |
| `GET /api/settings` | Settings: `{"row_limit": 1000, "row_limit_choices": [...]}`. | — |
| `PUT /api/settings` | Changes the default limit, returns the updated settings. | 400 value not among the choices |
| `GET /api/profile` | Profile: `{"email": "...", "first_name": "", "last_name": "", "read_only_session": true, "preferred_dialect": "source"}`. | — |
| `PUT /api/profile` | Changes the names (trimmed, 100 characters at most), the read-only sessions and the preferred dialect, returns the profile. | 400 name too long, unknown dialect or version |
| `GET /api/dialects` | Dialects offered, with their versions: `[{"id": "mysql", "label": "MySQL", "versions": ["8.0.11", ...]}, ...]`. | — |
| `POST /api/transpile` | Transpiles without running: `{"connection_id": 1, "sql": "...", "dialect": "mysql 8.0.19"}`, optional `cursor` and `database`; returns each query with its report. | 400 dialect unknown or `source`, 404 unknown connection, 502 / 504 server |

### Running queries over WebSocket

Queries go through a WebSocket, one session per saved connection: `ws://127.0.0.1:8484/api/connections/{id}/query`. The simplest way to try it is `wscat`, through Node.js:

```
npx wscat -c ws://127.0.0.1:8484/api/connections/1/query
```

Prisme answers `{"type":"ready",...}` once connected to the target server. Then each line typed is a JSON message:

- `run` runs `sql`, one or more queries. With `cursor` (a position in the text), only the query under the cursor runs; without it, all of them run in parallel. `row_limit` overrides the default limit for this run. `dialect` gives the dialect the queries are written in: `source` (or absent) for the native SQL of the server, otherwise a dialect with its version, such as `mysql 8.0.19`, transpiled to the dialect of the server.
- `cancel` stops every running query; `cancel_query` stops one, given its number in the batch.

```
{"type": "run", "sql": "SELECT * FROM information_schema.COLUMNS", "cursor": 0}
{"type": "run", "sql": "SELECT * FROM information_schema.COLUMNS", "cursor": 0, "row_limit": 2000}
{"type": "run", "sql": "SELECT 1; SELECT 2"}
{"type": "run", "sql": "VALUES ROW(1), ROW(2)", "cursor": 0, "dialect": "mysql 8.0.19"}
{"type": "cancel"}
{"type": "cancel_query", "query": 1}
```

Prisme first announces the queries of the batch (`batch`, each one numbered), then, for each query, messages carrying its number (`query`), in this order:

1. `started`: the query starts (in parallel, it may wait for its turn).
   For a query written in another dialect, `transpiled` follows, with the report of its transpilation: the transpiled query, its steps (rule, kind, message, position in the query) and its errors; what follows applies to the transpiled query, and a query that cannot be transpiled ends with an `error`.
2. `columns`: column names and types, columns right-aligned by the grid (numbers, dates and times), the text actually run, how the limit was handled (`added` if Prisme added `LIMIT N + 1`, `user_limit` if the query had one, `not_applicable` for `SHOW` or `EXPLAIN`), the maximum row count (`expected_rows`, `null` if unknown) and the execution time measured by the server (`execution_us`, in microseconds).
3. `rows`: the rows, in batches of 500, each cell as text or `null`.
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

### Measurements on real servers

The research of the dialects compares the servers through `probe`, a development tool outside the executable (`crates/probe`, Docker Desktop running). Each server is an official image, `--server mysql:8.0.11`, repeated for several servers (by default MySQL 8.0.11 and 8.4, MariaDB 10.6 and 11.8). Its container, `prisme-probe-<name>-<tag>`, is created at the first run, without any published port, then kept running for the next ones: `docker stop` stops it, `docker rm` deletes it.

```
# Statements on each server, output as tables (an error does not stop the next statements)
echo "SELECT LPAD('hi', 4, '')" | cargo run -p prisme-probe -- sql

# Type, nullability, value, warning or error of expressions; * marks the servers that differ
cargo run -p prisme-probe -- expr --expression "FROM_UNIXTIME(0)" --setup "SET time_zone = '+00:00'"
cargo run -p prisme-probe -- expr cases.txt --differ

# Built-in functions of a reference file against the servers of its dialect
cargo run -p prisme-probe -- functions mysql --server mysql:9.3.0
```

A file of cases holds one expression per line, `id expression`; an id starting with `~` for an expression whose value changes at each run (only its type is compared); `! statement` for a statement run first; `#` for a comment. The expressions read the one row of the table `sample`, described by `cargo run -p prisme-probe -- expr --help`.

`functions` brings a reference file up to a new version: it lists the functions of the file absent from a server whose version lies in their period, those present outside their period, the names that call a stored function between backticks or followed by a space otherwise than the file says, and the built-in functions of the server the file lacks (the internal functions of MySQL are left out on purpose).

## SQL keywords

Syntax highlighting and the beautifier share the keywords of each dialect, in its reference file, edited directly: `crates/dialects/reference/mysql.yaml` and `mariadb.yaml`, which also hold the constructs of the grammar by version. The keywords list reserved words, non-reserved keywords, types and values, with, when needed, the version a word appeared in (`since`). The editor only highlights the words of the server's version. The beautifier rules are described in section 2.9 of [V1.md](V1.md).

## Continuous integration and releases

GitHub Actions (`.github/workflows/ci.yaml`) checks every push to `master` and every pull request, in about two minutes:

- interface: formatting, lint, types and tests;
- backend: formatting, `clippy` and all tests, Docker ones included.

A change that only touches documentation (`*.md`, `docs/`, `.claude/`, `LICENSE`) runs nothing. The Windows, Linux and macOS executables are only built for a release, or when the workflow is run by hand ("Actions" tab, "CI", "Run workflow"): they are then downloadable from the run's artifacts (GitHub sign-in required, kept 90 days).

A version is published through a GitHub release, whose changelog is written by hand (full process in [CLAUDE.md](../CLAUDE.md), section "Publication d'une version"). The versions in `Cargo.toml` and `web/package.json` must already match the tag, and the target commit must be on GitHub:

```
git tag --annotate v0.1.0 --message "Prisme v0.1.0" origin/master
git push origin v0.1.0
gh release create v0.1.0 --verify-tag --title v0.1.0 --notes-file changelog.md
```

The published release runs the CI again on that commit. It checks the versions, then attaches `prisme-v0.1.0-windows-x86_64.exe`, `prisme-v0.1.0-linux-x86_64` and `prisme-v0.1.0-macos-universal` to the release, about twenty minutes after publication. Past changelogs are in [CHANGELOG.md](CHANGELOG.md).

## Repository layout

```
crates/
  connectors/      access to target databases (MySQL / MariaDB for now)
    tests/         integration tests on real servers (Docker)
  dialects/        query analysis: default limit, read / write, beautifier, keywords
  types/           internal types and values, with their nullability
  plan/            logical plan: nodes, expressions, output schemas
  splitter/        splitting a text into queries, per dialect
  probe/           measurements on real servers, for the research of the dialects (development tool)
  server/          the prisme executable: configuration, master key, internal database, API
    migrations/    numbered SQL migrations of the internal database
web/               Vue interface
  src/api/         API calls and query session
    generated/     TypeScript types generated from Rust (do not edit)
  src/components/  interface components
  src/stores/      shared state (connections, settings)
docs/              design decisions and notes
```

## Design documents

- [V1.md](V1.md): scope, decisions, detailed Rust stack, development order of the v1.
- [FEDERATED.md](FEDERATED.md): the federated engine, after the v1.
- [QUESTIONS.md](QUESTIONS.md): ideas kept or dropped, open questions.
- [PERF.md](PERF.md): performance problems met, their cause and fix.
- [IDEAS-EXPORTS.md](IDEAS-EXPORTS.md): ideas for copying and exporting results.
- [CHANGELOG.md](CHANGELOG.md): changelogs of the published versions.
- [CLAUDE.md](../CLAUDE.md): working rules of the repository.
