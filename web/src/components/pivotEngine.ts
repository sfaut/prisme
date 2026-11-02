/**
 * Engine of the Pivoteur as its Worker drives it (`pivotWorker.ts`): the module of
 * `crates/web-engine`, compiled to WebAssembly, and the messages the page exchanges with the
 * Worker (`pivotClient.ts`).
 *
 * The Worker holds one result: the page sends its rows once, block by block, then asks for
 * tables, and for the windows of the table it shows; the whole table never leaves the module.
 * A window names its table, by the id of the message that computed it: a window asked for a
 * table that another one has replaced since is refused (`stale`), and never mixes the shape of
 * one table with the cells of the next.
 * The values of each level are ranked in the order of the grid (`compareCells`), once per column;
 * the module ranks the periods of a level with a precision, in the order of time. A table asked
 * again in another order is only laid out again, which makes it a new table for its windows.
 * The Worker also gives the distinct values of a column, or the periods of a precision of its
 * dates, for the list of a filter.
 */

import type { DatePrecision } from '@/api/generated/DatePrecision'
import type { TypeFamily } from '@/api/generated/TypeFamily'
import type { PivotAggregate } from '@/api/generated/PivotAggregate'
import type { PivotTest } from '@/api/generated/PivotTest'
import type { Cell } from '@/api/querySession'

import { compareCells } from './resultGrid'
import { type TransferExports, copyBlock, releaseBlock } from './pivotTransfer'

/**
 * Measure of a table: `COUNT(*)`, `COUNT`, `SUM` and `AVG` of a column or of its distinct values,
 * `MIN` and `MAX` of a column.
 */
export interface PivotMeasure {
  /**
   * Aggregate.
   */
  aggregate: PivotAggregate
  /**
   * Column the measure comes from; `COUNT(*)` keeps it, for its label and its menu.
   */
  column: number
}

/**
 * Level of a table: the values of a column, or what a precision keeps of its dates.
 */
export interface PivotRequestLevel {
  /**
   * Index of the column.
   */
  column: number
  /**
   * What the level keeps of the dates of the column.
   */
  precision: DatePrecision
}

/**
 * Test of a value by a filter.
 */
export interface PivotRequestTest {
  /**
   * What the value passes.
   */
  condition: PivotTest
  /**
   * Value compared to, or lower bound; `IS NULL` and `IS NOT NULL` do not read it.
   */
  operand: string
  /**
   * Upper bound, which only `BETWEEN` and `NOT BETWEEN` read.
   */
  upper: string
}

/**
 * Filter of a table.
 */
export type PivotRequestFilter =
  | {
      /**
       * Keeps the rows whose value is one of some values, or none of them.
       */
      kind: 'values'
      /**
       * Column of the cells, and what its dates keep.
       */
      level: PivotRequestLevel
      /**
       * Values listed, `null` for NULL.
       */
      values: Cell[]
      /**
       * Does the filter keep the values left out of the list (`NOT IN`)?
       */
      excluded: boolean
    }
  | {
      /**
       * Keeps the rows whose value passes a test.
       */
      kind: 'column'
      /**
       * Column of the cells, and what its dates keep.
       */
      level: PivotRequestLevel
      /**
       * Test of the value of each row.
       */
      test: PivotRequestTest
    }
  | {
      /**
       * Keeps the rows of the groups whose measure passes a test.
       */
      kind: 'measure'
      /**
       * Measure compared.
       */
      measure: PivotMeasure
      /**
       * Levels of its groups, the outermost first; without any, all the rows make one group.
       */
      levels: PivotRequestLevel[]
      /**
       * Test of the value of each group.
       */
      test: PivotRequestTest
    }

/**
 * Table asked of the engine.
 */
export interface PivotRequest {
  /**
   * Levels of the rows, the outermost first.
   */
  rows: PivotRequestLevel[]
  /**
   * Levels of the columns, the outermost first.
   */
  columns: PivotRequestLevel[]
  /**
   * Measures, in the order of their zone.
   */
  measures: PivotMeasure[]
  /**
   * Filters, which combine by `AND`.
   */
  filters: PivotRequestFilter[]
}

