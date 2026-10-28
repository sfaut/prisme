/**
 * Result tabs for the tests of the components that show them: a finished
 * query, with its columns and its rows, as the server saves it.
 */
import { type QueryTab, savedTab } from '@/api/querySession'

/**
 * Finished tab of a query.
 *
 * @param columns Names of the columns; none for a query without a result.
 * @param rows Rows of the result.
 */
export function finishedTab(columns: string[], rows: string[][] = []): QueryTab {
  return savedTab({
    id: 1,
    query: 0,
    sql: 'SELECT 1',
    name: '',
    pinned: false,
    result: {
      started: true,
      done: true,
      error: null,
      error_detail: null,
      columns:
        columns.length === 0
          ? null
          : {
              columns,
              right_aligned: columns.map(() => false),
              types: [],
              sql: 'SELECT 1',
              limit: 'added',
              expected_rows: 1000,
              execution_us: 2000,
              executed_at: 1_792_500_000_000,
            },
      row_count: rows.length,
      truncated: false,
      affected_rows: 0,
      transfer_us: 3000,
      bytes: 120,
    },
    rows,
  })
}
