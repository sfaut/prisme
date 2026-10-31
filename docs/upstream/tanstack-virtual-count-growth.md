# TanStack Virtual: each new `count` measures the whole list again

Draft of a ticket for [TanStack/virtual](https://github.com/TanStack/virtual/issues), following its bug
template, to post from the maintainer's account. The result grid of Prisme placed its rows with
`@tanstack/vue-virtual` until 2026-10-31: during the reception of 1 million rows, by batches every
100 ms, `getMeasurements` took 7.6 s of the 32 s of work of the main thread (`docs/PERFORMANCES.md`,
chapter 12). All the rows of the grid have the same height: the grid now computes the rows to draw
itself (`drawnRange` in `web/src/components/resultGrid.ts`), and needs the virtualizer no more,
whatever the answer to the ticket. The tree of the navigator keeps it, through the `TreeVirtualizer`
of Reka UI: its count changes when a node opens, not by batches.

---

**Title:** `getMeasurements` goes through every item again whenever `count` grows: a list appended by batches costs the square of its size

### Describe the bug

When only `count` changes, `getMeasurementOptions` gives new options and resets `pendingMin` to
`null`, so `getMeasurements` starts again from index 0: it calls `getItemKey` and `estimateSize` for
every item, allocates a new `items` array of `count` keys and, as the count grows, a new
`Float64Array` of `2 * count` numbers. An update that appends 5,000 items to a list of 1,000,000 costs
as much as building the whole list.

A list fed by a stream (rows of a query result arriving every 100 ms, log lines, messages) grows by
batches: the total cost grows with the square of the final count. In Prisme, a SQL client that shows
the rows of a result as they arrive, `getMeasurements` took 7.6 s of the 32 s of work of the main
thread while 1,000,000 rows were received (Chrome profile, sampling every 200 µs); for 200,000 rows,
0.40 s: 5 times the rows, 19 times the time.

Keeping `pendingMin` at the previous count would not be enough for the single-lane path:
`this._singleLaneMeasurements.items.slice()` copies the keys of the items already measured, and the
`Float64Array` grows to exactly `2 * count`, so it is allocated and copied again at each new count.

### Your minimal, reproducible example

A Node script, without a framework nor a DOM: the list grows by 5,000 items up to 1,000,000, and each
update reads the total size, as a framework adapter does on render.

```js
import { Virtualizer } from '@tanstack/virtual-core'

const BATCH = 5_000
const TOTAL = 1_000_000

const virtualizer = new Virtualizer({
  count: 0,
  getScrollElement: () => null,
  estimateSize: () => 24,
  scrollToFn: () => {},
  observeElementRect: () => {},
  observeElementOffset: () => {},
})

let total = 0
for (let count = BATCH; count <= TOTAL; count += BATCH) {
  virtualizer.setOptions({ ...virtualizer.options, count })
  const start = performance.now()
  virtualizer.getTotalSize()
  const spent = performance.now() - start
  total += spent
  if ([100_000, 500_000, 1_000_000].includes(count)) {
    console.log(`update at ${count} items: ${spent.toFixed(2)} ms`)
  }
}
console.log(`all ${TOTAL / BATCH} updates: ${(total / 1000).toFixed(2)} s`)
```

Output with `@tanstack/virtual-core` 3.19.9, Node 26.7.0, Intel Core Ultra 7 155H:

```
update at 100000 items: 2.93 ms
update at 500000 items: 15.97 ms
update at 1000000 items: 26.68 ms
all 200 updates: 3.21 s
```

Over three runs: 1.7 to 3.5 s for all the updates, 27 to 38 ms for the update at 1,000,000 items.
In a browser, an adapter that updates `count` more often (at each frame of a reception, for example)
pays it more often: 7.6 s in the profile above.

### Steps to reproduce

1. Save the script as `repro.mjs` next to an install of `@tanstack/virtual-core`.
2. Run `node repro.mjs`.
3. See the time of one update grow with the count, and the time of all the updates with its square.

### Expected behavior

An update that only appends items costs in proportion to the items appended: the measurements of
the items below the previous count are still valid (same `getItemKey`, same `estimateSize`, nothing
measured in between). For example, when `count` is the only option that changed, `pendingMin` could
stay at `min(previous count, count)`, the `Float64Array` could grow by doubling its capacity, and the
array of keys could be extended in place.

### How often does this bug happen?

Every time

### Platform

- OS: Windows 11 Pro 10.0.26300
- Browser: Chrome 156 (the application), Node 26.7.0 (the script above)

### tanstack-virtual version

`@tanstack/virtual-core` 3.19.8 (through `@tanstack/vue-virtual` 3.13.49) in the application;
3.19.9 for the script, whose `getMeasurementOptions` and `getMeasurements` are the same.

### TypeScript version

v6.0.3

### Additional context

Related: [#565](https://github.com/TanStack/virtual/issues/565), on the arrays of `count` items that
`getMeasurements` allocates. Our workaround: the rows of our grid all have the same height, so the
rows to draw come from the scroll offset divided by this height, at a cost that does not depend on
the count.