/**
 * Node of an axis whose values rank the other axis: a column of values for the lines, a line for
 * the columns.
 */
export interface PivotValueTarget {
  /**
   * Values of the levels of the node, the outermost first: a shorter path reads a subtotal, an
   * empty one the grand total.
   */
  path: Cell[]
  /**
   * Measure, by its index.
   */
  measure: number
}

/**
 * Values that rank the nodes of an axis.
 */
export interface PivotValueOrder extends PivotValueTarget {
  /**
   * Is the order descending?
   */
  descending: boolean
}

/**
 * Order of the nodes of an axis, among those of one parent.
 */
export interface PivotAxisOrder {
  /**
   * Is each level in descending order, the outermost first? A level left out is ascending.
   */
  descending: boolean[]
  /**
   * Values that rank the nodes before their labels, `null` without any.
   */
  values: PivotValueOrder | null
}

/**
 * Order of the lines and of the columns of a table.
 */
export interface PivotOrder {
  /**
   * Order of the nodes of the rows, hence of the lines.
   */
  rows: PivotAxisOrder
  /**
   * Order of the nodes of the columns.
   */
  columns: PivotAxisOrder
}

/**
 * Order of a table just computed: its labels, ascending.
 */
export const LABEL_ORDER: PivotOrder = {
  rows: { descending: [], values: null },
  columns: { descending: [], values: null },
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
  /**
   * Value of each of its levels, the outermost first: empty for the grand total.
   */
  path: Cell[]
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
 * Refusal of the engine, as the module writes it: a cell that is not a number, a value beyond
 * 128 bits, a call out of order...
 */
export interface PivotFailure {
  /**
   * Kind of the refusal: `invalid_number`, `too_large`, `overflow`, `not_numeric`,
   * `invalid_date`, `filter_not_numeric`, `filter_overflow`, `filter_operand`, `unknown_column`,
   * `unknown_row`, `ranks`, `block`, `not_loading`, `incomplete`, `no_result`, `no_table`,
   * `request`, `stale` (a window of a replaced table).
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
   * Filter concerned, by its index in the request.
   */
  filter?: number
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
       * Computes a table, and lays it out in an order: the same request only lays it out again.
       */
      type: 'compute'
      id: number
      request: PivotRequest
      order: PivotOrder
    }
  | {
      /**
       * Asks for a window of a table, by the id of the message that computed it: lines and
       * columns, start included, end excluded.
       */
      type: 'window'
      id: number
      table: number
      lines: [number, number]
      columns: [number, number]
    }
  | {
      /**
       * Asks for the distinct values of a column, in the order of the grid, or for the periods of
       * a precision of its dates, in the order of time.
       */
      type: 'values'
      id: number
      column: number
      precision: DatePrecision
    }

/**
 * Answer of the Worker to the message of id `id`.
 */
export type PivotAnswer =
  | { type: 'finished'; id: number }
  | { type: 'table'; id: number; shape: PivotShape; durationMs: number }
  | { type: 'window'; id: number; window: PivotWindow }
  | { type: 'values'; id: number; values: Cell[] }
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
  distinct(column: number, precision: number): number
  set_ranks(column: number, ranks: number, count: number): number
  set_kept(rows: number, count: number, all: number): number
  compute(words: number, length: number): number
  sort(words: number, length: number): number
  window(firstLine: number, endLine: number, firstColumn: number, endColumn: number): number
}

/**
 * Code of a family of types for the module: 1 exact numbers, 2 floats, 3 dates, 4 instants,
 * 5 hours, 0 the others.
 *
 * @param family Family of the type of a column.
 */
