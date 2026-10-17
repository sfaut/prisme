# Performance problems

This document describes the performance problems met in Prisme: what could
be seen, how they were measured, why they appeared and how they were fixed.
It also serves as a guide to measure again. One chapter per problem.

## 1. Interface frozen while running: the grid drew every row

Problem met in October 2026 in the result grid.

### 1.1 Symptom

While several queries ran in parallel (Ctrl+Shift+Enter), the result tabs
froze: a click on a tab took several seconds to be taken into account, and the
whole interface seemed blocked until the end of the run.

A first fix (commit `6176cca`) grouped the messages received from the server
to redraw the interface at most once every 100 ms. It changed nothing visible:
it targeted a real but secondary cost, and it was written without measuring.

### 1.2 Measurement

The problem was then measured in a real browser:

- isolated environment: MariaDB 11.3 in Docker, a Prisme and a Vite server on
  other ports and a throwaway data folder, so as not to touch the development
  instance;
- headless Edge, driven by Playwright (`playwright-core`, which uses the
  installed browser);
- scenario: 8 queries in parallel, 4 of 50,000 rows and 4 of 1,250 rows, then
  12 clicks on tabs during the run;
- indicators: delay between the click and the tab shown, browser "long tasks"
  (blocks of more than 50 ms, collected by a `PerformanceObserver`), CPU
  profile (`Profiler` of the Chrome DevTools protocol), number of elements in
  the page.

| Measure | Before | After the 1st fix (`a91fd29`) | After the 2nd fix (`d0534f6`) |
|---|---|---|---|
| Total run time | 118 s | 6 s | 4.5 s |
| Delay of a click on a tab | 6 to 22 s | 0.07 to 0.5 s | 0.04 to 0.18 s |
| Long tasks (count, total) | 65, 110 s | 13, 1.2 s | 2, 0.14 s |
| Longest task | 13.5 s | 131 ms | 78 ms |
| Elements in the page (2 results of 20,000 rows) | 100,400 | 548 | 548 |

The profile before showed 53% of the time in the browser engine (style and
layout computation), 14% in garbage collection, and the rest in rendering the
grid's rows. The number of elements gave the key: the grid of a single tab
contained 20,000 drawn rows, for a result of 20,000 rows.

### 1.3 Main cause: virtual scrolling was bypassed

#### 1.3.1 The principle of virtual scrolling

The result grid (`web/src/components/ResultGrid.vue`) shows tens of thousands
of rows while drawing only a few dozen: those visible in its scroll area, plus
a margin. The `@tanstack/vue-virtual` library computes these rows from the
height of the scroll area.

```text
 Expected behaviour (600 px scroll area, 22 px rows)

 ┌───────────────────── scroll area: 600 px ──────────────────┐
 │ row 1,040   ◄── drawn                                      │
 │ row 1,041   ◄── drawn                                      │
 │ …           ◄── ~27 visible rows + 20 of margin            │
 │ row 1,067   ◄── drawn                                      │
 └────────────────────────────────────────────────────────────┘
   rows 1 to 1,019 and 1,088 to 20,000: only an empty space
   of the right height, so that the scroll bar is correct
```

#### 1.3.2 What was happening

The computation assumes that the scroll area has a height **bounded** by its
container. But nothing bounded it: it took the height of its content. For
20,000 rows of 22 pixels, it measured 440,022 pixels. For the library, all
the rows were therefore "visible", and it drew them all.

```text
 Before the fix                              After the fix

 window (900 px)                             window (900 px)
 ┌───────────────────────────┐               ┌───────────────────────────┐
 │ layout: 274 px            │               │ layout: 900 px            │
 │ ┌───────────────────────┐ │               │ ┌───────────────────────┐ │
 │ │ editor, buttons       │ │               │ │ editor, buttons       │ │
 │ │ tabs                  │ │               │ │ tabs                  │ │
 │ │ ┌───────────────────┐ │ │               │ │ ┌───────────────────┐ │ │
 │ │ │ grid              │ │ │               │ │ │ grid: 640 px      │ │ │
 │ │ │ 440,022 px        │ │ │               │ │ │ (remaining space) │ │ │
 │ │ │ = 20,000 rows     │ │ │               │ │ │ ~30 rows          │ │ │
 │ │ │   drawn           │ │ │               │ │ │   drawn           │ │ │
 ┆ ┆ ┆                   ┆ ┆ ┆               │ │ └───────────────────┘ │ │
 ┆ ┆ ┆  (overflows the   ┆ ┆ ┆               │ └───────────────────────┘ │
 ┆ ┆ ┆   page)           ┆ ┆ ┆               └───────────────────────────┘
```

