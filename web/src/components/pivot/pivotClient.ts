/**
 * Page side of the Pivoteur's Worker (`pivotWorker.ts`): one Worker per result, fed with a copy of
 * its rows as they arrive, and asked for tables and for the windows of the table shown.
 *
 * The client starts with the columns of the result. Each call of `follow` copies the rows
 * received since the last one, in blocks, with a pause between two blocks so that the page keeps
 * answering; `end` copies the last ones, once the result is complete. A table, a list of values or
 * a window waits for the copy of the rows the result held when it was asked, and is computed on
 * them: a table asked again after new rows adds them to its state. The filter of the grid is
 * applied in the page, to the rows copied so far, then to each block. Closing the client stops the
 * Worker, and with it the memory of its module.
 */

import { toRaw } from 'vue'

import type { DatePrecision } from '@/api/generated/DatePrecision'
import type { TypeFamily } from '@/api/generated/TypeFamily'
import type { Cell, QueryResult } from '@/api/querySession'

import {
  type PivotAnswer,
  type PivotFailure,
  type PivotLayout,
  type PivotOrder,
  type PivotRequest,
  type PivotRequestMessage,
  type PivotShape,
  type PivotTextLengths,
  type PivotValueTarget,
  type PivotWindow,
  FIRST_LAYOUT,
  LABEL_ORDER,
  PivotError,
} from './pivotEngine'

/**
 * Rows per block sent to the Worker.
 */
export const BLOCK_ROWS = 20_000

/**
 * Shortest delay between the starts of two computations of the table during the reception of the
 * rows, in milliseconds, without a setting of the user.
 */
export const DEFAULT_REFRESH_MS = 1000

/**
 * Test of the rows the filter of the grid keeps.
 */
export type RowTest = (row: Cell[]) => boolean

/**
 * What the client needs of a Worker: the tests give it one that runs the engine in the page.
 */
export interface PivotWorkerPort {
  /**
   * Sends a message to the Worker.
   */
  postMessage(message: PivotRequestMessage): void
  /**
   * Receives the answers of the Worker.
   */
  onmessage: ((event: MessageEvent<PivotAnswer>) => void) | null
  /**
   * Stops the Worker.
   */
  terminate(): void
}

/**
 * Table computed by the Worker.
 */
export interface PivotTable {
  /**
   * Id of the table, which its windows name.
   */
  id: number
  /**
   * Shape of the table.
   */
  shape: PivotShape
  /**
   * Position in the table of each column named for the computation, `null` for a column it
   * does not show.
   */
  positions: (number | null)[]
  /**
   * Lengths of the texts of its columns of values, which their default widths fit.
   */
  lengths: PivotTextLengths
  /**
   * Duration of the computation in the Worker, in milliseconds.
   */
  durationMs: number
  /**
   * Rows of the result the table is computed on, before the filter of the grid: all of them, or
   * those received when it was asked.
   */
  received: number
}

/**
 * Answer awaited for a message.
 */
interface Pending {
  /**
   * Gives the answer.
   */
  resolve: (answer: PivotAnswer) => void
  /**
   * Gives the refusal.
   */
  reject: (error: Error) => void
}

/**
 * Lets the page answer before the next block.
 */
function pause(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0))
}

/**
 * Family of each column of a result; `other` when the result was saved before its columns were
 * typed.
 *
 * @param result Result.
 */
function families(result: QueryResult): TypeFamily[] {
  return result.columns.map((_, column) => result.columnTypes[column]?.family ?? 'other')
}

/**
 * Client of the Worker of one result.
 */
export class PivotClient {
  /**
   * Id of the next message that awaits an answer.
   */
  private nextId = 1

  /**
   * Answers awaited, by id.
   */
  private readonly pending = new Map<number, Pending>()

  /**
   * Refusal of a block of rows, given to every answer that follows: the rows of the Worker are no
   * longer those of the result.
   */
  private blockFailure: PivotFailure | null = null

