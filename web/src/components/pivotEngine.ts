/**
 * Engine of the Pivoteur as its Worker drives it (`pivotWorker.ts`): the module of
 * `crates/web-engine`, compiled to WebAssembly, and the messages the page exchanges with the
 * Worker (`pivotClient.ts`).
 *
 * The Worker holds one result: the page sends its rows once, block by block, then asks for
 * tables, and for the windows of the table it shows; the whole table never leaves the module.
 * The values of each level are ranked in the order of the grid (`compareCells`), once per column.
 */

import type { TypeFamily } from '@/api/generated/TypeFamily'
import type { Cell } from '@/api/querySession'

import { compareCells } from './resultGrid'
import { type TransferExports, copyBlock, releaseBlock } from './pivotTransfer'

/**
 * Measure of a table: `COUNT(*)`, `COUNT(col)` or `SUM(col)`.
 */
export interface PivotMeasure {
  /**
   * Aggregate.
   */
  aggregate: 'count_star' | 'count' | 'sum'
  /**
   * Column the measure comes from; `COUNT(*)` keeps it, for its label and its menu.
   */
  column: number
}

/**
 * Table asked of the engine.
 */
export interface PivotRequest {
  /**
   * Columns of the levels of the rows, the outermost first.
   */
  rows: number[]
  /**
   * Columns of the levels of the columns, the outermost first.
   */
  columns: number[]
  /**
   * Measures, in the order of their zone.
   */
  measures: PivotMeasure[]
}

/**
 * Shape of a computed table.
 */
export interface PivotShape {
  /**
   * Number of lines, the grand total included.
   */
  lines: number
  /**
   * Number of columns of values, the labels of the lines apart.
   */
  columns: number
  /**
   * Number of levels of the rows.
   */
  rowLevels: number
  /**
   * Number of levels of the columns.
   */
  columnLevels: number
  /**
   * Number of groups: the distinct combinations of the values of all the levels.
   */
  groups: number
  /**
   * Number of rows taken into account.
   */
  rows: number
}

/**
 * Line of a table.
 */
export interface PivotLine {
  /**
   * Depth: 0 for the grand total, the number of its level otherwise.
   */
  depth: number
  /**
   * Value of its level, `null` for NULL or for the grand total.
   */
  label: Cell
}

/**
 * Column of values of a table.
 */
export interface PivotColumn {
  /**
   * Node of the columns of the column.
   */
  node: number
  /**
   * Node and value of each level of the column, the outermost first: the columns that share a
   * node at a level share its header cell there.
   */
  path: [number, Cell][]
  /**
   * Is the column a subtotal or the grand total? Its header reads `Total` on the level after its
   * path.
   */
  total: boolean
  /**
   * Measure of the column, `null` without any measure.
   */
  measure: number | null
}

/**
 * Value of a cell of a table: its text, `null` for NULL, `false` for an empty cell (no row falls
 * into it).
 */
export type PivotValue = string | null | false

/**
 * Window of a table: some lines, some columns, and their cells.
 */
export interface PivotWindow {
  /**
   * Lines of the window.
   */
  lines: PivotLine[]
  /**
   * Columns of the window.
   */
  columns: PivotColumn[]
  /**
   * Cells of the window, line by line.
   */
  cells: PivotValue[][]
}

/**
 * Refusal of the engine, as the module writes it: a cell that is not a number, a sum beyond
 * 128 bits, a call out of order...
 */
export interface PivotFailure {
  /**
   * Kind of the refusal: `invalid_number`, `too_large`, `overflow`, `not_numeric`,
   * `unknown_column`, `unknown_row`, `ranks`, `block`, `not_loading`, `incomplete`,
   * `no_result`, `no_table`, `request`.
   */
  kind: string
  /**
   * Column concerned.
   */
  column?: number
  /**
   * Row concerned.
   */
  row?: number
  /**
   * Measure concerned, by its index.
   */
  measure?: number
  /**
   * Detail, in English.
   */
  detail?: string
}

/**
 * Refusal of the engine.
 */
export class PivotError extends Error {
  /**
   * @param failure Refusal, as the module writes it.
   */
  constructor(readonly failure: PivotFailure) {
    super(`Pivot engine: ${failure.kind}`)
    this.name = 'PivotError'
  }
}

/**
 * Message of the page to the Worker.
 */