export function familyCode(family: TypeFamily): number {
  switch (family) {
    case 'integer':
    case 'decimal':
      return 1
    case 'float':
      return 2
    case 'date':
      return 3
    case 'datetime':
      return 4
    case 'time':
      return 5
    default:
      return 0
  }
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
 * Word that stands for NULL in a path of values.
 */
const NULL_STEP = 0xffff_ffff

const encoder = new TextEncoder()

/**
 * Words of a value: `NULL_STEP` for NULL, or its length in bytes then one word per byte of its
 * UTF-8 text.
 *
 * @param value Value.
 */
function valueWords(value: Cell): number[] {
  if (value === null) {
    return [NULL_STEP]
  }
  const bytes = encoder.encode(value)
  return [bytes.length, ...bytes]
}

/**
 * Code of each precision for the module.
 */
const PRECISION_CODES: Record<DatePrecision, number> = {
  none: 0,
  year: 1,
  quarter: 2,
  month: 3,
  week: 4,
  date: 5,
}

/**
 * Words of a test: the code of a comparison and its operand; 6 or 7 for `BETWEEN` and `NOT BETWEEN`
 * and their two bounds; 8 or 9 for `IS NULL` and `IS NOT NULL`.
 *
 * @param test Test.
 */
function testWords({ condition, operand, upper }: PivotRequestTest): number[] {
  switch (condition) {
    case 'between':
      return [6, ...valueWords(operand), ...valueWords(upper)]
    case 'not_between':
      return [7, ...valueWords(operand), ...valueWords(upper)]
    case 'is_null':
      return [8]
    case 'is_not_null':
      return [9]
    default: {
      const comparisons = {
        equal: 0,
        not_equal: 1,
        less: 2,
        less_or_equal: 3,
        greater: 4,
        greater_or_equal: 5,
      }
      return [comparisons[condition], ...valueWords(operand)]
    }
  }
}

/**
 * Words of a request for the module: numbers of levels and of measures, a column and a precision
 * per level, a kind and a column per measure, then the filters. A filter on a list of values: 0,
 * its level, a word for `NOT IN`, the number of its values and each value. A filter on a measure: 1,
 * the measure, the number of its levels and each level, and the test. A filter on a column: 2, its
 * level and the test.
 *
 * @param request Table asked.
 */
export function requestWords(request: PivotRequest): Uint32Array {
  const levelWords = ({ column, precision }: PivotRequestLevel) => [
    column,
    PRECISION_CODES[precision],
  ]
  const kinds = {
    count_star: 0,
    count: 1,
    sum: 2,
    count_distinct: 3,
    sum_distinct: 4,
    avg: 5,
    avg_distinct: 6,
    min: 7,
    max: 8,
  }
  const measureWords = ({ aggregate, column }: PivotMeasure) => [kinds[aggregate], column]
  const filterWords = (filter: PivotRequestFilter): number[] => {
    switch (filter.kind) {
      case 'values':
        return [
          0,
          ...levelWords(filter.level),
          Number(filter.excluded),
          filter.values.length,
          ...filter.values.flatMap(valueWords),
        ]
      case 'measure':
        return [
          1,
          ...measureWords(filter.measure),
          filter.levels.length,
          ...filter.levels.flatMap(levelWords),
          ...testWords(filter.test),
        ]
      case 'column':
        return [2, ...levelWords(filter.level), ...testWords(filter.test)]
    }
  }
  return Uint32Array.from([
    request.rows.length,
    request.columns.length,
    request.measures.length,
    ...request.rows.flatMap(levelWords),
    ...request.columns.flatMap(levelWords),
    ...request.measures.flatMap(measureWords),
    ...request.filters.flatMap(filterWords),
  ])
}

/**
 * Words of the order of an axis: the number of its levels and a word per level, then 0 without
 * values that rank it; otherwise 1, the measure, a word for the direction, the number of steps of
 * the path, and each step (`valueWords`).
 *
 * @param order Order of the axis.
 */
function axisWords({ descending, values }: PivotAxisOrder): number[] {
  const words = [descending.length, ...descending.map(Number)]
  if (values === null) {
    return [...words, 0]
  }
  const steps = values.path.flatMap(valueWords)
  return [...words, 1, values.measure, Number(values.descending), values.path.length, ...steps]
}

/**
 * Words of an order for the module: that of the rows, then that of the columns.
 *
 * @param order Order of the table.
 */
export function orderWords(order: PivotOrder): Uint32Array {
  return Uint32Array.from([...axisWords(order.rows), ...axisWords(order.columns)])
}

/**
 * Words of the order of a table just computed.
 */
const LABEL_ORDER_KEY = orderWords(LABEL_ORDER).join()

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
   * Words of the request of the table in the module, and its shape: asking for it again only lays
   * it out again. `null` before any table, or once the rows taken into account have changed.
   */
  private computed: { key: string; shape: PivotShape } | null = null

  /**
   * Words of the order the table in the module is laid out in.
   */
  private laidOut = LABEL_ORDER_KEY

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
    this.computed = null
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
    this.computed = null
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
    const values = this.answer<Cell[]>(this.engine.distinct(column, PRECISION_CODES.none))
    const ranks = ranksOf(values)
    this.withWords(ranks, (address) =>
      this.answer(this.engine.set_ranks(column, address, ranks.length)),
    )
    this.ranked.add(column)
  }

  /**
   * Distinct values of a column, in the order of the grid; at a precision, the labels of its
   * periods in the order of time, without the cells outside of the calendar.
   *
   * @param column Index of the column.
   * @param precision What the column keeps of its dates.
   */
  values(column: number, precision: DatePrecision = 'none'): Cell[] {
    const values = this.answer<Cell[]>(this.engine.distinct(column, PRECISION_CODES[precision]))
    return precision === 'none' ? values.sort(compareCells) : values
  }

  /**
   * Computes a table, and keeps it for the windows in place of the previous one, then lays it out
   * in an order. The table in the module, asked again, is only laid out again: a sort computes no
   * aggregate. A refused table keeps the previous one: the panel goes on showing it, with the
   * error.
   *
   * @param request Table asked.
   * @param order Order of its lines and of its columns.
   * @returns Its shape.
   * @throws {PivotError}
   *     If the engine refuses it: a cell that is not a number, a value beyond 128 bits...
   */
  compute(request: PivotRequest, order: PivotOrder = LABEL_ORDER): PivotShape {
    for (const { column, precision } of [...request.rows, ...request.columns]) {
      if (precision === 'none') {
        this.rank(column)
      }
    }
    const words = requestWords(request)
    const key = words.join()
    if (this.computed?.key !== key) {
      const shape = this.withWords(words, (address) =>
        this.answer<PivotShape>(this.engine.compute(address, words.length)),
      )
      this.computed = { key, shape }
      this.laidOut = LABEL_ORDER_KEY
    }
    const { shape } = this.computed
    const ordering = orderWords(order)
    const layout = ordering.join()
    if (layout !== this.laidOut) {
      this.withWords(ordering, (address) => this.answer(this.engine.sort(address, ordering.length)))
      this.laidOut = layout
    }
    return shape
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
  /**
   * Id of the message that computed the table of the engine, `null` before any. A refused
   * table leaves the previous one.
   */
  table: number | null
}

/**
 * Handles a message of the page.
 *
 * @param state State of the Worker.
 * @param message Message of the page.
 * @returns
 *     The answer to send back, `null` for a message without an answer. A refusal is an answer
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
        state.table = null
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
        const shape = state.engine.compute(message.request, message.order)
        state.table = message.id
        return { type: 'table', id: message.id, shape, durationMs: performance.now() - started }
      }
      case 'window':
        if (message.table !== state.table) {
          return { type: 'failure', id, failure: { kind: 'stale' } }
        }
        return {
          type: 'window',
          id: message.id,
          window: state.engine.window(message.lines, message.columns),
        }
      case 'values':
        return {
          type: 'values',
          id: message.id,
          values: state.engine.values(message.column, message.precision),
        }
    }
  } catch (failure) {
    if (failure instanceof PivotError) {
      return { type: 'failure', id, failure: failure.failure }
    }
    throw failure
  }
}
