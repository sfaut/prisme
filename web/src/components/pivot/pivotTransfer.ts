/**
 * Copy of a block of rows of a result into the memory of the engine of the Pivoteur
 * (`crates/web-engine`, `append`): the cells of one column, encoded in UTF-8 end to end, with the
 * offset of each cell and its validity. The layout of `bench/pivot/js/transfer.mjs`, whose copy of
 * a million rows takes 0.63 s (`docs/PERFORMANCES.md`, chapter 9).
 *
 * The cells of the block are joined and encoded in one call. Their offsets, counted in UTF-16
 * units, are those in bytes as long as the block is ASCII; otherwise they are counted again, cell
 * by cell.
 */

import type { Cell } from '@/api/querySession'

/**
 * Exports of the module the copy uses.
 */
export interface TransferExports {
  /**
   * Memory of the module.
   */
  memory: WebAssembly.Memory
  /**
   * Reserves bytes in the memory of the module, and returns their address.
   */
  reserve(size: number): number
  /**
   * Frees bytes reserved by `reserve`.
   */
  release(address: number, size: number): void
}

/**
 * Block of cells copied into the memory of the module, to free once read.
 */
export interface CopiedBlock {
  /**
   * Address of the UTF-8 bytes of the cells.
   */
  bytes: number
  /**
   * Number of bytes written.
   */
  length: number
  /**
   * Size reserved for the bytes.
   */
  capacity: number
  /**
   * Address of the `rows + 1` offsets (`u32`).
   */
  offsets: number
  /**
   * Address of the validity bytes, one per row.
   */
  valid: number
  /**
   * Number of rows of the block.
   */
  rows: number
}

const encoder = new TextEncoder()

/**
 * Length in UTF-8 of a text, as `TextEncoder` writes it: a lone surrogate becomes U+FFFD
 * (3 bytes).
 *
 * @param text Text.
 */
export function utf8Length(text: string): number {
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
 * Copies the cells of a column, for a block of rows, into the memory of the module.
 *
 * @param rows Rows of the block.
 * @param column Index of the column.
 * @param engine Exports of the module.
 * @returns Addresses of the copy, to free by `releaseBlock` once the module has read it.
 */
export function copyBlock(rows: Cell[][], column: number, engine: TransferExports): CopiedBlock {
  const count = rows.length
  const parts: string[] = []
  for (const row of rows) {
    const cell = row[column] ?? null
    if (cell !== null) {
      parts.push(cell)
    }
  }
  const text = parts.join('')
  // A UTF-16 unit takes 3 bytes of UTF-8 at most.
  const capacity = Math.max(1, text.length * 3)
  const bytes = engine.reserve(capacity) >>> 0
  const offsets = engine.reserve((count + 1) * 4) >>> 0
  const valid = engine.reserve(Math.max(1, count)) >>> 0
  // The memory may grow on each reservation: its views are taken afterwards.
  const { written } = encoder.encodeInto(
    text,
    new Uint8Array(engine.memory.buffer, bytes, capacity),
  )
  const offsetView = new Uint32Array(engine.memory.buffer, offsets, count + 1)
  const validView = new Uint8Array(engine.memory.buffer, valid, count)
  const ascii = written === text.length
  let position = 0
  rows.forEach((row, index) => {
    const cell = row[column] ?? null
    offsetView[index] = position
    validView[index] = cell === null ? 0 : 1
    if (cell !== null) {
      position += ascii ? cell.length : utf8Length(cell)
    }
  })
  offsetView[count] = written
  return { bytes, length: written, capacity, offsets, valid, rows: count }
}

/**
 * Frees a copy once the module has read it.
 *
 * @param block Copy to free.
 * @param engine Exports of the module.
 */
export function releaseBlock(block: CopiedBlock, engine: TransferExports): void {
  engine.release(block.bytes, block.capacity)
  engine.release(block.offsets, (block.rows + 1) * 4)
  engine.release(block.valid, Math.max(1, block.rows))
}