export type PivotRequestMessage =
  | {
      /**
       * Starts the copy of a result, with the family of each column.
       */
      type: 'begin'
      families: TypeFamily[]
    }
  | {
      /**
       * Block of rows of the result, in their order.
       */
      type: 'rows'
      rows: Cell[][]
    }
  | {
      /**
       * End of the copy.
       */
      type: 'finish'
      id: number
    }
  | {
      /**
       * Rows kept by the filter of the grid, `null` for all.
       */
      type: 'kept'
      rows: Uint32Array | null
    }
  | {
      /**
       * Computes a table.
       */
      type: 'compute'
      id: number
      request: PivotRequest
    }
  | {
      /**
       * Asks for a window of the last table computed: lines and columns, start included, end
       * excluded.
       */
      type: 'window'
      id: number
      lines: [number, number]
      columns: [number, number]
    }

/**
 * Answer of the Worker to the message of id `id`.
 */
export type PivotAnswer =
  | { type: 'finished'; id: number }
  | { type: 'table'; id: number; shape: PivotShape; durationMs: number }
  | { type: 'window'; id: number; window: PivotWindow }
  | { type: 'failure'; id: number | null; failure: PivotFailure }

/**
 * Exports of the module of `crates/web-engine`.
 */
interface EngineExports extends TransferExports {
  output_address(): number
  begin(families: number, columns: number): void
  append(
    column: number,
    bytes: number,
    length: number,
    offsets: number,
    valid: number,
    rows: number,
  ): number
  finish(): number
  distinct(column: number): number
  set_ranks(column: number, ranks: number, count: number): number
  set_kept(rows: number, count: number, all: number): number
  compute(words: number, length: number): number
  window(firstLine: number, endLine: number, firstColumn: number, endColumn: number): number
}

/**
 * Code of a family of types for the module: 1 exact numbers, 2 floats, 0 the others.
 *
 * @param family Family of the type of a column.
 */
export function familyCode(family: TypeFamily): number {
  if (family === 'integer' || family === 'decimal') {
    return 1
  }
  return family === 'float' ? 2 : 0
}

/**
 * Rank of each value in the order of the grid (`compareCells`: NULL first, numbers by value,
 * texts by the collator), by index.
 *
 * @param values Distinct values of a column.
 */
export function ranksOf(values: Cell[]): Uint32Array {
  const order = values.map((_, index) => index)
  order.sort((a, b) => compareCells(values[a] ?? null, values[b] ?? null))
  const ranks = new Uint32Array(values.length)
  order.forEach((index, rank) => {
    ranks[index] = rank
  })
  return ranks
}

/**
 * Words of a request for the module: numbers of levels and of measures, the columns of the
 * levels, then a kind and a column per measure.
 *
 * @param request Table asked.
 */
export function requestWords(request: PivotRequest): Uint32Array {
  const kinds = { count_star: 0, count: 1, sum: 2 }
  return Uint32Array.from([
    request.rows.length,
    request.columns.length,
    request.measures.length,
    ...request.rows,
    ...request.columns,
    ...request.measures.flatMap((measure) => [kinds[measure.aggregate], measure.column]),
  ])
}

const decoder = new TextDecoder()

/**
 * Engine of one result, in the memory of the module.
 */
export class PivotEngine {
  /**
   * Values of each column already ranked, by column.
   */
  private readonly ranked = new Set<number>()

  /**
   * @param engine Exports of the module.
   */
  private constructor(private readonly engine: EngineExports) {
    //
  }

  /**
   * Instantiates the module.
   *
   * @param module Compiled module.
   */
  static async create(module: WebAssembly.Module): Promise<PivotEngine> {
    const instance = await WebAssembly.instantiate(module, {})
    return new PivotEngine(instance.exports as unknown as EngineExports)
  }

  /**
   * Reads the answer the module has just written.
   *
   * @param length Length of the answer, in bytes.
   */
  private answer<T>(length: number): T {
    const address = this.engine.output_address() >>> 0
    const text = decoder.decode(new Uint8Array(this.engine.memory.buffer, address, length >>> 0))
    const value = JSON.parse(text) as T | { error: PivotFailure }
    if (value !== null && typeof value === 'object' && 'error' in value) {
      throw new PivotError(value.error)
    }
    return value as T
  }

  /**
   * Copies numbers (`u32`) into the memory of the module, runs `use` with their address, then
   * frees them.
   *
   * @param words Numbers to copy.
   * @param use Call that reads them.
   */
  private withWords<T>(words: Uint32Array, use: (address: number) => T): T {
    const size = Math.max(1, words.length * 4)
    const address = this.engine.reserve(size) >>> 0
    new Uint32Array(this.engine.memory.buffer, address, words.length).set(words)
    try {
      return use(address)
    } finally {
      this.engine.release(address, size)
    }
  }

