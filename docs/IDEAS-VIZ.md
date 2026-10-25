# Graph of a database and its relations

This document gathers the ideas kept for drawing a database as a graph: its tables, and the relations given by their foreign keys. Nothing is implemented. Settled points are marked **[decided]**, the others stay open (section 8). Section 5 keeps the measurements of a trial of vis-network, on 2026-10-23.

## 1. Starting point

- The catalog already holds what the graph needs. For a database, `details()` (`crates/connectors/src/mysql/catalog.rs`) reads in one go the columns, indexes and foreign keys of all its tables; each `ForeignKeyInfo` gives its columns, the referenced table, its database (`referenced_schema`) and the referenced columns.
- The row count of the tables is not read: `TABLE_ROWS` would force the server to open each table, which is very slow on a large server (module comment of `catalog.rs`). Sizing the tables by their volume therefore needs a dedicated read (section 8).
- Relations come from the declared foreign keys only **[decided]**. Relations guessed from the names (`user_id`), or tables without foreign keys (MyISAM), are left for later.

## 2. The graph

- **A node per table**, sized by its volume, colored by its database.
- **An arrow per foreign key**, from the referencing table to the referenced one. A foreign key to another database is dashed.
- **Folded / unfolded mode** **[decided]**: a folded table is a disc with its name; an unfolded table shows its columns, primary key and foreign keys marked, with their types.
- **Cardinalities**, read from the catalog without approximation: a nullable foreign key gives 0..1 on the parent side, a foreign key covered by a `UNIQUE` index gives 1:1, otherwise 1:N.
- **Databases as clusters** **[decided]**: each database has its color, its tables gather in one region, and a delimiter (hull) surrounds them, with the database's name. A table of another database reached by a foreign key is shown inside its own database's hull.
- **Tables without relations**: kept out of the simulation, gathered in a separate area under the graph ("tables without relations"). In the simulation, an isolated table is either ejected far away or kept by a gravity that distorts the rest (section 5.3).

## 3. Scope: everything or a neighbourhood

The scope is a setting **[decided]**:

- **Everything**: all the tables of the database (or of several databases).
- **Exploration**: from a table, the tables within N foreign-key hops, in both directions, N adjustable. The next ring (N+1) is shown faded, as a frontier: it tells that the graph goes on there. Clicking a frontier table opens it and widens the exploration step by step.

Beyond about 1,000 tables, the exploration is the usable mode (section 5.6).

## 4. Expected behaviour: an "organic" graph

The graph is alive: it organises itself, reorganises when handled, and its forces carry meaning.

- **Weight**: the big tables look bigger, repel the other big tables, and attract the small tables that depend on them. Small reference tables (`country`, `currency`, `carrier`) settle right against their table.
- **Handling**: a table can be grabbed and moved; during the move, its relations are highlighted and the graph follows. Once dropped, the table stays where it was put (pinned, marked by a dashed border); a double click releases it.
- **Positions saved** **[decided]**: positions moved by hand are saved in the internal database, per graph (database or exploration). On reopening, saved positions are restored and only new tables are placed by the simulation.
- **Reproducible layout**: the same schema gives the same layout, through a seeded random source for the initial positions.

## 5. Trial of vis-network (2026-10-23)

Trial on the vis-network example "Playing with Physics" (`visjs.github.io/vis-network/examples/network/physics/physicsConfiguration.html`), its data replaced by a realistic schema: 37 tables over two databases (`shop`, `crm`), with volumes from 10 to 60 million rows, self references (`category`, `employee`), foreign keys between the databases and two tables without relations (`audit_log`, `setting`). Each setting was measured after stabilisation: mean length of the edges to a small table (fewer than ~3,000 rows), mean distance between the six biggest tables, overlaps, edge crossings, and residual motion one second after stabilisation.

### 5.1 What vis-network brings

- Dragging, zooming, selection, tooltips and highlighting of the relations of the selected table (in red during a drag), all built in.
- Node sizes through `value` and `scaling`, groups with their colors, dashed edges, `beforeDrawing` / `afterDrawing` hooks to draw on the canvas (database hulls), custom shapes (`ctxRenderer`).
- Stabilisation before display, then a live simulation when a table is moved.

### 5.2 Mass: double-edged

The vis-network `mass` of a node multiplies the repulsion it exerts. A mass growing with the volume gives the expected effect (big tables apart), but:

- with a mass in the square of the volume's logarithm, repulsion between big tables (16 × 16 = 256 times the normal one) overwhelms the springs: the graph becomes a uniform disc, without families;
- a mass proportional to the logarithm of the volume, with a floor of 1 (`1 + log10(rows) / 2`), gives the best trade-off.

### 5.3 Central gravity and isolated tables

