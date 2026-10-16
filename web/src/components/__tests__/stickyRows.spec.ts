import { describe, expect, it } from 'vitest'

import type { ExplorerNode } from '../explorerNodes'
import { pinnedRows, visibleRows } from '../stickyRows'

const ROW_HEIGHT = 24

/**
 * Node of the test tree.
 *
 * @param key Key of the node.
 * @param label Displayed text of the node.
 * @param children Children of the node, none if omitted.
 */
function node(key: string, label: string, children?: ExplorerNode[]): ExplorerNode {
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
 * Labels of rows.
 *
 * @param rows Rows to read.
 */
function labels(rows: { node: ExplorerNode }[]): string[] {
  return rows.map((row) => row.node.label)
}

describe('visibleRows', () => {
  it('shows only the roots and the children of the open nodes', () => {
    const rows = visibleRows(roots, ['sources'])
    expect(labels(rows)).toEqual(['Sources', 'Origen'])
    expect(rows.map((row) => row.level)).toEqual([1, 2])
    expect(rows.map((row) => row.parent)).toEqual([null, 0])
  })

  it('lists the rows in display order, with their depth and their parent', () => {
    const rows = visibleRows(roots, everythingOpen)
    expect(rows).toHaveLength(18)
    // Row 6 is the first column, below the folder of the columns (row 5).
    expect(rows[6]).toMatchObject({ level: 7, parent: 5 })
    expect(rows[5]).toMatchObject({ level: 6, parent: 4 })
    // The index folder is closed: its (absent) children do not show.
    expect(rows[16]).toMatchObject({ level: 6, parent: 4 })
    expect(rows[17]).toMatchObject({ level: 3, parent: 1 })
  })
})

describe('pinnedRows', () => {
  const rows = visibleRows(roots, everythingOpen)

  it('pins nothing at the top of the tree', () => {
    expect(pinnedRows(rows, 0, ROW_HEIGHT)).toEqual([])
  })

  it('pins the ancestors of the first row under the pinned rows', () => {
    // Scrolled by five rows: the columns folder is the first row, its six
    // ancestors and the first column line are the pinned rows, hidden under
    // them at the top of the tree.
    const pinned = pinnedRows(rows, 5 * ROW_HEIGHT, ROW_HEIGHT)
    expect(labels(pinned)).toEqual(['Sources', 'Origen', 'a', 'Tables', 't', 'Colonnes'])
    expect(pinned.map((row) => row.level)).toEqual([1, 2, 3, 4, 5, 6])
  })

  it('uses the row at the scroll position, not the rows of the pinned height', () => {
    // A position between two rows is the row above it.
    expect(labels(pinnedRows(rows, 5 * ROW_HEIGHT + 10, ROW_HEIGHT))).toEqual(
      labels(pinnedRows(rows, 5 * ROW_HEIGHT, ROW_HEIGHT)),
    )
  })

  it('pins nothing when no count of pinned rows settles', () => {
    // The last column is at the top: six ancestors would be pinned, which
    // hides the rows that follow the columns and changes the count to zero
    // (no row there), then back to six. Nothing is pinned.
    expect(pinnedRows(rows, 15 * ROW_HEIGHT, ROW_HEIGHT)).toEqual([])
  })

  it('pins nothing past the end of the rows', () => {
    expect(pinnedRows(rows, 100 * ROW_HEIGHT, ROW_HEIGHT)).toEqual([])
  })
})