  /**
   * Has the client been closed?
   */
  private closed = false

  /**
   * Rows of the result, which grow during the reception.
   */
  private readonly rows: Cell[][]

  /**
   * Rows sent to the Worker so far.
   */
  private sent = 0

  /**
   * Test of the filter of the grid applied to the blocks sent, `null` without a filter.
   */
  private keep: RowTest | null = null

  /**
   * Copies asked so far, each after the previous one; refused with the end of the copy refused.
   */
  private copy: Promise<void> = Promise.resolve()

  /**
   * End of the copy, once asked (`end`).
   */
  private ending: Promise<void> | null = null

  /**
   * Starts the copy of the rows of a result into a Worker, with the rows it holds already.
   *
   * @param worker Worker of the engine.
   * @param result Result to copy, complete or still being received.
   */
  constructor(
    private readonly worker: PivotWorkerPort,
    result: QueryResult,
  ) {
    worker.onmessage = (event) => this.receive(event.data)
    this.rows = toRaw(result.rows)
    worker.postMessage({ type: 'begin', families: families(result) })
    this.follow()
  }

  /**
   * Creates the client of a result, with a Worker of its own.
   *
   * @param result Result to copy, complete or still being received.
   */
  static start(result: QueryResult): PivotClient {
    const worker = new Worker(new URL('./pivotWorker.ts', import.meta.url), { type: 'module' })
    return new PivotClient(worker, result)
  }

  /**
   * Handles an answer of the Worker.
   *
   * @param answer Answer.
   */
  private receive(answer: PivotAnswer): void {
    if (answer.id === null) {
      if (answer.type === 'failure') {
        this.blockFailure ??= answer.failure
      }
      return
    }
    const pending = this.pending.get(answer.id)
    this.pending.delete(answer.id)
    if (this.blockFailure !== null) {
      pending?.reject(new PivotError(this.blockFailure))
    } else if (answer.type === 'failure') {
      pending?.reject(new PivotError(answer.failure))
    } else {
      pending?.resolve(answer)
    }
  }

  /**
   * Sends a message that awaits an answer.
   *
   * @param message Message, with the id of its answer.
   */
  private ask(message: PivotRequestMessage & { id: number }): Promise<PivotAnswer> {
    if (this.closed) {
      return Promise.reject(new Error('Pivot client closed'))
    }
    return new Promise((resolve, reject) => {
      this.pending.set(message.id, { resolve, reject })
      this.worker.postMessage(message)
    })
  }

  /**
   * Runs a step of the copy after the steps asked before it.
   *
   * @param step Step of the copy.
   */
  private chain(step: () => Promise<void> | void): Promise<void> {
    this.copy = this.copy.then(step)
    // A refused copy is given to the calls that wait for it.
    this.copy.catch(() => undefined)
    return this.copy
  }

  /**
   * Rows the filter of the grid keeps, from `start` included to `end` excluded.
   *
   * @param keep Test of the filter.
   * @param start First row.
   * @param end End of the rows.
   */
  private keptRows(keep: RowTest, start: number, end: number): Uint32Array {
    const kept: number[] = []
    for (let row = start; row < end; row++) {
      if (keep(this.rows[row]!)) {
        kept.push(row)
      }
    }
    return Uint32Array.from(kept)
  }

  /**
   * Sends the rows of the result after those sent, up to `end`, block by block.
   *
   * @param end End of the rows to send.
   */
  private async send(end: number): Promise<void> {
    const first = this.sent
    for (let start = first; start < end; start += BLOCK_ROWS) {
      if (start > first) {
        await pause()
      }
      if (this.closed) {
        return
      }
      const stop = Math.min(end, start + BLOCK_ROWS)
      const rows = this.rows.slice(start, stop)
      const keep = this.keep
      if (keep === null) {
        this.worker.postMessage({ type: 'rows', rows })
      } else {
        this.worker.postMessage({ type: 'rows', rows, kept: this.keptRows(keep, start, stop) })
      }
      this.sent = stop
    }
  }

