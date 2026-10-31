/**
 * Copy of the text cells of a result into the memory of a WebAssembly module, for the WebAssembly
 * side of the benchmark of the Pivoteur's engine (`wasm.mjs`). The module reads only its own
 * memory: each column is copied once, encoded in UTF-8, with the offset and the validity of each
 * cell (see `read_column` in `../wasm/src/lib.rs`).
 */

/**
 * Rows encoded per call of the encoder: their cells are joined into one string, far below the
 * maximum length of a string.
 */
const BLOCK = 65_536

/**
 * Margin over the size of a column estimated from its first block.
 */
const GROWTH_MARGIN = 1.1

const encoder = new TextEncoder()

/**
 * Length in UTF-8 of a text, as `TextEncoder` writes it: a lone surrogate becomes U+FFFD
 * (3 bytes).
 *
 * @param {string} text Text.
 */
function utf8Length(text) {
  let length = 0
  for (let index = 0; index < text.length; index++) {
    const unit = text.charCodeAt(index)
    if (unit < 0x80) {
      length += 1
    } else if (unit < 0x800) {
      length += 2
    } else if (
      unit >= 0xd800 &&
      unit < 0xdc00 &&
      (text.charCodeAt(index + 1) & 0xfc00) === 0xdc00
    ) {
      length += 4
      index++
    } else {
      length += 3
    }
  }
  return length
}

/**
 * Moves the bytes copied so far into a larger memory area.
 *
 * @param {WebAssembly.Exports} engine Exports of the module.
 * @param {{ bytes: number, length: number, capacity: number }} copy Copy under way.
 * @param {number} needed Bytes still to write.
 */
function grow(engine, copy, needed) {
  const capacity = Math.max(copy.capacity * 2, copy.length + needed)
  const bytes = engine.reserve(capacity) >>> 0
  new Uint8Array(engine.memory.buffer).copyWithin(bytes, copy.bytes, copy.bytes + copy.length)
  engine.release(copy.bytes, copy.capacity)
  copy.bytes = bytes
  copy.capacity = capacity
}

/**
 * Copies a column of the rows into the memory of the module: the UTF-8 bytes of its cells, end to
 * end, their `rows + 1` offsets (`u32`, cell `row` runs from `offsets[row]` to
 * `offsets[row + 1]`), and one validity byte per row (0 for NULL).
 *
 * The cells of a block of rows are joined and encoded in one call. In UTF-16 units, their
 * offsets are those in bytes as long as the block is ASCII; otherwise they are counted again,
 * cell by cell. The first block gives the size to reserve for the whole column, which grows if
 * needed: the rows are read only once.
 *
 * @param {(string | null)[][]} rows Rows of the result.
 * @param {number} column Index of the column.
 * @param {WebAssembly.Exports} engine Exports of the module (`memory`, `reserve`, `release`).
 * @returns {{ bytes: number, length: number, capacity: number, offsets: number, valid: number }}
 *   Addresses of the bytes, of the offsets and of the validities; length of the bytes and size
 *   reserved for them.
 */
export function copyColumn(rows, column, engine) {
  const count = rows.length
  const copy = { bytes: engine.reserve(1) >>> 0, length: 0, capacity: 1 }
  const offsets = engine.reserve((count + 1) * 4) >>> 0
  const valid = engine.reserve(count) >>> 0
  const parts = []
  for (let start = 0; start < count; start += BLOCK) {
    const end = Math.min(count, start + BLOCK)
    const blockStart = copy.length
    // The memory may have grown since the last block: its views are taken again.
    let offsetView = new Uint32Array(engine.memory.buffer, offsets, count + 1)
    const validView = new Uint8Array(engine.memory.buffer, valid, count)
    parts.length = 0
    let position = blockStart
    for (let row = start; row < end; row++) {
      const text = rows[row][column]
      offsetView[row] = position
      if (text === null) {
        validView[row] = 0
      } else {
        validView[row] = 1
        parts.push(text)
        position += text.length
      }
    }
    let rest = parts.join('')
    const blockUnits = rest.length
    if (start === 0) {
      // One byte per UTF-16 unit in ASCII, and a margin for the cells to come.
      grow(engine, copy, Math.ceil(((blockUnits * count) / (end - start)) * GROWTH_MARGIN))
    }
    for (;;) {
      const free = new Uint8Array(
        engine.memory.buffer,
        copy.bytes + copy.length,
        copy.capacity - copy.length,
      )
      const { read, written } = encoder.encodeInto(rest, free)
      copy.length += written
      if (read === rest.length) {
        break
      }
      rest = rest.slice(read)
      grow(engine, copy, rest.length * 3)
    }
    if (copy.length - blockStart !== blockUnits) {
      offsetView = new Uint32Array(engine.memory.buffer, offsets, count + 1)
      position = blockStart
      for (let row = start; row < end; row++) {
        const text = rows[row][column]
        offsetView[row] = position
        if (text !== null) {
          position += utf8Length(text)
        }
      }
    }
  }
  new Uint32Array(engine.memory.buffer, offsets, count + 1)[count] = copy.length
  return { ...copy, offsets, valid }
}