Each batch of rows received then redrew tens of thousands of elements, in each
open tab. The browser spent its time recomputing styles and layout, and no
longer handled clicks.

#### 1.3.3 Why it happened

Three elements combined:

1. **The layout did not fill the window.** The `NLayout` component of
   Naive UI, used for the overall layout (`web/src/App.vue`), takes the height
   of its content by default, not that of the window. The screen was only 274
   pixels high: there was therefore no "remaining space" to share.
2. **The chain of heights was broken.** For an element to take the remaining
   space without exceeding it, in CSS, each level must be a flexible container
   (`display: flex`) with `flex: 1` and `min-height: 0`. Without
   `min-height: 0`, a flexible element never shrinks below the height of its
   content. The result tabs (`NTabs`), added for parallel execution,
   introduced more levels in this chain, without these rules.
3. **Nothing reported it.** The interface tests run in jsdom, a simulated
   browser without a layout engine: all heights are 0 there, and an unbounded
   grid costs nothing. The defect only shows with many rows, in a real
   browser. With a query of 1,000 rows (the default limit), it went almost
   unnoticed.

### 1.4 Secondary cause: reactivity on each row

Once the grid was bounded, the profile showed another cost: 11% of the time in
Vue's change tracking (`set`). Received rows were added one by one to a
reactive array, and Vue intercepted each addition, whereas only their count
matters to redraw the grid.

### 1.5 Fixes

| Commit | Fix |
|---|---|
| `a91fd29` | The layout fills the window (`position="absolute"` on `NLayout`). Each level, down to the grid, takes the remaining space without exceeding it (`flex: 1`, `min-height: 0`). **Safeguard**: the grid's scroll area cannot exceed the window's height (`max-height: 100vh`), so that virtual scrolling can no longer be bypassed, even if the chain of heights is broken again. |
| `d0534f6` | Received rows are kept in a plain array (`markRaw`), outside Vue's tracking. Only their count (`rowCount`) is reactive: the grid and the counters read it to redraw. |
| `6176cca` | Kept: the server's messages are applied in batches, every 100 ms, and the duration clock only redraws the dots of the running queries. |

### 1.6 Takeaways

- **A virtualised grid needs a bounded height.** Any new container placed
  between the layout and the grid must extend the `flex: 1` / `min-height: 0`
  chain. The `max-height: 100vh` safeguard limits the damage, but does not
  replace a correct chain.
- **Measure before fixing.** The first fix, made without measuring, treated a
  secondary cost. Measuring in a real browser pointed to the cause in a few
  minutes: the number of elements in the page.
- **Tests in jsdom validate the logic, not the display.** For display
  performance, only a real browser is authoritative.

### 1.7 Measuring again

