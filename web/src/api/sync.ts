/**
 * Synchronisation with the internal database, as shown in the top bar.
 *
 * Every request that reads or writes the internal database is counted while
 * it runs (`trackRead`, `trackWrite`, used by `request`). When the server
 * refuses a write, the state is `failed` until a later write is accepted; a
 * refused read leaves the state alone, its error shows where it was asked.
 * The text of the editors is saved with a delay: its changes are declared
 * here (`markPending`) until their save ends.
 */

import { computed, reactive, ref } from 'vue'

/**
 * `idle` before any exchange with the internal database, `reading` while a
 * read runs, `writing` while a save runs or waits, `failed` after a refused
 * write, `synced` when everything is read and saved.
 */
export type SyncState = 'idle' | 'reading' | 'writing' | 'failed' | 'synced'

/**
 * Reads running.
 */
const reading = ref(0)

/**
 * Writes running.
 */
const writing = ref(0)

/**
 * Whether an exchange with the internal database has started since the page opened.
 */
const exchanged = ref(false)

/**
 * Whether the last write was refused by the server.
 */
const refused = ref(false)

/**
 * Changes waiting to be saved, by key (`markPending`).
 */
const pending = reactive(new Set<string>())

/**
 * Synchronisation state, shown in the top bar. A refused write comes first,
 * then the saves, which matter more than the reads.
 */
export const syncState = computed<SyncState>(() => {
  if (refused.value) {
    return 'failed'
  }
  if (writing.value > 0 || pending.size > 0) {
    return 'writing'
  }
  if (reading.value > 0) {
    return 'reading'
  }
  return exchanged.value ? 'synced' : 'idle'
})

/**
 * Follows a read of the internal database until its answer.
 *
 * @param read The read, already sent.
 * @returns The answer of the read, unchanged.
 * @throws The failure of the read, unchanged.
 */
export async function trackRead<T>(read: Promise<T>): Promise<T> {
  exchanged.value = true
  reading.value++
  try {
    return await read
  } finally {
    reading.value--
  }
}

/**
 * Follows a write to the internal database until its answer.
 *
 * @param write The write, already sent.
 * @returns The answer of the write, unchanged.
 * @throws The failure of the write, after marking it as refused.
 */
export async function trackWrite<T>(write: Promise<T>): Promise<T> {
  exchanged.value = true
  writing.value++
  try {
    const result = await write
    refused.value = false
    return result
  } catch (failure) {
    refused.value = true
    throw failure
  } finally {
    writing.value--
  }
}

/**
 * Declares a change waiting to be saved.
 *
 * @param key Name of the change, unique for what it concerns.
 */
export function markPending(key: string): void {
  exchanged.value = true
  pending.add(key)
}

/**
 * Declares a change saved, or abandoned after a refused save.
 *
 * @param key Name given to `markPending`.
 */
export function clearPending(key: string): void {
  pending.delete(key)
}