  /**
   * Copies the rows the result has received since the last copy, after the copies asked before.
   */
  follow(): void {
    if (this.ending === null) {
      void this.chain(() => this.send(this.rows.length))
    }
  }

  /**
   * Copies the last rows of the complete result, then ends the copy: no row comes after.
   *
   * @returns The end of the copy, refused with the refusal of a block or of the end.
   */
  end(): Promise<void> {
    this.ending ??= this.chain(async () => {
      await this.send(this.rows.length)
      await this.ask({ type: 'finish', id: this.nextId++ })
    })
    return this.ending
  }

  /**
   * Keeps only the rows the filter of the grid keeps, for the next tables: among the rows copied
   * so far, then in each block copied after.
   *
   * @param keep Test of the filter; `null` for all the rows.
   */
  setKept(keep: RowTest | null): void {
    void this.chain(() => {
      if (!this.closed) {
        this.keep = keep
        const kept = keep === null ? null : this.keptRows(keep, 0, this.sent)
        this.worker.postMessage({ type: 'kept', rows: kept })
      }
    })
  }

  /**
   * Computes a table on the rows copied so far, once their copy is over, or lays it out again when
   * only its order or its layout changes. Asked again after new rows, it adds them to its state.
   *
   * @param request Table asked.
   * @param order Order of its lines and of its columns.
   * @param layout Form of its lines and totals shown.
   * @param targets Columns of values named by their values, whose positions the table gives.
   * @throws {PivotError} If the engine refuses it, or refused a block.
   */
  async compute(
    request: PivotRequest,
    order: PivotOrder = LABEL_ORDER,
    layout: PivotLayout = FIRST_LAYOUT,
    targets: PivotValueTarget[] = [],
  ): Promise<PivotTable> {
    await this.copy
    // The Worker takes in every row sent before the table.
    const received = this.sent
    const id = this.nextId++
    const answer = await this.ask({ type: 'compute', id, request, order, layout, targets })
    if (answer.type !== 'table') {
      throw new Error(`Pivot client: unexpected answer ${answer.type}`)
    }
    const { shape, positions, lengths, durationMs } = answer
    return { id, shape, positions, lengths, durationMs, received }
  }

  /**
   * Reads a window of a table.
   *
   * @param table Id of the table (`PivotTable`).
   * @param lines Lines, start included, end excluded.
   * @param columns Columns of values, start included, end excluded.
   * @throws {PivotError} If another table has replaced it since (`stale`).
   */
  async window(
    table: number,
    lines: [number, number],
    columns: [number, number],
  ): Promise<PivotWindow> {
    await this.copy
    const answer = await this.ask({ type: 'window', id: this.nextId++, table, lines, columns })
    if (answer.type !== 'window') {
      throw new Error(`Pivot client: unexpected answer ${answer.type}`)
    }
    return answer.window
  }

  /**
   * Reads the distinct values of a column among the rows copied so far, in the order of the grid,
   * or the periods of a precision of its dates, in the order of time.
   *
   * @param column Index of the column.
   * @param precision What the column keeps of its dates.
   * @throws {PivotError} If the engine refuses it, or refused a block.
   */
  async values(column: number, precision: DatePrecision = 'none'): Promise<Cell[]> {
    await this.copy
    const answer = await this.ask({ type: 'values', id: this.nextId++, column, precision })
    if (answer.type !== 'values') {
      throw new Error(`Pivot client: unexpected answer ${answer.type}`)
    }
    return answer.values
  }

  /**
   * Stops the Worker; the answers still awaited are refused.
   */
  close(): void {
    this.closed = true
    this.worker.terminate()
    for (const pending of this.pending.values()) {
      pending.reject(new Error('Pivot client closed'))
    }
    this.pending.clear()
  }
}