With `barnesHut`, central gravity is a constant force, independent of the mass, while the repulsion of the rest of the graph grows with its total mass. A heavy table without relations (`audit_log`) settles about 1,000 px away, and the fit to the screen crushes the rest. `forceAtlas2Based` multiplies gravity by mass and degree and keeps it, but see section 5.4. Hence the separate area for tables without relations (section 2).

### 5.4 Convergence

- `forceAtlas2Based` never stabilises as soon as it is pushed: with stiff springs or `avoidOverlap`, the graph turns on itself endlessly (no drift of its centre, but 250 px/s at its periphery). Its repulsion between two nodes depends on the degree of only one of them: forces are not symmetric, and the resulting torque makes the graph spin. Neither a mass floor nor a smaller time step (0.25, 0.1) stops it.
- `avoidOverlap` > 0 prevents convergence, whatever the solver.
- `barnesHut` without `avoidOverlap` stabilises fully (zero residual motion), even with stiff springs (`springConstant` 0.6).
- Best result with vis-network's own physics: `barnesHut`, `gravitationalConstant` −3000, `centralGravity` 0.3, `springConstant` 0.6, `damping` 0.3, mass `1 + log10(rows) / 2`, edge length `20 + 14 × log10(rows of the smaller end)^1.3`. Stable in 465 ms; big tables 3.4 to 7.7 times further from each other than from their satellites.

### 5.5 The limit: a closed physics

vis-network's forces are fixed:

- a single stiffness (`springConstant`) for all edges, per-edge length only;
- no group force: gathering a database's tables needs an invisible "anchor" node per database, tied to its tables by springs as stiff as the foreign keys. With long anchor springs, the database becomes a ring, its tables piled on the rim; with short ones, a heap (26 overlaps);
- no stable collision force (`avoidOverlap` breaks convergence).

### 5.6 Hybrid: d3-force for the physics, vis-network for drawing

vis-network's physics turned off, positions computed by `d3-force` (3.0.0) and copied to vis-network's nodes at each tick:

- **Forces per element**: link distance `r_a + r_b + 10 + 3 × r_min` and strength `0.7 / min(degree)`; repulsion `−80 × (r / 8)²` (big tables repel much more); collision on the radius + 10 px (no overlap of discs); `forceX` / `forceY` towards a centre per database, strength 0.06 to 0.08 (clusters without distortion).
- **Radius** `4 + rows^0.2`: 250 rows → 7 px, 40,000 → 12 px, 8 million → 28 px, 25 million → 34 px. The logarithm flattened the differences too much (`city`, 40,000 rows, looked as big as `order`, 8 million).
- **Result**: families appear on their own (geography, catalogue, orders and payments, `crm` apart); no overlap; 10 crossings for 47 edges; databases separated, their hulls without overlap; tables bridging two databases (`employee`) sit on the edge of their hull, facing the other database.
- **Computation**: 400 ticks in 25 to 35 ms for 37 tables, computed at once (`sim.tick()`) or animated.
- **Dragging**: vis-network's `dragStart` / `dragging` / `dragEnd` events drive `fx` / `fy` of d3. vis-network emits `dragging` before moving the node: the position must be computed from `pointer.canvas` plus the offset taken at `dragStart`, never read from the node.

Load (generated schemas, tables spread over 4 databases):

| Tables | Simulated | Without relations | 300 ticks | Per tick | One frame |
|---|---|---|---|---|---|
| 300 | 171 | 129 | 194 ms | 0.65 ms | 10 ms |
| 1,000 | 578 | 422 | 902 ms | 3 ms | 30 ms |
| 3,000 | 1,791 | 1,209 | 7,156 ms | 24 ms | 335 ms |

Up to about 1,000 tables, everything can be shown and animated (about 30 frames per second). At 3,000, vis-network's canvas drawing falls behind: exploration by depth, or a WebGL rendering. The area of tables without relations must wrap as a grid, or fold into one block ("1,209 tables without relations").

### 5.7 Unfolded tables in vis-network

A custom shape (`ctxRenderer`) draws an unfolded table: header with its name and volume, columns with primary key, foreign keys and types. Limits:

- arrows reach the border of the block, towards its centre, never a column row: linking `customer_id` to `customer.id` is impossible;
- the collision stays circular around a rectangular block;
- the block is painted on the canvas: it escapes the CSS theme (rule 10 of `CLAUDE.md`), and its columns can be neither hovered nor selected.

## 6. Rendering: options

| Option | For | Against |
|---|---|---|
| vis-network + d3-force | Interaction built in, canvas fast up to ~1,000 tables, trial done. | Unfolded tables painted on the canvas, out of the theme; arrows towards the centre of the blocks; colors to read from the CSS variables. |
| Vue Flow (`@vue-flow/core`) + d3-force | Nodes as Vue components (theme, Tailwind, light / dark); one handle per column, hence arrows from column to column; pan, zoom, drag built in. | Performance of hundreds of DOM nodes moved at each tick, to measure. |
| Own SVG or canvas + d3-force | Full control, no dependency beyond `d3-force`. | Pan, zoom, drag, selection to write. |
| sigma.js + graphology | WebGL, thousands of nodes, correct ForceAtlas2. | Unfolded tables hard to draw; another physics to tune. |

