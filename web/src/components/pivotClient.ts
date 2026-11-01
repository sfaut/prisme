/**
 * Page side of the Pivoteur's Worker (`pivotWorker.ts`): one Worker per result, fed once with a
 * copy of its rows, then asked for tables and for the windows of the table shown.
 *
 * The rows go to the Worker in blocks, with a pause between two blocks so that the page keeps
 * answering. The other messages wait for the end of the copy: a table asked during it is
 * computed on all the rows. Closing the client stops the Worker, and with it the memory of its
 * module.
 */

import { toRaw } from 'vue'

import type { TypeFamily } from '@/api/generated/TypeFamily'
import type { QueryResult } from '@/api/querySession'

import {
  type PivotAnswer,
  type PivotFailure,
  type PivotRequest,
  type PivotRequestMessage,
  type PivotShape,
  type PivotWindow,
  PivotError,
} from './pivotEngine'

/**
 * Rows per block sent to the Worker.
 */
export const BLOCK_ROWS = 20_000

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
   * Duration of the computation in the Worker, in milliseconds.
   */
  durationMs: number
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
   * Refusal of a block of rows, given with the end of the copy.
   */
  private blockFailure: PivotFailure | null = null

  /**
   * Has the client been closed?
   */
  private closed = false

  /**
   * Copy of the rows of the result into the Worker: resolved once the Worker holds them all,
   * refused with the refusal of a block.
   */
  readonly loaded: Promise<void>

  /**
   * Starts the copy of the rows of a result into a Worker.
   *
   * @param worker Worker of the engine.
   * @param result Result to copy, received in full.
   */
  constructor(
    private readonly worker: PivotWorkerPort,
    result: QueryResult,
  ) {
    worker.onmessage = (event) => this.receive(event.data)
    this.loaded = this.load(result)
    // A refused copy is given to the calls that wait for it.
    this.loaded.catch(() => undefined)
  }

  /**
   * Creates the client of a result, with a Worker of its own.
   *
   * @param result Result to copy, received in full.
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
    if (answer.type === 'failure') {
      pending?.reject(new PivotError(this.blockFailure ?? answer.failure))
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
   * Copies the rows of a result into the Worker, block by block.
   *
   * @param result Result to copy.
   */
  private async load(result: QueryResult): Promise<void> {
    const rows = toRaw(result.rows)
    this.worker.postMessage({ type: 'begin', families: families(result) })
    for (let start = 0; start < rows.length; start += BLOCK_ROWS) {
      if (start > 0) {
        await pause()
      }
      if (this.closed) {
        return
      }
      this.worker.postMessage({ type: 'rows', rows: rows.slice(start, start + BLOCK_ROWS) })
    }
    await this.ask({ type: 'finish', id: this.nextId++ })
  }

  /**
   * Keeps only the rows the filter of the grid keeps, for the next tables, once the copy is
   * over.
   *
   * @param rows Rows kept, in their order; `null` for all.
   */
  setKept(rows: number[] | null): void {
    const kept = rows === null ? null : Uint32Array.from(rows)
    this.loaded.then(
      () => {
        if (!this.closed) {
          this.worker.postMessage({ type: 'kept', rows: kept })
        }
      },
      // The tables refuse the copy.
      () => undefined,
    )
  }

  /**
   * Computes a table, once the copy is over.
   *
   * @param request Table asked.
   * @throws {PivotError} If the engine refuses it, or refused the copy.
   */
  async compute(request: PivotRequest): Promise<PivotTable> {
    await this.loaded
    const id = this.nextId++
    const answer = await this.ask({ type: 'compute', id, request })
    if (answer.type !== 'table') {
      throw new Error(`Pivot client: unexpected answer ${answer.type}`)
    }
    return { id, shape: answer.shape, durationMs: answer.durationMs }
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
    await this.loaded
    const answer = await this.ask({ type: 'window', id: this.nextId++, table, lines, columns })
    if (answer.type !== 'window') {
      throw new Error(`Pivot client: unexpected answer ${answer.type}`)
    }
    return answer.window
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
