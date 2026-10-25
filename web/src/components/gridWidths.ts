/**
 * Column widths chosen in the result grid, remembered per query for the
 * life of the page.
 *
 * Each run recreates the result grid: without memory, running a query again
 * would lose the chosen widths. They are therefore kept here, by query text,
 * and only apply if the result has the same columns, in the same order (a
 * table changed in the meantime starts again from the estimated widths).
 */

/**
 * Number of queries whose widths are remembered; beyond, the oldest ones are
 * forgotten.
 */
const MAX_REMEMBERED = 100

/**
 * Widths remembered for a query.
 */
interface RememberedWidths {
  /**
   * Names of the result's columns, in the order of the `SELECT`.
   */
  columns: string[]
  /**
   * Chosen width, a CSS length relative to the root, by column index in the
   * `SELECT`.
   */
  widths: Record<number, string>
}

/**
 * Remembered widths, by query text, from the oldest to the most recent.
 */
const remembered = new Map<string, RememberedWidths>()

/**
 * Widths chosen for a query, empty if none is remembered or if the columns
 * have changed.
 *
 * @param sql Text of the query.
 * @param columns Names of the result's columns.
 */
export function rememberedWidths(sql: string, columns: string[]): Record<number, string> {
  const entry = remembered.get(sql)
  if (entry === undefined || entry.columns.join('\u0000') !== columns.join('\u0000')) {
    return {}
  }
  return { ...entry.widths }
}

/**
 * Remembers the widths chosen for a query.
 *
 * @param sql Text of the query.
 * @param columns Names of the result's columns.
 * @param widths Chosen width, by column index.
 */
export function rememberWidths(
  sql: string,
  columns: string[],
  widths: Record<number, string>,
): void {
  // Delete then put back: the query becomes the most recent.
  remembered.delete(sql)
  remembered.set(sql, { columns: [...columns], widths: { ...widths } })
  if (remembered.size > MAX_REMEMBERED) {
    const oldest = remembered.keys().next().value
    if (oldest !== undefined) {
      remembered.delete(oldest)
    }
  }
}
