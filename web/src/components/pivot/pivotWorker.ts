/**
 * Worker of the Pivoteur: runs the engine (`crates/web-engine`, compiled to WebAssembly) off the
 * main thread, on the copy of the rows of one result, and answers the messages of the page
 * (`pivotClient.ts`) in their order (`answerMessage` in `pivotEngine.ts`).
 *
 * A crash of the module (a panic of the engine) leaves its memory in an unknown state: the Worker
 * then answers each message with a `crash` refusal, and the page starts another one.
 */

import engineUrl from '@pivot-engine?url'

import {
  type PivotAnswer,
  type PivotFailure,
  type PivotRequestMessage,
  type WorkerState,
  PivotEngine,
  answerMessage,
} from './pivotEngine'

/**
 * Engine of the Worker, once its module is compiled; the refusal of every message otherwise.
 */
const ready: Promise<WorkerState | PivotFailure> = fetch(engineUrl)
  .then((response) => response.arrayBuffer())
  .then((bytes) => WebAssembly.compile(bytes))
  .then(async (module) => ({
    engine: await PivotEngine.create(module),
    columns: 0,
    table: null,
  }))
  .catch((error: unknown) => ({ kind: 'module', detail: String(error) }))

/**
 * Messages handled so far: each waits for the previous one.
 */
let queue: Promise<WorkerState | PivotFailure> = ready

self.onmessage = (event: MessageEvent<PivotRequestMessage>) => {
  const message = event.data
  queue = queue.then((state) => {
    const id = 'id' in message ? message.id : null
    let answer: PivotAnswer | null
    if ('engine' in state) {
      try {
        answer = answerMessage(state, message)
      } catch (error) {
        state = { kind: 'crash', detail: String(error) }
        answer = { type: 'failure', id, failure: state }
      }
    } else {
      answer = { type: 'failure', id, failure: state }
    }
    if (answer !== null) {
      self.postMessage(answer)
    }
    return state
  })
}
