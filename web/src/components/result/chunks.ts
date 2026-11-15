/**
 * Writing of a file in the page by chunks of rows: between two chunks, a
 * pause lets the browser handle the user's actions, so that a large result
 * does not freeze the page. Shared by the serialisers of the downloads (CSV,
 * CTE) and by the `IN` clause of a selection.
 */

/**
 * Rows written between two pauses.
 */
const CHUNK = 2000

/**
 * Lets the browser handle the waiting events before the next chunk. A timer
 * keeps going when the tab is hidden, at a slower pace, where a frame of the
 * display would wait for its return.
 */
function pause(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve))
}

/**
 * Visits rows by chunks, with a pause between two chunks.
 *
 * @param count Number of rows.
 * @param visit Visit of a row, from its index.
 * @param chunkEnd Called at the end of each chunk, before its pause.
 */
export async function visitByChunks(
  count: number,
  visit: (row: number) => void,
  chunkEnd: () => void = () => {},
): Promise<void> {
  for (let start = 0; start < count; start += CHUNK) {
    const end = Math.min(count, start + CHUNK)
    for (let row = start; row < end; row++) {
      visit(row)
    }
    chunkEnd()
    if (end < count) {
      await pause()
    }
  }
}

/**
 * Text of rows, written by chunks, with a pause between two chunks.
 *
 * @param count Number of rows.
 * @param line Text of a row, from its index, line ending included.
 * @returns The text of each chunk, in order.
 */
export async function writeByChunks(
  count: number,
  line: (row: number) => string,
): Promise<string[]> {
  const parts: string[] = []
  let text = ''
  await visitByChunks(
    count,
    (row) => {
      text += line(row)
    },
    () => {
      parts.push(text)
      text = ''
    },
  )
  return parts
}