  /**
   * Starts the copy of a result; the previous one is dropped.
   *
   * @param families Family of the type of each column.
   */
  begin(families: TypeFamily[]): void {
    const size = Math.max(1, families.length)
    const address = this.engine.reserve(size) >>> 0
    new Uint8Array(this.engine.memory.buffer, address, families.length).set(
      families.map(familyCode),
    )
    this.engine.begin(address, families.length)
    this.engine.release(address, size)
    this.ranked.clear()
  }

  /**
   * Appends a block of rows to the copy under way, column by column.
   *
   * @param rows Rows of the block.
   * @param columns Number of columns of the result.
   * @throws {PivotError} If the module refuses the block.
   */
  append(rows: Cell[][], columns: number): void {
    for (let column = 0; column < columns; column++) {
      const block = copyBlock(rows, column, this.engine)
      const length = this.engine.append(
        column,
        block.bytes,
        block.length,
        block.offsets,
        block.valid,
        block.rows,
      )
      releaseBlock(block, this.engine)
      if (length !== 0) {
        this.answer(length)
      }
    }
  }

  /**
   * Ends the copy under way.
   *
   * @throws {PivotError} If no copy is under way, or its columns differ in length.
   */
  finish(): void {
    const length = this.engine.finish()
    if (length !== 0) {
      this.answer(length)
    }
  }

  /**
   * Keeps only the rows the filter of the grid keeps.
   *
   * @param rows Rows kept, in their order; `null` for all.
   */
  setKept(rows: Uint32Array | null): void {
    if (rows === null) {
      this.answer(this.engine.set_kept(0, 0, 1))
      return
    }
    this.withWords(rows, (address) => this.answer(this.engine.set_kept(address, rows.length, 0)))
  }

  /**
   * Ranks the values of a column in the order of the grid, once.
   *
   * @param column Index of the column.
   */
  private rank(column: number): void {
    if (this.ranked.has(column)) {
      return
    }
    const values = this.answer<Cell[]>(this.engine.distinct(column))
    const ranks = ranksOf(values)
    this.withWords(ranks, (address) =>
      this.answer(this.engine.set_ranks(column, address, ranks.length)),
    )
    this.ranked.add(column)
  }

  /**
   * Computes a table, and keeps it for the windows in place of the previous one. A refused
   * table keeps the previous one: the panel goes on showing it, with the error.
   *
   * @param request Table asked.
   * @returns Its shape.
   * @throws {PivotError}
   *     If the engine refuses it: a cell that is not a number, a sum beyond 128 bits...
   */
  compute(request: PivotRequest): PivotShape {
    for (const column of [...request.rows, ...request.columns]) {
      this.rank(column)
    }
    const words = requestWords(request)
    return this.withWords(words, (address) =>
      this.answer<PivotShape>(this.engine.compute(address, words.length)),
    )
  }

  /**
   * Reads a window of the last table computed.
   *
   * @param lines Lines, start included, end excluded.
   * @param columns Columns of values, start included, end excluded.
   */
  window(lines: [number, number], columns: [number, number]): PivotWindow {
    return this.answer<PivotWindow>(this.engine.window(lines[0], lines[1], columns[0], columns[1]))
  }
}

/**
 * State of a Worker between two messages: the engine, and the number of columns of the result
 * under copy.
 */
export interface WorkerState {
  /**
   * Engine of the Worker.
   */
  engine: PivotEngine
  /**
   * Number of columns of the result.
   */
  columns: number
}

/**
 * Handles a message of the page.
 *
 * @param state State of the Worker.
 * @param message Message of the page.
 * @returns The answer to send back, `null` for a message without an answer. A refusal is an answer
 *     too, attached to the id of the message, `null` for a message without an id.
 */
export function answerMessage(
  state: WorkerState,
  message: PivotRequestMessage,
): PivotAnswer | null {
  const id = 'id' in message ? message.id : null
  try {
    switch (message.type) {
      case 'begin':
        state.engine.begin(message.families)
        state.columns = message.families.length
        return null
      case 'rows':
        state.engine.append(message.rows, state.columns)
        return null
      case 'finish':
        state.engine.finish()
        return { type: 'finished', id: message.id }
      case 'kept':
        state.engine.setKept(message.rows)
        return null
      case 'compute': {
        const started = performance.now()
        const shape = state.engine.compute(message.request)
        return { type: 'table', id: message.id, shape, durationMs: performance.now() - started }
      }
      case 'window':
        return {
          type: 'window',
          id: message.id,
          window: state.engine.window(message.lines, message.columns),
        }
    }
  } catch (failure) {
    if (failure instanceof PivotError) {
      return { type: 'failure', id, failure: failure.failure }
    }
    throw failure
  }
}
