/**
 * Rows of the object explorer as they are displayed, and the rows pinned at
 * the top of the tree while it scrolls.
 *
 * Each level of the hierarchy has its own slot at the top of the tree: the
 * first level in the first slot, the second one below, and so on. A row
 * pins to its slot as soon as it leaves the top of the tree, and stays there
 * in full while its section (the row and the rows below it that belong to
 * it) still reaches that slot. The source, the database and the table thus
 * stay fully visible while the columns of a table scroll, and each one leaves
 * the top only when its last line reaches it.
 *
 * The functions of this module do not depend on the display: the tree shows
 * the result (`ObjectExplorer.vue`).
 */

import type { ExplorerNode } from './explorerNodes'

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
  /**
   * Index of the last row of the row's section: the row itself if it has no
   * displayed children, else the last row among its descendants.
   */
  end: number
}

/**
 * Rows displayed by the tree, with the indexes of the rows of each level.
 */
export interface VisibleRows {
  /**
   * Rows, in display order.
   */
  rows: VisibleRow[]
  /**
   * Indexes of the rows of each level, in display order: `byLevel[1]` lists
   * the roots. The entry 0 is unused.
   */
  byLevel: number[][]
}

/**
 * Row pinned at the top of the tree.
 */
export interface PinnedRow {
  /**
   * Node shown on the pinned row.
   */
  node: ExplorerNode
  /**
   * Depth of the row, 1 for the roots: its slot is the level minus one.
   */
  level: number
}

/**
 * Rows displayed by the tree: a node's children follow it only if its key is
 * in the open keys.
 *
 * @param nodes Roots of the tree.
 * @param openKeys Keys of the open nodes.
 */
export function visibleRows(nodes: ExplorerNode[], openKeys: string[]): VisibleRows {
  const open = new Set(openKeys)
  const rows: VisibleRow[] = []
  const byLevel: number[][] = []
  const visit = (list: ExplorerNode[], level: number, parent: number | null): void => {
    for (const node of list) {
      const index = rows.length
      const row: VisibleRow = { node, level, parent, end: index }
      rows.push(row)
      byLevel[level] ??= []
      byLevel[level].push(index)
      if (node.children?.length && open.has(node.key)) {
        visit(node.children, level + 1, index)
      }
      row.end = rows.length - 1
    }
  }
  visit(nodes, 1, null)
  return { rows, byLevel }
}

/**
 * Rows to pin at the top of the tree for a scroll position, from the root
 * down. A pinned row is the last row of its level that is above its slot,
 * and its section still reaches the slot. A row without children is never
 * pinned: it has nothing to keep in view.
 *
 * @param visible Rows displayed by the tree, from `visibleRows()`.
 * @param scrollTop Scroll position of the tree, in pixels.
 * @param rowHeight Height of a row, in pixels: all the rows have the same one.
 */
export function pinnedRows(
  visible: VisibleRows,
  scrollTop: number,
  rowHeight: number,
): PinnedRow[] {
  const { rows, byLevel } = visible
  const scroll = scrollTop / rowHeight
  const pinned: PinnedRow[] = []
  for (let level = 1; level < byLevel.length; level++) {
    const slot = level - 1
    const index = lastBelow(byLevel[level] ?? [], scroll + slot)
    const row = index === undefined ? undefined : rows[index]
    if (row === undefined || row.end === index || row.end - scroll < slot) {
      continue
    }
    pinned.push({ node: row.node, level })
  }
  return pinned
}

/**
 * Largest value of a sorted list that is strictly below a limit.
 *
 * @param sorted Values in ascending order.
 * @param limit Exclusive upper bound.
 */
function lastBelow(sorted: number[], limit: number): number | undefined {
  let low = 0
  let high = sorted.length
  while (low < high) {
    const middle = Math.floor((low + high) / 2)
    if ((sorted[middle] ?? Number.POSITIVE_INFINITY) < limit) {
      low = middle + 1
    } else {
      high = middle
    }
  }
  return low === 0 ? undefined : sorted[low - 1]
}
