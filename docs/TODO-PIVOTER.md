# Pivoteur: progress

Steps of the work on the Pivoteur, the local pivot table under the result grid. Each step is validated with the maintainer before it starts; its boxes are ticked as the work is done (⬜ to do, ✅ done), and the maintainer validates its result before the next step starts. Specification: "Cahier des charges du Pivoteur de Prisme" (Claude Docs, in French), https://claude.ai/artifact/UvfFRfmTaFLZ1d7kDPkDSD.

## 1. Review of the specification

- ✅ Read the specification and check its references against the code
- ✅ Settled: a dialect of the Pivoteur's own, close to MySQL, MariaDB and the others; the Pivoteur works on the result set as it is; the panel stays under the grid; the federated engine is set aside for now
- ✅ Panel in three columns: fields, stacked zones, pivot table

## 2. Engine benchmark (point A1)

- ⬜ Data generator shared by Rust and JavaScript (same pseudo-random generator, fixed seed)
- ⬜ Rust benchmark, on one thread and on all cores
- ⬜ JavaScript benchmark under Node (V8)
- ⬜ Same checksums on both sides
- ⬜ Runs on 10 million rows, both profiles, with memory
- ⬜ Results submitted to the maintainer
- ⬜ Engine decided (A1)

## 3. Corrections of the specification

- ⬜ Expression language: the Pivoteur's dialect, conversions (`CONCAT`, date literals), invalid data
- ⬜ New section on the cases that diverge from one database to another, each to be decided
- ⬜ Catalog: `CAST(x AS DATE)`, `CAST(x AS DATETIME)`
- ⬜ Conformance: reference file, binary collation, functions absent from MySQL or MariaDB
- ⬜ A1 with the benchmark, and the heading of the points to decide
- ⬜ Mockup of the panel in three columns, search in the field list (F-10)
- ⬜ Minor fixes: "clefs" (C-03), N-03, TypeScript block in English, checks and expected outputs of the acceptance, N-14, sources

## 4. Lots of the specification

- ⬜ Lot 1: engine
- ⬜ Lot 2: panel
- ⬜ Lot 3: complements