1. Start a throwaway MariaDB: `docker run --rm -d --name prisme-perf -p 3310:3306 -e MARIADB_ROOT_PASSWORD=secret -e MARIADB_DATABASE=test mariadb:11.3`.
2. Start an isolated Prisme: `cargo run -- --listen 127.0.0.1:8599 --data-dir ./tmp/perf`, then save a connection to `127.0.0.1:3310` in it (user `root`, database `test`).
3. Start Vite on another port, with a proxy to `127.0.0.1:8599`.
4. With Playwright (`playwright-core`, `msedge` or `chrome` browser already installed): open the interface, choose the connection, type several queries on `seq_1_to_200000` (MariaDB's sequence table), click **Tout exécuter** (Run all), then click on the tabs during the run while measuring the delay, and collect the long tasks with a `PerformanceObserver` of type `longtask`.
5. Most telling indicator: `document.querySelectorAll('.grid-row').length` during the run. It must stay in the order of a few dozen per visible tab, whatever the number of rows received.

## 2. Explorer frozen during the initial load: the catalog was reactive

Problem met in October 2026 in the object explorer.

### 2.1 Symptom

On choosing a connection, the explorer stayed inert: a click on the connected
database did not open it, whereas it should be available right away. The more
databases and tables the server had, the longer the wait.

### 2.2 Measurement

Same isolated environment as for chapter 1 (section 1.7), with a large
catalog: 20 databases of 50 tables, that is 1,000 tables of 15 columns, each
with an index and a foreign key.

The server was not at fault: read directly, the catalog stream delivers the
databases in 48 ms, the objects of the current database in 49 ms, and the
whole catalog (1.7 MB) in 0.5 s.

| Measure, during the initial load | Before | After (`9f8707a`) |
|---|---|---|
| Databases shown | ~3 min, page frozen | 0.5 to 0.6 s |
| Opening a database, "Tables", a table | more than 30 s (measurement given up) | 0.05 to 0.16 s |
| Whole catalog shown | ~3 min | 2.6 to 2.9 s |
| Longest blocking task | ~3 min | 0.14 to 0.41 s |

The profile before was unambiguous: 98% of the time in Vue's change tracking,
of which 94% in `traverse`, the function that walks an object in depth for a
"deep" watch.

### 2.3 Cause

```text
 Before: each message received starts a walk of the whole catalog

 message 1 (databases)      ──► walk of       20 objects
 message 2 (db00 objects)   ──► walk of      ~70 objects
 …
 message 20 (db19 details)  ──► walk of  ~40,000 objects
 …                              (and each message adds more)
```

The catalog (databases, tables, columns, indexes, foreign keys) was a Vue
reactive object, filled as the server's messages came. Vue tracked each of its
tens of thousands of objects. A deep watch of the store (probably that of the
Vue and Pinia devtools, active with `npm run dev`) walked it entirely on each
message: the work grew with the catalog, and the browser no longer handled
clicks.

The defect did not show on a test database of a few tables: a catalog of real
size is needed for it to appear.

### 2.4 Fix

The principle is the same as for the grid's rows (section 1.5):

- the catalog is a plain object (`markRaw`), outside Vue's tracking;
- only a revision number per connection is reactive. It increases at most
  every 100 ms during reception, then at the end;
- the explorer reads the catalog through `catalogOf()`, which also reads this
  number: the tree is rebuilt on each revision.

### 2.5 Takeaways

- **A large data structure received from the server stays outside Vue's
  reactivity.** Only a light signal (a counter, a revision) tells the
  interface that it changed.
- **Test with a realistic volume.** A catalog of a few tables reveals nothing.
- **Remaining limit**: the whole tree is rebuilt on each revision (0.4 s at
  most over 1,000 tables). With many sources and tables, only the modified
  databases will have to be rebuilt (question 26 of `docs/QUESTIONS.md`).

### 2.6 Measuring again

As in section 1.7, with a large catalog generated by a script (`CREATE
DATABASE` and `CREATE TABLE` in a loop) then loaded through
`docker exec -i … mariadb < catalog.sql`. Scenario: choose the connection,
measure the delay before the databases show, then open the current database,
its "Tables" folder and a table during the load. Wait for the end of a node's
opening animation (about 0.5 s) before clicking one of its children: a click
during the animation is ignored by the tree.

## 3. Sorted result: the view was sorted again on each batch

### 3.1 Symptom

With a sort active while a large result is received, the interface froze
during the reception. A result of 50 000 rows, sorted on a text column, took
about 15 s of main thread in total, spread over the batches.

### 3.2 Measurement

The functions of `resultGrid.ts` run on 50 000 rows, in a test in the
project, with a 4-column result (identifier, text, decimal number, date
with some `NULL`), the rows arriving by batches of 500 in one array that
grows, as the store does with `push`. Without a sort, the whole view takes
under 1 ms per computation. With the sort, the view is recomputed on each
batch:

| Scenario (50 000 rows, 100 batches)     | Time  |
|-----------------------------------------|-------|
| Full sort on each batch (before)        | 15.4 s |
| Incremental view (after)                | 1.5 s  |

A single sort of 50 000 text values takes about 0.2 s.

### 3.3 Cause

`ResultGrid.vue` computed its view with `visibleRows` on the whole array,
each time `rowCount` changed, that is on each batch. The sort is O(n log n)
collator comparisons, so the total cost grew with the square of the number of
batches.

### 3.4 Fix

The view is kept from one computation to the next (`updateView`, with its
`ViewMemory`). Each update filters and sorts only the rows received since,
then merges them into the view already sorted. The merge keeps the server's
order for equal values, as the full sort does. A new array, a new filter or
a new sort starts again from the first row.

### 3.5 Takeaways

- **Measure with the real data flow.** A benchmark that copies the array on
  each batch resets the memory each time and measures the wrong path: the
  fix was first measured that way and looked slower.
- **A memoised value that changes in place must return a new array**, or Vue
  does not see the change and the virtual list keeps its old length.
- **Remaining cost**: 1.5 s over 50 000 rows, spread over the reception
  (about 15 ms per batch of 500 rows). The comparisons of the merge are the
  largest part.

## 4. Long single-line SQL: beautifying was quadratic

### 4.1 Symptom

The beautifier, and the analysis that runs after each pause of the typing,
slowed down sharply on a query written on one line, such as an `INSERT` with
thousands of rows. A 16 KB query took 5 s to beautify, a 138 KB one 203 s.

### 4.2 Measurement

A temporary integration test of `prisme-dialects` calls `beautify_with` on
`INSERT INTO t (a, b, c, d) VALUES (…), (…)` of growing sizes, all on one
line (4 values per row, about 9 tokens per row):

| Size (rows)      | Before   | After   |
|------------------|----------|---------|
| 500 (16 KB)      | 5.0 s    | 0.3 s   |
| 2 000 (67 KB)    | 54.5 s   | 1.2 s   |
| 4 000 (138 KB)   | 202.8 s  | 1.8 s   |
| 16 000 (582 KB)  | not run  | 8.7 s   |

### 4.3 Cause

Each token position given by `sqlparser` (line and column) was converted into
a byte offset by `byte_offset`, which scanned the text from the start of its
line, character by character, and counted the lines before it. On one long
line, each conversion costs the length of the line, and the conversions
of all the tokens make the total quadratic in the length of the query.

### 4.4 Fix

`OffsetIndex` (module `offsets.rs`) is built once per text, in one pass: the
byte offset of each character, and the index of the first character of each
line. A conversion is then a lookup. The five modules that converted positions
(`alias`, `analysis`, `beautify`, `functions`, `layout`, and `limit` for the
insertion point of a `LIMIT`) build the index once, before their loops.

### 4.5 Takeaways

- **A conversion done per token must not scan the text.** Build the index
  once, then look up.
- **Long single-line texts are a case of their own**: a scan per line hides
  behind the usual short lines of a query.
- **Remaining cost**: about 1.8 s for 138 KB, still growing a little faster
  than linearly. Section 4.6 covers the two scans on the list of tokens that
  were fixed after this one; the remaining growth is not explained yet.

### 4.6 Two scans on the list of tokens

Two functions searched the list of tokens for each alias (`preceded_by_as`,
module `alias`) and for each function (the token of each function start,
module `functions`): a scan of the whole list for each of them. Both now use a
table of the tokens by position, built once.

Measured with `SELECT` lists of growing size, each column
written with an alias (`c1 a1, …`) or inside a function (`upper(c1), …`),
on one line:

| Columns | Aliases: before | Aliases: after | Functions: before | Functions: after |
|---------|-----------------|----------------|-------------------|------------------|
| 2 000   | 1.07 s          | 0.41 s         | 0.78 s            | 0.46 s           |
| 4 000   | 1.56 s          | 1.27 s         | 1.35 s            | 1.64 s (noise)   |

The gain is clear at 2 000 columns and within the noise at 4 000: the time of
the other passes dominates there, and it is not explained yet.

## 5. Resumed result: the view was computed in one go

### 5.1 Symptom

When a query session resumes after a reconnection, its rows come back at once.
The grid then computed its whole view in one call: with a sort active, a sort
of 50 000 rows (0.2 s), which froze the interface for the duration.

### 5.2 Measurement

On 50 000 rows sorted on a text column, a single call of `updateView` takes
about 0.2 s. Computed by chunks of 2 000 rows, one per frame (25 frames), the
longest frame takes 41 ms and the total is 0.55 s: the merge of each chunk
compares the rows already sorted, which grows with the result.

### 5.3 Cause

The view was a `computed` over the whole array of rows, so Vue ran it in the
frame that changed the rows, without a break.

### 5.4 Fix

`updateView` takes a `chunk` argument: a call examines at most that many rows.
The grid (`ResultGrid.vue`) computes its view with `refreshView`: the first
chunk at once, then one chunk per animation frame, until every row is in the
view. A newer computation stops the previous one. A small result is still
computed at once, in its first chunk.

### 5.5 Takeaways

- **A computation that can grow with the data must be split by frames** when
  it can take longer than a frame. Splitting it costs a little more in total.
- **The merge of a chunk is linear in the rows already sorted**: a much
  larger result would need a different split, or a worker.

## 6. Keyword lookups allocated for each entry

### 6.1 Symptom

Beautifying a multi-line `INSERT` of 4 000 rows (146 KB) took 3.2 s, and its
analysis took 1.0 s. The time grew linearly with the query, but with a large
constant factor.

### 6.2 Measurement

On the same query, measured with a temporary test of `prisme-dialects`:
analysis 0.97 s → 0.67 s, beautifying 3.17 s → 1.19 s.

### 6.3 Cause

Each keyword check (`is_keyword`, `is_known_in_version`,
`is_structural_keyword`) compared the word with every entry of the dialect's
keyword file, and each comparison built the entry's name again, in capitals,
in a new `String`. Several checks run for each token, in several passes.

### 6.4 Fix

`Entry::name` returns the name as written in the file, and the comparisons
ignore its case (`eq_ignore_ascii_case`): no allocation. The list of keywords
for the editor (`keywords()`) still returns its names in capitals.

### 6.5 Takeaways

- **A helper called per token must not allocate.** Check the cost of the
  helpers of a hot loop before the loop.
- **A lookup in a list of several hundred entries per token is still linear.**
  A map built once would remove it; not done here, as the gain is already
  measured on the allocations.

## 7. Identifiers by visitor, keywords by index

### 7.1 Measurement

On a multi-line `INSERT` of 4 000 rows (146 KB), measured with a temporary
test of `prisme-dialects`:

| Step                          | Before | After  |
|-------------------------------|--------|--------|
| Analysis of the text          | 0.67 s | 0.21 s |
| Beautifying                   | 1.19 s | 0.82 s |

### 7.2 Identifiers: visitor instead of the JSON of the tree

The analysis found the position of each identifier by serialising the whole
syntax tree to JSON, then looking for the objects that had the shape of an
identifier. The serialisation took 0.18 s on this query, and the shape test
could match any other node of the same form. The visitor of `sqlparser`
(`pre_visit_ident`) reaches each identifier directly. A test compared the two
methods on eight queries (joins, CTEs, subqueries, `UPDATE`, `INSERT`,
`CREATE`, quoted names): the same positions. The `serde_json` dependency and
the `serde` option of `sqlparser` are removed from `prisme-dialects`.

### 7.3 Keywords: index instead of a walk

Each keyword check walked the list of the dialect's words, several hundred
entries. The words are now indexed once by their name in capitals, and the
values in a set. A lookup is a hash, and the words are compared without
allocating.

### 7.4 Takeaways

- **Prefer the visitor of a parser to the shape of a serialised tree**: the
  visitor says what a node is, the shape only suggests it.
- **A table of words read once is the normal form of a keyword list**: a
  scan per token only makes sense for a handful of words.

## 8. Slow conformance suite: each connection waited for IPv6, the servers wrote to disk

Problem met in October 2026 in the conformance suite of the dialects
(`crates/dialects/tests/conformance.rs`).

### 8.1 Symptom

A run took about six minutes (361 s) once the images were downloaded, for 610
comparisons of a query and its transpilation, checked 8 at a time on four
servers.

### 8.2 Measurement

Temporary timers in the harness; the time of each phase of a comparison is
added over all the comparisons, 8 of them running at once:

| Phase | Time |
|---|---|
| Start of the 4 servers, in parallel | 30.4 s |
| Tables of the files (`setup`) | 10.7 s |
| Comparisons, wall clock | 317.8 s |
| Opening the connection to the input server, its session included (sum) | 1,253 s |
| Opening the connection to the output server (sum) | 1,252 s |
| `SELECT VERSION()`, transpilation and catalog of the tables (sum) | 2.0 s |
| Both queries (sum) | 13.5 s |

Each of the 1,220 connections took about 2 s; the queries, a few
milliseconds.

### 8.3 Cause

`testcontainers` gives the host `localhost`. On Windows, `localhost`
resolves to `::1` first, then to `127.0.0.1`
(`[System.Net.Dns]::GetHostAddresses("localhost")`). The port that Docker
Desktop publishes for the container does not answer on `::1`: the connection
is refused, and Windows reports a refused connection after about 2 seconds
(2.09 s on `::1`, 2.05 s on `127.0.0.1`, measured on a closed port).
`tokio` then tries `127.0.0.1`, which answers. The tool `probe` was spared:
it runs its client inside the container, on `127.0.0.1`.

### 8.4 Fix

The harness writes `localhost` as `127.0.0.1` (`ipv4_host`), as do the
integration tests of the connector (`crates/connectors/tests/mysql.rs`).

| Measure | Before | After |
|---|---|---|
| Opening a connection, on average | 2.05 s | 18 ms |
| Comparisons, wall clock | 317.8 s | 5.2 s |
| Tables of the files | 10.7 s | 2.3 s |
| Whole run | 361 s | 39 s |

### 8.5 Second cause: the start of the servers

The start of the four servers then took 70% of the run. At its first start,
the entrypoint of the MySQL and MariaDB images creates the data directory on
the disk of the container, then loads the tables of the named time zones of
the system (`mysql_tzinfo_to_sql`). The cases set a time zone by its offset
only (`SET time_zone = '+05:00'`). The harness now skips these tables
(`MYSQL_INITDB_SKIP_TZINFO`, `MARIADB_INITDB_SKIP_TZINFO`), and places the
data directory in memory (a `tmpfs` on `/var/lib/mysql`), lost with the
container, which the suite drops at its end anyway. One run each, the start
varying by about 2 s from one run to the next (30.4 s and 28.6 s with the same
settings):

| Measure | Through `127.0.0.1` | Without time zone tables | Data in memory as well |
|---|---|---|---|
| Start of the 4 servers | 28.6 s | 21.6 s | 12.1 s |
| Tables of the files | 2.3 s | 2.1 s | 0.4 s |
| Comparisons, wall clock | 5.2 s | 4.7 s | 5.0 s |
| Whole run | 39.1 s | 31.0 s | 20.5 s |

A later run, without the timers, took 27.0 s. Checking 16 pairs at once
instead of 8 gives 4.8 s of comparisons instead of 5.0 s: the parallelism
stays at 8.

### 8.6 Takeaways

- **On Windows, `localhost` costs 2 seconds per connection to a server that
  does not answer on `::1`**, as a container of Docker Desktop: write
  `127.0.0.1`. A connection of Prisme to `localhost` meets the same delay.
- **Time each phase before optimising.** The comparisons ran in parallel
  and none looked slow: the connections took 99% of their time.
- **A throwaway server needs neither a durable disk nor the time zones it
  does not read**: in memory, the servers start twice as fast.

### 8.7 Measuring again

Add timers around the phases of `check` and of the test (`Instant::now()`,
the durations added in a `static Mutex<[Duration; N]>`), then run the suite
with `-- --include-ignored --nocapture`.
