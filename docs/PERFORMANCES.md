# Performance problems and measures

This document describes the performance problems met in Prisme: what could
be seen, how they were measured, why they appeared and how they were fixed.
It also keeps every measure made to compare options (benchmarks), with their
numbers and the decision they led to. It serves as a guide to measure again.
One chapter per problem or per measure.

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

1. Start the latest MariaDB LTS of the development servers: `docker compose up --detach --wait mariadb-lts` (`docs/DEVELOPER.md`, section "Development servers").
2. Start an isolated Prisme: `cargo run -- --listen 127.0.0.1:8599 --data-dir ./tmp/perf`, then save a connection to `127.0.0.1:3322` in it (user `root` without a password, database `shop`).
3. Start Vite on another port, with a proxy to `127.0.0.1:8599`.
4. With Playwright (`playwright-core`, `msedge` or `chrome` browser already installed): open the interface, choose the connection, type several queries on `seq_1_to_200000` (MariaDB's sequence table), click **Exécuter tout** (Run all), then click on the tabs during the run while measuring the delay, and collect the long tasks with a `PerformanceObserver` of type `longtask`.
5. Most telling indicator: `document.querySelectorAll('.grid-row').length` during the run. It must stay in the order of a few dozen per visible tab, whatever the number of rows received.

## 2. Navigator frozen during the initial load: the catalog was reactive

Problem met in October 2026 in the object navigator.

### 2.1 Symptom

On choosing a connection, the navigator stayed inert: a click on the connected
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
- the navigator reads the catalog through `catalogOf()`, which also reads this
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
computed at once, in its first chunk. Without a user filter nor a sort, the
whole view is computed at once: examining a row then costs only its addition
to the view, while each chunk costs an update of the grid and a drawing,
several per batch during the reception of a large result.

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

## 8. Navigator without virtual scrolling: a trial measured

Trial made in October 2026 on the object navigator, without a commit: should
the tree keep its virtual scrolling?

### 8.1 Question

The navigator draws only its visible lines (`TreeVirtualizer` of Reka UI).
The lines pinned at the top while the tree scrolls (`stickyRows.ts`) are
therefore computed from the scroll position, then drawn a second time in a
layer above the tree. Without virtual scrolling, a nested tree, where each
open node is a section holding its line and its children, gets the same
behaviour from CSS alone: each line is `position: sticky` at
`calc((var(--level) - 1) * var(--row-height))`, and a sticky element never
leaves its containing block, so the line stays at the top while its section
scrolls. The code of `stickyRows.ts`, the second layer and their tests would
go. Reka UI allows it: `TreeItem` only needs its value and its level, and the
keyboard navigation reads the lines in the order of the page.

### 8.2 Measurement

- MariaDB 11.8.9 in Docker: 10 databases of 3,000 tables of 100 columns, that
  is 30,000 tables and 3 million columns.
- Prisme v0.7.0 built in release, on another port and a throwaway data folder.
- Headless Chrome, window of 1,600 × 1,000, driven over the DevTools protocol;
  long tasks collected by a `PerformanceObserver` of type `longtask`.
- Two variants of the same build: the current tree (virtual), and a nested
  prototype without virtual scrolling, its lines sticky in CSS.
- Scenario: open the source, then `perf00`, its "Tables (3000)" folder, the
  table `t0000` and its "Colonnes (100)" folder, during the reception of the
  catalog then once it is complete; last, open the "Tables" folders of nine
  databases (27,000 lines).

| Measure | Virtual | Nested |
|---|---|---|
| Opening "Tables (3000)" during the reception | 0.14 s | 6.2 s |
| Opening `t0000` during the reception | 3.0 s (waiting for its details) | 20.6 s |
| Longest blocking task during the reception | 4.7 s | 12.8 s |
| Opening "Tables (3000)", catalog complete | 0.07 s | 1.9 s |
| Opening a table, then its columns (3,000 lines open) | 0.08 s, 0.06 s | 2.6 s, 2.4 s |
| Opening the ninth "Tables" folder (24,000 lines open) | under 0.1 s | 431 s |
| Elements in the page, 27,000 lines open | 6,600 | 466,000 |
| Duration of a frame while scrolling, 27,000 lines (median) | 67 ms | 513 ms |
| JavaScript memory used | 1.3 GB | 2.0 GB |

Headless Chrome draws in software: even the virtual tree stays above 40 ms per
frame. The durations of frames are only compared between the two variants.

### 8.3 Cause

Without virtual scrolling, each open line is a `TreeItem` of Reka UI, with its
focus management and its entry in the collection: about 0.6 ms per line on
mounting. The cost also grows faster than the number of lines: each
`TreeItem` computes whether it is open by looking for its key in the list of
the open keys, so an opening costs the number of lines times the number of
open nodes. The prototype added its own share, a context rebuilt on each
opening that redrew every branch; an optimised version would reduce the gap
without closing it.

### 8.4 Decision

The navigator keeps its virtual scrolling, and the pinned lines stay computed
by `stickyRows.ts`. Sticky lines in CSS need a nested tree, which a virtual
list, flat by nature, cannot give.

### 8.5 Takeaways

- **A tree whose lines come from the catalog keeps a virtual scrolling**: a
  single database can open thousands of lines.
- **The catalog itself weighs at this volume**, virtual scrolling or not:
  279 MB of JSON, received in 27 to 63 s, about 1.4 GB of memory, and blocking
  tasks of 4.7 to 10 s during the reception (question 26 of
  `docs/QUESTIONS.md`).

### 8.6 Measuring again

1. Generate the catalog: one SQL file per database, `CREATE TABLE tNNNN (id
   INT NOT NULL PRIMARY KEY, c001 …, c099 …)` with types in rotation (`INT`,
   `VARCHAR(50)`, `DECIMAL(12,2)`, `DATETIME`).
2. Load it into the latest MariaDB LTS of the development servers
   (`docker compose up --detach --wait mariadb-lts`, port 3322;
   `docs/DEVELOPER.md`, section "Development servers"): first
   `SET GLOBAL innodb_flush_log_at_trx_commit = 0, innodb_file_per_table = OFF, table_definition_cache = 40000, table_open_cache = 40000`,
   then the ten files in parallel
   (`docker exec -i prisme-mariadb-lts mariadb < perfNN.sql`): about 15
   minutes. Once measured, create the server again, with its demo databases
   only (`docker compose rm --stop --force --volumes mariadb-lts`, then
   `docker compose up --detach --wait mariadb-lts`).
3. Start Prisme in release on a throwaway data folder (`--listen`,
   `--data-dir`), and save the connection through `POST /api/connections`.
4. Start Chrome headless with `--window-size=1600,1000`: the interface folds
   the navigator in a narrow window and keeps it folded
   (`prisme.objectsOpen` in `localStorage`). Check the height of the tree
   (`[role=tree]`) before measuring.
5. Time each click until the expected line is in the page, then one frame.
   In the virtual tree, the number of lines in the page does not grow when a
   folder opens: wait for a given line, or read the long tasks.

## 9. Engine of the Pivoteur: native, WebAssembly, JavaScript

Benchmark of 2026-10-31, before the specification of the Pivoteur (the local
pivot table under the result grid, `docs/TODO-PIVOTER.md`).

### 9.1 Question

The Pivoteur groups and aggregates a result already received. Where, and in
what, should its engine run (point A1 of the specification): in the server,
in Rust, or in the page, in Rust compiled to WebAssembly or in TypeScript,
both run by V8?

### 9.2 Measurement

- `node bench/pivot/run.mjs [rows]` (`docs/DEVELOPER.md`, section "Engine of
  the Pivoteur"): three sides, in processes of their own, on the same rows.
- Intel Core Ultra 7 155H (22 logical cores), 33.8 GB, Windows 11, Node
  26.7.0, Rust 1.99.1.
- Rows generated alike on every side (Mulberry32, seed 42), as text cells,
  the way they arrive: `pays` (10 values), `region` (50), `categorie` (20),
  `produit` (1,000), `canal` (4), `d` (datetime, 2024 to 2026), `qte`
  (integer, 5 % of NULL), `montant` (decimal with 2 digits), `remise`
  (float), `client` (100,000).
- Pivot: columns `YEAR(d)`, `MONTH(d)`; rows on 4 levels, in two profiles,
  "high" (`pays`, `region`, `categorie`, `produit`) and "low" (`canal` in place
  of `produit`); all the subtotals and totals, that is 15 grouping sets;
  12 measures (`COUNT(*)`, `COUNT(qte)`, `SUM(qte)`, `SUM(montant)`,
  `AVG(montant)`, `AVG(remise)`, `MIN` and `MAX` of `montant`, `d` and
  `client`).
- Preparation once per result (dictionaries of the texts, exact numbers
  scaled to integers), first pass measured; pivot, best of 3 passes. The
  WebAssembly side measures apart the copy of the cells into the memory of
  the module. Every side gives the same pivot (groups, sizes of the grouping
  sets, checksums of the states, grand total).

10 million rows:

| Step | Native, one thread | Native, all threads | WebAssembly | JavaScript |
|---|---|---|---|---|
| Copy of the cells into the module | | | 11.3 s | |
| Preparation | 15.0 s | 6.1 s | 8.0 s | 41.9 s |
| Pivot "high" (2,035,319 groups) | 5.2 s | 2.3 s | 4.9 s | 33.3 s |
| Pivot "low" (202,440 groups) | 1.6 s | 0.95 s | 1.8 s | 9.4 s |
| Peak memory | 7.25 GB | | 3.47 GB, module 1.13 GB | 4.02 GB |
| JSON of the whole pivot, "high" / "low" | 304 MB in 4.1 s / 31 MB in 0.35 s | | | |

1 million rows:

| Step | Native, one thread | Native, all threads | WebAssembly | JavaScript |
|---|---|---|---|---|
| Copy of the cells into the module | | | 0.63 s | |
| Preparation | 1.15 s | 0.47 s | 0.60 s | 1.25 s |
| Pivot "high" (1,009,005 groups) | 0.39 s | 0.21 s | 0.46 s | 1.73 s |
| Pivot "low" (202,290 groups) | 0.22 s | 0.11 s | 0.22 s | 0.58 s |
| Peak memory | 1.27 GB | | 0.64 GB, module 0.23 GB | 0.86 GB |
| JSON of the whole pivot, "high" / "low" | 144 MB in 1.6 s / 30 MB in 0.38 s | | | |

Memory: private bytes for the native side, resident set for the others. The
rows as JavaScript strings take 2.05 GB of V8 heap at 10 million rows
(0.23 GB at 1 million); the typed columns of the module, 0.92 GB (0.09 GB).
The memory of a WebAssembly module never shrinks. Timings vary up to 40 %
from one run to the next on this laptop: the sides are compared within the
same run.

### 9.3 Analysis

- **WebAssembly on one thread runs as fast as native on one thread** for
  the pivot (4.9 s against 5.2 s at 10 million rows), and prepares faster
  (8.0 s against 15.0 s): it reads each column from contiguous bytes, where
  the native side reads its cells row by row, one string allocated per cell.
- **JavaScript is 4 to 7 times slower than WebAssembly** on the pivot, and 5
  times on the preparation at 10 million rows.
- **Native on all threads halves the time of one thread**, but its pivot
  must then reach the page: 304 MB of JSON for "high" at 10 million rows,
  4.1 s to write alone, or windows of the visible lines asked as the user
  scrolls.
- **The copy of the cells into the module costs about as much as the
  preparation** (11.3 s at 10 million rows). A first version took 1.85 s at
  1 million rows: a first pass summed the lengths of the cells (0.42 s), on
  top of the reading of the cells themselves (0.42 s for 10 columns). The
  size reserved is now estimated from the first block, then grows if needed:
  0.63 s. SIMD (`simd128`) gave no measurable gain at 1 million rows, and is
  not enabled.

### 9.4 Decision

Engine in Rust compiled to WebAssembly, run in a Worker of the page, on the
rows the page already holds (2026-10-31). Prisme runs locally, so an engine
in the server spares no network; a server and a client of Prisme are
expected in the long run, the client doing this work. The engine stays a
crate independent of its target (native and `wasm32`), as in `bench/pivot`
(`engine/` shared by `native/` and `wasm/`).

### 9.5 Takeaways

- **The layout of the data weighs as much as the language**: columns of
  contiguous bytes beat cells allocated one by one.
- **WebAssembly runs on one thread** here: the parallelism of the native
  side is lost (2.3 s against 4.9 s for "high" at 10 million rows).
- **The copy of the cells is the remaining cost**: it goes once the rows
  reach the memory of the module directly (single store of the results,
  `docs/QUESTIONS.md`, section 2).

### 9.6 Measuring again

`node bench/pivot/run.mjs` (10 million rows, several minutes, up to 8 GB of
memory) or `node bench/pivot/run.mjs 1000000`, from the root of the
repository. The command builds the Rust sides, adds the WebAssembly target
of the toolchain when missing, checks the pivots, prints the table and
writes the results to `target/pivot-bench/results-<rows>.json`.

## 10. Saved results: their save and their reading again

Benchmark of 2026-10-31, on the real server.

### 10.1 Question

The rows of a result exist in three places: in the page (every result tab of
every open editor; the editors stay mounted, so coming back to a tab reads
nothing again); in the memory of the server, for the last batch of each
editor (`recorder.rs`); in the internal database, for every tab, as one JSON
value (`rows` of `result_tabs`). The page reads them again on opening, on a
reload and on a reconnection: all the tabs of the editor, with all their
rows, in one answer (`GET /api/editors/{id}/results`). What do the save and
this reading cost, for large results?

### 10.2 Measurement

- `node bench/reload/run.mjs [rows...]` (`docs/DEVELOPER.md`, section "Saved
  results"): Prisme v0.8.0 built in release, on a throwaway data folder;
  MariaDB 12.3.3 of the development servers (port 3322); a query of N rows
  of 10 columns like those of chapter 9 (`seq_1_to_N` of the sequence
  engine), with its own `LIMIT`; client under Node, over the WebSocket of
  the session.
- Same machine as chapter 9.

| Step | 100,000 rows | 1,000,000 rows | 10,000,000 rows |
|---|---|---|---|
| JSON of the rows | 10.4 MB | 104 MB | about 1.04 GB |
| Rows received (last batch) | 0.43 s | 8.3 s | 68 to 72 s |
| Save, before `done` | 0.36 s | 3.3 s | fails after 37 to 46 s |
| Reading again: answer of the server | 0.26 s | 3.0 s | empty tab |
| Reading again: download | 0.05 s | 0.55 s | |
| Reading again: parsing by the client | 0.18 s | 2.5 s | |
| Peak memory of the server | | 1.3 GB | 10.8 GB |

The answer of the server includes the reading of the internal database, the
parsing of the JSON and its writing again. The 10 million rows were measured
twice.

### 10.3 Causes

- **The save fails beyond 1,000,000,000 bytes**: SQLite caps a value at this
  size (`SQLITE_MAX_LENGTH`, "string or blob too big"). The recorder only
  logs a warning ("result not saved"); the tab keeps no rows, and the
  result is gone at the next opening of Prisme.
- **Even saved, it could not be read again in one answer**: 1.04 GB exceeds
  the maximum length of a string in V8, 536,870,888 characters
  (`buffer.constants.MAX_STRING_LENGTH` under Node 26).
- **The memory of the server** holds the rows of the last batch, their copy
  for the save, then their JSON: 10.8 GB at 10 million rows.
- **The save delays `done`**: the server ends it before announcing the end
  of the query, so the interface shows the query as running 3.3 s longer
  per million rows.

### 10.4 Decision

Question 32 of `docs/QUESTIONS.md`, to do later (leads: rows saved and read
again in chunks, a failed save shown to the user). For the Pivoteur, the
engine runs in the page on the rows it holds (chapter 9): nothing to read
again as long as the page is open.

### 10.5 Takeaways

- **Reading a saved tab again costs about 6 s per million rows** of this
  shape (3.0 + 0.55 + 2.5 s), at each opening of Prisme, for each large tab
  of the open editors.
- **A failed save must not stay silent**: the user learns it only at the
  next opening, when the tab comes back empty.

### 10.6 Measuring again

The development servers started (`docker compose up --detach --wait`) and
the interface built (`npm run build` in `web/`), from the root of the
repository: `node bench/reload/run.mjs` (1 then 10 million rows, several
minutes, up to 11 GB of memory for Prisme) or `node bench/reload/run.mjs
100000`. The results go to `target/reload-bench/results.json`, with the log
of each server (`server-<rows>.log`).

## 11. Reception of a result, and a store of the rows in a Worker

Benchmark of 2026-10-31, on the real server and in Chrome, before the
specification of the single store of the results (`docs/QUESTIONS.md`,
section 2).

### 11.1 Question

Where does the time of the reception of a large result go: database, server,
transport, page? And can a Worker that holds the rows answer the grid fast
enough, when it asks for the rows it shows?

### 11.2 Measurement

- `node bench/store/run.mjs [rows...]` (`docs/DEVELOPER.md`, section "Store of
  the results"): Prisme v0.8.0 built in release, on a throwaway data folder;
  MariaDB 12.3.3 of the development servers; Chrome 156 without a window,
  1,600 × 1,000, driven over the DevTools protocol; same machine as
  chapter 9.
- The query of chapter 10 (10 columns, 104 MB of JSON per million rows), run
  five times: by the client of MariaDB in its container, the rows thrown away
  (database alone); in the interface of Prisme, run by Ctrl+Enter once the
  interface is quiet; in a page that recognises the messages of rows by their
  start and throws them away (server and transport); in a page that handles
  them as `querySession.ts` does, without the interface; in a Worker that
  keeps them in columns (UTF-8 bytes, offsets, validities, the layout of
  `bench/pivot/js/transfer.mjs`), while the page asks it, at each frame, for
  a window of rows at a random place.
- Processor time of Prisme sampled every 100 ms, that of MariaDB read before
  and after; main thread of the page by `Performance.getMetrics`, long tasks
  by a `PerformanceObserver`; memory of the page after a garbage collection.
- `node bench/store/trace.mjs` traces the interface during the same reception
  (200,000 rows): own time of each kind of event of the main thread.

1 million rows:

| Step | Interface | Page, rows thrown away | Page, rows handled as the interface | Worker, store in columns |
|---|---|---|---|---|
| Last rows received | 33.2 s | 6.6 s | 7.5 s | 7.7 s |
| `done` received | 37.7 s | 10.3 s | 11.4 s | 11.7 s |
| Processor of Prisme, rows / save | 10.6 / 4.0 s | 9.4 / 3.3 s | 10.5 / 3.7 s | 11.2 / 3.7 s |
| Processor of MariaDB | 6.0 s | 5.0 s | 7.4 s | 5.6 s |
| Handling of the messages | | 0.07 s | 4.4 s | 2.8 s parsing, 1.3 s store |
| Main thread busy | 34.0 s, scripts 5.1 s, layout 2.0 s | | | |
| Long tasks: number, longest | 147, 0.17 s | none | 2, 0.10 s | none |
| Memory of the page, before / after | 10 / 164 MB | 1 / 1 MB | 1 / 139 MB | 1 / 1 MB |
| Store in columns, used / reserved | | | | 122 / 161 MB |

The database alone runs the query in 3.6 s, with 3.5 s of processor. A first
run gave 36.6 s for the interface and 6.9 to 7.5 s for the other steps.

Windows of visible rows asked to the Worker (1 million rows, 1,002 frames):

| Windows | Number | Median | 95th percentile | 99th percentile | Longest |
|---|---|---|---|---|---|
| 50 rows, during the reception | 429 | 2.0 ms | 9.7 ms | 16.1 ms | 20.1 ms |
| 50 rows, Worker idle | 391 | 0.6 ms | 1.2 ms | 1.5 ms | 22.1 ms |
| 1,000 rows, Worker idle | 168 | 3.0 ms | 7.3 ms | 10.8 ms | 10.8 ms |

Main thread of the interface while it receives 200,000 rows (last rows after
5.4 s, main thread busy 5.8 s):

| Work | Events | Own time | Share |
|---|---|---|---|
| Microtasks (updates of Vue) | 157 | 1.45 s | 25 % |
| Layers (`Layerize`) | 181 | 1.06 s | 18 % |
| Scripts called by the page (messages, timers) | 1,361 | 0.95 s | 16 % |
| Painting | 535 | 0.68 s | 12 % |
| Layout | 246 | 0.42 s | 7 % |
| Running of the tasks | 2,126 | 0.31 s | 5 % |
| Styles, pre-painting, commit, messages of the browser | | 0.64 s | 12 % |

### 11.3 Analysis

- **The server sets the floor**: 6.6 s per million rows, rows thrown away by
  the page. Through the network of Docker, the thread of MariaDB works 5.0 to
  7.4 s, against 3.5 s when its client runs in the container: it is busy 73 to
  98 % of the reception. Prisme works 9.4 to 11.2 s over the same time, on
  several threads. The share of each in the floor is not told apart here.
- **Parsing in a page does not lengthen the reception**: 4.4 s of main thread
  per million rows, done while the server sends (7.5 s, within the spread of
  the steps, 6.6 to 7.7 s).
- **The interface makes the reception 5 times slower** (33.2 s). Its main
  thread is busy 34 s of the 39.7 s from Ctrl+Enter to the drawing after
  `done`, and Chrome reads the WebSocket only as fast as the page takes its
  messages: the server waits, its processor time is the same as elsewhere,
  spread over 33 s. The page applies a batch every 100 ms, but draws about 30
  frames per second (181 in 5.8 s for 200,000 rows, chapter 12 says why):
  updates of Vue (25 %), drawing (layers, painting, layout, styles: about
  45 %) and handling of the messages (16 %). The interface stays usable
  (longest task 0.17 s), and slows down.
- **The Worker receives at the speed of the server** (7.7 s), parses and
  stores on its own thread (4.2 s), and leaves the main thread without any
  long task. It answers a window of 50 rows in 0.6 ms (median), 2.0 ms during
  the reception, where a window can wait for the batch being stored (20 ms at
  most); 1,000 rows, 3.0 ms. A frame lasts 16.7 ms.
- **Stored as text, the cells save little memory**: 122 MB in columns against
  139 MB as JavaScript rows (164 MB with the interface). The offsets weigh
  40 MB (4 bytes per cell). The typed columns of the Pivoteur take 0.09 GB per
  million rows of its data (chapter 9), but the store must give back the exact
  text of each cell.
- Chrome without a window draws in software (chapter 8): the figures of the
  drawing compare steps measured alike.

### 11.4 Consequences

- The second stage of the single store (the Worker owns the WebSocket of the
  session) takes the reception off the main thread: it no longer waits for
  the interface.
- The cost of each update of the interface during a reception (Vue, drawing)
  remains, with or without the store; it is a lead of its own (chapter 12).
- The layout of the cells in the store (text, dictionary of the repeated
  values, typed values that give back their exact text) is set with the lot 1
  of the Pivoteur.

These points are open in `docs/QUESTIONS.md` (section 2, single store of the
results).

### 11.5 Takeaways

- **Measure in the real interface**: a page that does the same handling
  without the interface receives 5 times faster, and hides the cost.
- **The slowest reader sets the pace**: the WebSocket of Chrome holds the
  server back as long as the main thread is busy.
- **A store in a Worker answers within a frame**, even while it receives.

### 11.6 Measuring again

The development servers started (`docker compose up --detach --wait`) and
the interface built (`npm run build` in `web/`), from the root of the
repository: `node bench/store/run.mjs` (1 million rows, a few minutes) or
`node bench/store/run.mjs 100000`; `node bench/store/trace.mjs` for the
trace of the interface (200,000 rows). The results go to
`target/store-bench/results-<rows>.json`, the trace to
`target/store-bench/trace-<rows>.json`, which the Performance panel of
DevTools opens.

## 12. Updates of the interface during a reception

Study of 2026-10-31, after chapter 11: where the main thread of the interface
goes while it receives a large result, and what five changes of the interface,
tried as prototypes, give.

### 12.1 Question

The interface receives 1 million rows 4 to 5 times slower than a bare page
that handles them the same way (chapter 11): its main thread is saturated,
and Chrome reads the WebSocket only as fast as the page takes its messages.
Which work of the interface fills the main thread, and how much does each
change save?

### 12.2 Measurement

- `node bench/store/profile.mjs` (`docs/DEVELOPER.md`, section "Store of the
  results"): the interface built without minification, sampled every 200 µs
  from Ctrl+Enter to the drawing after `done`; own time and total time of
  each function. The sampling slows the reception down (31.6 s against 25 to
  40 s without it): the profile gives the shares, not the durations.
- `node bench/store/trace.mjs`: kinds of work of the main thread
  (chapter 11), 200,000 rows.
- `node bench/store/run.mjs --interface`: the database alone and the
  interface, 1 million rows, the query of chapter 10. Each prototype is built
  once, then named by `PRISME_EXECUTABLE`; the builds of a series run in
  turns, three rounds each.
- Same machine, Chrome 156, MariaDB 12.3.3, Prisme v0.8.0 and its prototypes,
  built in release. Over the evening, the database alone took 2.6 to 8.5 s for
  the same query: the machine changes speed from one run to the next, and
  only builds measured in the same round compare.

Profile of the interface as it is, 1 million rows (main thread busy 32.3 s):

| Work | Own time | Share |
|---|---|---|
| Work of the browser outside JavaScript (styles, layout, painting, layers) | 12.3 s | 38 % |
| `getMeasurements` of the virtualizer of TanStack | 7.6 s | 24 % |
| `parseMessage` (`JSON.parse` of the messages) | 3.2 s | 10 % |
| Garbage collection | 1.9 s | 6 % |
| `utf8Length` (bytes of each message) | 0.9 s | 3 % |
| `setStyle` of Vue (styles of the rows drawn) | 0.9 s | 3 % |
| `updateView` (copy of the view) | 0.7 s | 2 % |

The updates of Vue take 13.0 s in all, the virtualizer included, called by
the render of the grid. At 200,000 rows, `getMeasurements` takes 0.40 s: 5
times more rows, 19 times more time. The page has 10 composited layers
during the reception, the grid included.

Prototypes, each one on top of the previous:

- **P1**: the grid places its rows itself: they all have the same height,
  so the first row drawn is the scroll position divided by the height of a
  row, plus 20 rows on each side, in place of the virtualizer of TanStack;
  the view grows in place (`triggerRef`), in place of a copy at each update.
- **P2**: `v-memo` on each row drawn: a row whose source row, columns,
  widths, selection and recognized contents did not change is not patched
  again.
- **P3**: the progress bar of the tab grows by a transform
  (`transform: scaleX()`, drawn by the compositor), in place of a transition
  of its width.
- **P4**: without a filter nor a sort, the view takes all the new rows at
  once, in place of 2,000 rows per frame (chapter 5).
- **P5**: the bytes of each message are counted by `TextEncoder.encodeInto`,
  by pieces of a buffer of 64 kB, in place of a loop over the characters.

Last rows received, 1 million rows, three rounds per series:

| Build | Series 1 | Series 2 | Series 3 | Long tasks |
|---|---|---|---|---|
| Interface as it is | 40.1, 34.7, 31.5 s | 26.2, 39.0, 38.1 s | | 53 to 191 |
| P1 | 26.7, 17.9, 24.4 s | | | 8 to 12 |
| P2 | 23.8, 16.5, 24.1 s | 22.7, 21.6, 23.2 s | | 5 to 14 |
| P3 | | 17.1, 17.9, 19.4 s | 20.0, 19.7, 18.4 s | 7 to 14 |
| P4 | | | 10.4, 12.3, 10.3 s | 4 to 7 |
| P5 | | | 10.2, 10.1, 9.9 s | 5 to 6 |

Whole benchmark of chapter 11 with P5, in one run (database alone 4.8 s):

| Step | Interface (P5) | Page, rows thrown away | Page, rows handled as the interface | Worker, store in columns |
|---|---|---|---|---|
| Last rows received | 10.4 s | 8.0 s | 9.8 s | 11.2 s |
| `done` received | 18.2 s | 13.4 s | 19.5 s | 19.3 s |
| Long tasks: number, longest | 6, 0.13 s | none | 1, 0.14 s | 1, 0.05 s |

Main thread of P3 while it receives 200,000 rows (last rows after 4.2 s,
main thread busy 4.6 s): 129 frames, against 181 for the interface as it is
(chapter 11); scripts 1.03 s, layers 0.73 s, updates of Vue 0.73 s (against
1.45 s), painting 0.51 s, layout 0.37 s.

Profile of P5, 1 million rows (main thread busy 13.1 s): work of the browser
outside JavaScript 5.1 s (39 %), `parseMessage` 4.5 s (34 %), updates of Vue
1.5 s (11 %), garbage collection 0.9 s, `encodeInto` 0.7 s.

### 12.3 Analysis

- **The virtualizer of TanStack costs the rows received so far at each
  update.** When the number of rows changes, `getMeasurements` computes again
  the position of every row (`@tanstack/virtual-core` 3.19.8, same code in
  3.19.9): at 1 million rows, 7.6 s, quadratic in the rows, and its arrays
  feed the garbage collection. The rows of the grid have a fixed height,
  which its own windowing turns into two divisions.
- **The copy of the view is linear at each update too**: 0.7 s.
- **The progress bar of the tab made the page draw at every frame.** Its
  width changes with each batch, every 100 ms, and each change starts a
  transition of 0.3 s: a width is animated by the main thread (styles,
  layout, painting, layers at each frame) during the whole reception. A
  transform is animated by the compositor. P3 saves 3.7 to 5.6 s.
- **Chunks of 2,000 rows multiplied the frames.** At 50,000 rows per second,
  a batch of 100 ms takes three frames of the view, each with an update of
  Vue and a drawing (about 16 ms of drawing per frame at 200,000 rows).
  Without a filter nor a sort, examining a row costs a `push`: P4 saves
  7.4 to 9.6 s. A filter or a sort keeps its chunks (chapter 5).
- **`v-memo` and `encodeInto` save less than the spread of the measures**:
  P2 is faster than P1 in each round, by 1 to 11 %, and P5 than P4, by 2 to
  18 %, while the same build varies by a third from one round to the next.
- **With the five changes, the interface receives at the speed of a bare
  page** (10.4 s against 9.8 s in the same run), and its longest task stays
  under 0.3 s. What remains on the main thread: the work of the browser
  (39 %), the parsing of the messages (34 %) and the updates of Vue (11 %).
- **`done` comes 5 to 10 s after the last rows in every step**: the server
  saves the result before it ends the query (chapter 10).
- The tree of the navigator draws its lines with the same virtualizer
  (`TreeVirtualizer` of Reka UI): not measured here.

### 12.4 Consequences

- P1, P3 and P4 bring the large gains; P2 and P5, small ones. P1 removes
  `@tanstack/vue-virtual` from the dependencies of the interface (Reka UI
  keeps it for its own lists).
- The parsing of the messages leaves the main thread with the second stage
  of the single store, when the Worker owns the WebSocket (chapter 11).
- The save of the result before `done` is a lead of the server, apart.

The changes made are in section 12.5. The save of the result before `done`
stays open in `docs/QUESTIONS.md` (section 5, question 33).

### 12.5 Changes made

Decision of 2026-10-31: P1, P3 and P4 are made in the interface, then P5
on 2026-11-01. P2 is left out: its gain stays within the spread of the
measures, and `v-memo` would have to list every value a row shows, or leave
a row stale.

- The grid computes the rows to draw itself (`drawnRange` in
  `resultGrid.ts`): two divisions of the scroll position, at a cost that
  does not depend on the number of rows. `@tanstack/vue-virtual` leaves the
  direct dependencies of the interface; Reka UI keeps it for the tree of the
  navigator, whose count changes when a node opens.
- The view grows in place (`updateView`): the grid tells Vue of the change
  (`triggerRef`), and the frame of a download copies the view once
  (`viewFrame`), so that rows received during the download stay out of it.
- The progress bar of the tab grows by `transform: scaleX()`: it gets a
  composited layer of its own.
- Without a user filter nor a sort, the view takes all the new rows at once;
  with one of them, it keeps its chunks of 2,000 rows per frame (chapter 5).
- The bytes of each message are counted by `TextEncoder.encodeInto`, by
  pieces of a buffer of 64 kB (`utf8Length`), in place of a loop over its
  characters in JavaScript.

Each change carries its measure in a comment of the code, and a test that
fails if it is undone:

- `ResultTab.spec.ts`: the bar is scaled, its width untouched;
- `ResultGrid.component.spec.ts`: the whole view at once without a filter,
  by chunks with one; the length of the view follows the rows received into
  the same array;
- `querySession.spec.ts`: a long text measured by pieces, with a surrogate
  pair on each side of the end of a piece in turn;
- `resultGrid.spec.ts`: the view grown in place, a new one for a new array;
  a frame that keeps its rows while the view grows; `drawnRange` (margins,
  last row, view that shrank, a billion rows by two divisions).

A ticket for TanStack is drafted in
`docs/upstream/tanstack-virtual-count-growth.md`. Its script, without a
browser, grows a list by 5,000 items up to 1,000,000: the 200 updates take
1.7 to 3.5 s, the last one 27 to 38 ms (`@tanstack/virtual-core` 3.19.9,
three runs).

Last rows received, 1 million rows, `run.mjs --interface`, the order of the
builds reversed every other round (database alone 3.1 to 5.3 s):

| Build | Round 1 | Round 2 | Round 3 |
|---|---|---|---|
| Interface before the changes | 37.8 s | 23.5 s | 36.1 s |
| Interface with the changes | 11.8 s | 13.5 s | 9.0 s |

| Measure | Before the changes | With the changes |
|---|---|---|
| `done` received | 28.2 to 47.3 s | 17.0 to 22.4 s |
| Main thread busy | 23.7 to 38.4 s | 9.1 to 12.3 s |
| Long tasks: number | 36 to 181 | 5 to 6 |
| Long tasks: total, longest | 2.6 to 13.3 s, 0.20 to 0.43 s | 0.5 to 0.7 s, 0.20 to 0.26 s |
| Memory of the page after the reception | 164 MB | 155 MB |

Profile with the changes, 1 million rows (last rows after 16.2 s, main
thread busy 16.2 s): work of the browser outside JavaScript 6.2 s (39 %),
`parseMessage` 4.6 s (28 %), updates of Vue 2.1 s (13 %), `utf8Length`
1.7 s (10 %), garbage collection 1.1 s (7 %), `updateView` 0.05 s;
`getMeasurements` is gone.

P5, measured on 2026-11-01 against the build with the other changes, three
rounds, the order of the builds reversed every other round (database alone
3.1 to 7.4 s):

| Build | Round 1 | Round 2 | Round 3 |
|---|---|---|---|
| Last rows, other changes | 11.5 s | 9.8 s | 10.4 s |
| Last rows, with P5 too | 11.6 s | 10.5 s | 9.6 s |
| Scripts of the page, other changes | 5.27 s | 5.47 s | 5.46 s |
| Scripts of the page, with P5 too | 4.92 s | 4.77 s | 4.99 s |

The last rows stay within the spread of the measures (main thread busy 10.0
to 10.7 s without P5, 9.7 to 10.6 s with it); the scripts of the page run
0.35 to 0.70 s less in each round. Profile with P5, 1 million rows (last
rows after 14.7 s, main thread busy 13.9 s): work of the browser outside
JavaScript 5.6 s (40 %), `parseMessage` 4.6 s (33 %), updates of Vue 1.9 s
(14 %), garbage collection 0.7 s (5 %), `utf8Length` 0.7 s (5 %), of which
`encodeInto` 0.6 s, against 1.7 s for the loop. `parseMessage` is now the
first function of the page.

Checked in Chrome, at 1,400 by 900 pixels then 1,400 by 1,400: on a result
of 5,000 rows, at the top, in the middle, at the bottom, after the window
grew and after a fast scroll by the wheel, every visible row is drawn, with
33 to 65 rows in the page; during the reception of 1 million rows, the bar
of the tab is scaled to 10.7 % after 2.5 s.

Limit seen, the same before and after the changes: Chrome caps the height of
an element at 33,554,428 pixels. At 24 pixels per row (a row of `2rem`, the
root at 75 % of 16 pixels), the grid scrolls down to row 1,398,101 at most:
the rows beyond are received and counted, and cannot be reached by
scrolling.

### 12.6 Takeaways

- **Profile by function before guessing**: the trace of chapter 11 gave
  "updates of Vue" and "layers"; the profile named the virtualizer and the
  progress bar.
- **A dependency can be quadratic in a case it was not tried in**: a list
  that grows while it is shown.
- **An animation of a size draws at every frame**: animate a transform or an
  opacity.
- **On a laptop, compare builds in turns**: the same measure varies by a
  third from one run to the next.
- **A profile of a plain build needs its own styles**: the scoped styles of
  Vue are named after the source of their component, so the styles of
  another build leave a changed component without its styles.

### 12.7 Measuring again

The development servers started (`docker compose up --detach --wait`) and
the interface built (`npm run build` in `web/`), from the root of the
repository: `node bench/store/profile.mjs` for the profile (1 million rows),
`node bench/store/run.mjs --interface` for the reception by the interface
alone. To compare builds, each one copied aside once built, then
`PRISME_EXECUTABLE=<build> node bench/store/run.mjs --interface`, in turns.
The profile goes to `target/store-bench/profile-<rows>.cpuprofile`, which the
Performance panel of DevTools opens.

## 13. Parser of the Pivoteur: sqlparser compiled to WebAssembly

Benchmark of 2026-11-01, step 3 of `docs/TODO-PIVOTER.md`.

### 13.1 Question

The Worker of the Pivoteur parses the expressions of its virtual columns and
of its measures, written in its own dialect, in the engine compiled to
WebAssembly (chapter 9). Prisme already parses SQL with `sqlparser`
(`crates/dialects`, version pinned at 0.63.0). How much does `sqlparser` weigh
in a WebAssembly module, and how long does a parse take there?

### 13.2 Measurement

- `node bench/parser/run.mjs` (`docs/DEVELOPER.md`, section "Parser of the
  Pivoteur"): a module holding `sqlparser` 0.63.0 alone, with the features of
  Prisme (`visitor`) and its generic dialect, target `wasm32-unknown-unknown`;
  it parses one expression, and refuses a text with tokens left after it.
- Two Cargo profiles: `release`, the profile of Prisme (Cargo's default), and
  `lto`, the same with `lto = true` and `codegen-units = 1`.
- Each module under Node 26.7.0 (V8), in a process of its own: the first
  `WebAssembly.compile`, the instantiation, the first parse (`SUM(montant)`),
  the first parse in a second module compiled from the same bytes, then each
  expression, best of 5 rounds of 1,000 parses after 1,000 parses of warm-up.
- Same machine as chapter 9: Intel Core Ultra 7 155H (22 logical cores),
  33.8 GB, Windows 11, Rust 1.99.1. Three runs: the sizes are the same in
  each, the times are given as the range of the three.

| Measure | `release` | `lto` |
|---|---|---|
| Module | 4,532.2 kB | 2,386.8 kB |
| gzip, level 9 | 1,100.9 kB | 586.6 kB |
| Brotli, quality 11 | 561.6 kB | 395.7 kB |
| Compilation | 10.1 to 12.6 ms | 6.8 to 8.2 ms |
| Instantiation | 0.5 to 0.6 ms | 0.2 to 0.3 ms |
| First parse | 11.4 to 12.5 ms | 13.4 to 14.7 ms |
| First parse in a module compiled again from the same bytes | 0.14 to 0.42 ms | 0.12 to 0.14 ms |
| Memory of the module after the parses | 3.6 MB | 3.5 MB |

Parse of each expression, in microseconds:

| Expression | Bytes | `release` | `lto` |
|---|---|---|---|
| `SUM(montant)` | 12 | 8.3 to 10.0 | 10.8 to 16.1 |
| `SUM(montant) / NULLIF(SUM(qte), 0)` | 34 | 18.7 to 21.7 | 13.3 to 18.0 |
| `CONCAT(EXTRACT(YEAR FROM d), '-T', QUARTER(d))` | 46 | 17.0 to 20.5 | 13.4 to 16.8 |
| `CASE WHEN qte >= 10 THEN 'gros' WHEN qte > 0 THEN 'petit' ELSE 'aucun' END` | 74 | 16.4 to 19.0 | 13.2 to 14.4 |
| `COUNT(*) FILTER (WHERE canal = 'web')` | 37 | 9.6 to 11.3 | 6.9 to 8.6 |
| `LISTAGG(produit, ', ') WITHIN GROUP (ORDER BY produit)` | 54 | 12.8 to 16.8 | 9.3 to 11.3 |
| `CAST(d AS DATE)` | 15 | 4.9 to 5.7 | 3.3 to 4.2 |
| `c1 + c2 + … + c200` | 1,289 | 265.7 to 595.2 | 264.9 to 283.9 |
| `SUM(montant`, refused | 11 | 4.0 to 13.7 | 4.2 to 6.1 |
| `SUM(montant) montant`, refused | 20 | 4.4 to 16.3 | 5.1 to 6.6 |

The generic dialect accepts every expression of the Pivoteur measured,
`FILTER` and `LISTAGG … WITHIN GROUP` included, and refuses the two wrong
texts.

### 13.3 Analysis

- **A parse takes tens of microseconds**, and grows with the text: 0.27 to
  0.6 ms for a sum of 200 columns. Parsing at each key typed stays far below
  a frame (16.7 ms).
- **The first parse costs 11 to 15 ms**: V8 compiles the functions of a
  module on their first call (`--wasm-lazy-compilation`, on by default). A
  module compiled again from the same bytes in the same process reuses them
  (`--wasm-native-module-cache`, on by default): its first parse takes 0.1 to
  0.4 ms.
- **The optimizations across crates halve the module**: 4.5 MB to 2.4 MB,
  and 1.1 MB to 0.59 MB in gzip, with a compilation shorter by a third. The
  parses differ less between the two profiles than from one run to the next.
- The module reaches the page from the executable of Prisme, on the same
  machine (section 9.4): its size weighs on the executable and on the loading
  of the Worker.

### 13.4 Consequences

- `sqlparser` parses the dialect of the Pivoteur in the Worker, as the
  specification states since step 3.
- The profile of the module of the engine, `lto` or the one of Prisme, is
  chosen with lot 1, once the engine and its parser form one module.

### 13.5 Takeaways

- **The first call of a WebAssembly function pays its compilation**: it is
  measured apart from the others.
- **V8 reuses a module compiled in the same process**: each module is
  measured in a process of its own, for its first parse to start cold.

### 13.6 Measuring again

From the root of the repository: `node bench/parser/run.mjs` (a few minutes
for the first build). The command adds the WebAssembly target of the
toolchain when missing, builds the module in both profiles, checks that each
one accepts the expressions of the Pivoteur and refuses the wrong texts,
prints the table and writes the results to `target/parser-bench/results.json`.
