/**
 * Synchronisation with the internal database, as shown in the top bar.
 *
 * Every write sent to the server is counted while it runs (`trackWrite`, used
 * by `request` for each method other than GET). When the server refuses one,
 * the state is `failed` until a later write is accepted. The text of the
 * editors is saved with a delay: its changes are declared here (`markPending`)
 * until their save ends.
 */

import { computed, reactive, ref } from 'vue'

/**
 * `synced` when nothing is left to save, `pending` while a save runs or waits,
 * `failed` after a refused write.
 */
export type SyncState = 'synced' | 'pending' | 'failed'

/**
 * Writes running.
 */
const running = ref(0)

/**
 * Whether the last write was refused by the server.
 */
const refused = ref(false)

/**
 * Changes waiting to be saved, by key (`markPending`).
 */
const pending = reactive(new Set<string>())

/**
 * Synchronisation state, shown in the top bar.
 */
export const syncState = computed<SyncState>(() => {
  if (refused.value) {
    return 'failed'
  }
  return running.value > 0 || pending.size > 0 ? 'pending' : 'synced'
})

/**
 * Follows a write sent to the server until its answer.
 *
 * @param write The write, already sent.
 * @returns The answer of the write, unchanged.
 * @throws The failure of the write, after marking it as refused.
 */
export async function trackWrite<T>(write: Promise<T>): Promise<T> {
  running.value++
  try {
    const result = await write
    refused.value = false
    return result
  } catch (failure) {
    refused.value = true
    throw failure
  } finally {
    running.value--
  }
}

/**
 * Declares a change waiting to be saved.
 *
 * @param key Name of the change, unique for what it concerns.
 */
export function markPending(key: string): void {
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
