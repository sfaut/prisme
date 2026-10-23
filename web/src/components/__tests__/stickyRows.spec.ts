import { describe, expect, it } from 'vitest'

import type { NavigatorNode } from '../navigatorNodes'
import { pinnedRows, type VisibleRows, visibleRows } from '../stickyRows'

const ROW_HEIGHT = 24

/**
 * Node of the test tree.
 *
 * @param key Key of the node.
 * @param label Displayed text of the node.
 * @param children Children of the node, none if omitted.
 */
function node(key: string, label: string, children?: NavigatorNode[]): NavigatorNode {
  return {
    key,
    label,
    isLeaf: !children?.length,
    connectionId: 1,
    schema: 'a',
    children,
  }
}

/**
 * Tree of the tests: a source with a database that holds a table of ten
 * columns and an index folder, then a second database.
 */
const columns = Array.from({ length: 10 }, (_, index) => node(`col:${index + 1}`, `c${index + 1}`))
const roots = [
  node('sources', 'Sources', [
    node('source:1', 'Origen', [
      node('schema:1/a', 'a', [
        node('folder:tables', 'Tables', [
          node('table:t', 't', [
            node('folder:cols', 'Colonnes', columns),
            node('folder:indexes', 'Index'),
          ]),
        ]),
      ]),
      node('schema:1/b', 'b'),
    ]),
  ]),
]
const everythingOpen = [
  'sources',
  'source:1',
  'schema:1/a',
  'folder:tables',
  'table:t',
  'folder:cols',
]

/**
 * Labels of the pinned rows.
 *
 * @param pinned Pinned rows.
 */
function labels(pinned: { node: NavigatorNode }[]): string[] {
  return pinned.map((row) => row.node.label)
}

/**
 * Pinned rows for a scroll position, in rows.
 *
 * @param visible Rows displayed by the tree.
 * @param rows Scroll position, in rows (may be fractional).
 */
function pinnedAt(visible: VisibleRows, rows: number) {
  return pinnedRows(visible, rows * ROW_HEIGHT, ROW_HEIGHT)
}

describe('visibleRows', () => {
  it('shows only the roots and the children of the open nodes', () => {
    const { rows } = visibleRows(roots, ['sources'])
    expect(rows.map((row) => row.node.label)).toEqual(['Sources', 'Origen'])
    expect(rows.map((row) => row.level)).toEqual([1, 2])
    expect(rows.map((row) => row.parent)).toEqual([null, 0])
  })

  it('lists the rows in display order, with their depth, their parent and their section', () => {
    const { rows } = visibleRows(roots, everythingOpen)
    expect(rows).toHaveLength(18)
    // Row 6 is the first column, below the folder of the columns (row 5).
    expect(rows[6]).toMatchObject({ level: 7, parent: 5, end: 6 })
    // The columns folder ends with the last column (row 15); the table holds
    // the index folder (row 16) after it.
    expect(rows[5]).toMatchObject({ level: 6, parent: 4, end: 15 })
    expect(rows[4]).toMatchObject({ level: 5, parent: 3, end: 16 })
    // The index folder is closed: its (absent) children do not show.
    expect(rows[16]).toMatchObject({ level: 6, parent: 4, end: 16 })
    expect(rows[17]).toMatchObject({ level: 3, parent: 1, end: 17 })
  })

  it('groups the indexes of the rows by level', () => {
    const { byLevel } = visibleRows(roots, everythingOpen)
    expect(byLevel[1]).toEqual([0])
    expect(byLevel[3]).toEqual([2, 17])
    expect(byLevel[6]).toEqual([5, 16])
  })
})

describe('pinnedRows', () => {
  const visible = visibleRows(roots, everythingOpen)

  it('pins nothing at the top of the tree', () => {
    expect(pinnedAt(visible, 0)).toEqual([])
  })

  it('pins the root as soon as it leaves the top, in full', () => {
    // A tenth of a row: the root is pinned, the source, the database and the
    // table are already above their slots, and the column folder too.
    expect(labels(pinnedAt(visible, 0.1))).toEqual([
      'Sources',
      'Origen',
      'a',
      'Tables',
      't',
      'Colonnes',
    ])
  })

  it('pins the chain of the rows above the top of the tree, from the root down', () => {
    // Scrolled by five rows: the columns folder is the first row under the
    // pinned ones, and its six ancestors (itself included) fill the six slots.
    const pinned = pinnedAt(visible, 5)
    expect(labels(pinned)).toEqual(['Sources', 'Origen', 'a', 'Tables', 't', 'Colonnes'])
    expect(pinned.map((row) => row.level)).toEqual([1, 2, 3, 4, 5, 6])
  })

  it('keeps the source pinned while its section lasts, and releases the columns first', () => {
    // The last column is at the top: the columns folder and the table no longer
    // reach their slot, but the root and the source still do.
    expect(labels(pinnedAt(visible, 15))).toEqual(['Sources', 'Origen'])
    // The table is released once its last line (the index folder) reaches the top.
    expect(labels(pinnedAt(visible, 11))).toEqual(['Sources', 'Origen', 'a', 'Tables', 't'])
  })

  it('releases the columns folder once its last column reaches the top', () => {
    // Ten rows down, the last column is still below the top: the folder stays.
    expect(labels(pinnedAt(visible, 10))).toContain('Colonnes')
    // Eleven rows down, the last column is above the top: the folder leaves.
    expect(labels(pinnedAt(visible, 11))).not.toContain('Colonnes')
  })

  it('pins nothing past the end of the rows', () => {
    expect(pinnedAt(visible, 100)).toEqual([])
  })

  it('never pins a row without children', () => {
    // Row 11, the sixth column, is above its slot at five rows and a half
    // (slot 6), but a column has nothing below it to keep in view.
    expect(labels(pinnedAt(visible, 5.5))).not.toContain('c6')
  })
})
