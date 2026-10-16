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