`d3-force` is the physics in every case: it is the only one tried whose forces can be set per element (section 5.6).

## 7. Exports

The graph can be exported as an image **[decided, to note]**: SVG (vector, selectable text) and PNG, following the principles of `docs/IDEAS-EXPORTS.md` (section 2.1): drawing with the theme's variables, sharpness through `devicePixelRatio`. vis-network draws on a canvas: PNG through `canvas.toBlob()`; SVG would need its own rendering from the positions.

## 8. Open questions

1. **Volume of the tables**: `TABLE_ROWS` for MySQL / MariaDB **[decided]**, read with the other objects of the catalog, shown as an estimate (« ≈ N lignes »). Its cost when the tables are cold is measured in section 9.2. Later: `reltuples` (PostgreSQL, an estimate), `COUNT(*)` (SQLite, exact but costly).
2. **Rendering**: Vue Flow + d3-force, prototype built (section 9); its performance on hundreds of tables is still to measure before deciding.
3. **Place in the interface**: the « Visualiseur d’objets », in the right sidebar, in place of the data explorer **[decided]**; it shows the active source.
4. **Saved positions**: per database, per exploration, per user? What happens to a saved position when a table is dropped or renamed?
5. **Settings shown to the user**: depth, scope, forces (repulsion, cluster strength), or only a few presets?
6. **Cardinalities on the edges**: crow's feet, labels (`0..1`, `1:N`), or only on hover?
7. **Large schemas**: from how many tables does the graph refuse "everything" and offer the exploration?

## 9. Prototype: Vue Flow + d3-force (2026-10-23)

The prototype lives on the branch `object-visualizer`, apart from `master` until it is accepted.

### 9.1 What it does

The « Visualiseur d’objets » (`web/src/components/ObjectVisualizer.vue`) shares the right sidebar with the data explorer: one or the other is open, each with a button to switch to the other. It shows the tables of the active source, system databases left out.

- **Tables**: colored by database (`--graph-1` to `--graph-6`), in one of two formats, switched by a button of the toolbar:
  - label (by default): the name inside a rounded box, its text scaled by `sqrt(r / 8)`, where `r` is the radius of the disc below (1 to 2.8 times the small size);
  - disc: a radius that grows with the number of rows (`8 + rows^0.2`, capped at 64; 12 when unknown), the name under it.

  A double click unfolds a table into a card of its columns (primary and foreign keys marked), and folds it back.
- **Weight and box**: the weight of a table (the radius of its disc) sets the repulsion and the length of its springs; its box on screen (disc, label or card, measured once drawn, estimated before) sets the collision, rectangular (`forceBoxCollide`, a sweep from left to right). A collision by circles kept wide labels as far apart vertically as horizontally, and a repulsion by the box spread the graph out.
- **Relations**: one arrow per foreign key, from the referencing table to the referenced one; from the column itself when the table is unfolded; dashed between two databases.
- **Databases**: each one pulls its tables towards its own centre, and gets an outline (convex hull) with its name, when there are several.
- **Layout**: the forces of section 5.6 (`schemaLayout.ts`), computed at once before the first display, then alive while a table is moved. Tables without relations form a grid under the graph.
- **Interaction**: a table dropped stays where it is (pinned, dashed border); its pin button frees it. The places are kept for the life of the page, per source.
- **Scope**: everything, or an exploration from one or more tables (search field, or the selection) up to a depth set with − and +. The tables one step beyond are shown dimmed; a click on one adds it to the exploration. Beyond 1,000 tables, only the exploration is offered.

### 9.2 Cost of `TABLE_ROWS`

Measured on 2,000 InnoDB tables (MySQL 8.4.11, MariaDB 10.6.28 and 11.4.13): 20 to 40 ms when the tables are warm, as for the names alone; 3 to 8 s cold (after a restart). MariaDB then opens each table (`Open_full_table` instead of `Open_frm_only`), and does so on every read when the database has more tables than `table_open_cache` (146 to 226 ms with a cache of 400), evicting the server's own cache. MySQL 8.4 opens nothing but writes its statistics cache (`information_schema_stats_expiry`, 24 h by default): its values can be a day old; `SET SESSION information_schema_stats_expiry = 0` gives fresh values (140 ms cold), the `SET_VAR` hint being refused (warning 3637). Views give `NULL`. The estimates measured are close to the real counts.

### 9.3 Not done yet

- Positions saved in the internal database (section 4): they last as long as the page.
- Cardinalities on the edges, exports (section 7).
- Performance with hundreds of tables moved at each step of the simulation.
