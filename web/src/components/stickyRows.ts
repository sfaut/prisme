/**
 * Rows of the object explorer as they are displayed, and the rows pinned at
 * the top of the tree while it scrolls.
 *
 * A pinned row is an ancestor of the first row that is not hidden under the
 * pinned rows: the source, the database and the table stay visible while the
 * columns of a table scroll. The functions of this module do not depend on
 * the display: the tree shows the result (`ObjectExplorer.vue`).
 */

import type { ExplorerNode } from './explorerNodes'

/**
 * Maximum number of passes to settle the pinned rows: their number changes
 * the first row hidden under them, which changes their number again. The
 * depth of the explorer's tree (about seven levels) is well under this bound.
 */
const MAX_PASSES = 8

/**
 * Row of the tree, as displayed.
 */
export interface VisibleRow {
  /**
   * Node shown on the row.
   */
  node: ExplorerNode
  /**
   * Depth of the row, 1 for the roots.
   */
  level: number
  /**
   * Index of the parent row in the list of rows, `null` for a root.
   */
  parent: number | null
}

/**
 * Rows displayed by the tree, in display order: a node's children follow it
 * only if its key is in the open keys.
 *
 * @param nodes Roots of the tree.
 * @param openKeys Keys of the open nodes.
 */
export function visibleRows(nodes: ExplorerNode[], openKeys: string[]): VisibleRow[] {
  const open = new Set(openKeys)
  const rows: VisibleRow[] = []
  const visit = (list: ExplorerNode[], level: number, parent: number | null): void => {
    for (const node of list) {
      const index = rows.length
      rows.push({ node, level, parent })
      if (node.children?.length && open.has(node.key)) {
        visit(node.children, level + 1, index)
      }
    }
  }
  visit(nodes, 1, null)
  return rows
}

/**
 * Rows to pin at the top of the tree for a scroll position, from the root
 * down: the ancestors of the first row under them. The pinned rows hide as
 * many rows at the top of the tree, so their count must equal the number of
 * ancestors of the row right below them. When no count settles (the rows
 * below are shallower than the rows they would hide), nothing is pinned.
 *
 * @param rows Rows displayed by the tree, from `visibleRows()`.
 * @param scrollTop Scroll position of the tree, in pixels.
 * @param rowHeight Height of a row, in pixels: all the rows have the same one.
 */
export function pinnedRows(rows: VisibleRow[], scrollTop: number, rowHeight: number): VisibleRow[] {
  const first = Math.floor(scrollTop / rowHeight)
  let count = 0
  for (let pass = 0; pass < MAX_PASSES; pass++) {
    const ancestors = ancestorsOf(rows, first + count)
    if (ancestors.length === count) {
      return ancestors
    }
    count = ancestors.length
  }
  return []
}

/**
 * Ancestors of a row, from the root down. A row out of the list has none.
 *
 * @param rows Rows displayed by the tree.
 * @param index Index of the row.
 */
function ancestorsOf(rows: VisibleRow[], index: number): VisibleRow[] {
  const ancestors: VisibleRow[] = []
  let parent = rows[index]?.parent ?? null
  while (parent !== null) {
    const ancestor = rows[parent]
    if (ancestor === undefined) {
      break
    }
    ancestors.unshift(ancestor)
    parent = ancestor.parent
  }
  return ancestors
}
