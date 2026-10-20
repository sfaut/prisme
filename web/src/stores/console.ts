/**
 * Console, at the bottom of the screen: its state (open, tab shown) and
 * the history of the queries sent to the target servers, read from the
 * server then kept up to date as it grows (`api/console.ts`).
 *
 * The console is open on its History tab by default; its state is a
 * display preference specific to this machine, kept in the browser's local
 * storage. The interface keeps the last `MAX_ENTRIES` queries; the older
 * ones are read again on request (`loadOlder`).
 */

import { defineStore } from 'pinia'
import { ref, shallowRef, watch } from 'vue'

import { followConsole, listHistory } from '@/api/console'
import { errorMessage } from '@/api/errorMessage'
import type { QueryEntry } from '@/api/generated/QueryEntry'

import { loadFlag, saveFlag } from './layout'

/**
 * Key of the console's state in local storage.
 */
const OPEN_KEY = 'prisme.consoleOpen'

/**
 * Most queries kept by the interface.
 */
export const MAX_ENTRIES = 2000

/**
 * Number of queries read per page.
 */
const PAGE = 500

/**
 * Tabs of the console.
 */
export type ConsoleTab = 'history'

export const useConsoleStore = defineStore('console', () => {
  /**
   * Is the console open?
   */
  const open = ref(loadFlag(OPEN_KEY, true))

  watch(open, (value) => saveFlag(OPEN_KEY, value))

  /**
   * Tab shown.
   */
  const tab = ref<ConsoleTab>('history')

  /**
   * Queries of the history, in chronological order. The array is replaced
   * as a whole, never changed in place.
   */
  const entries = shallowRef<QueryEntry[]>([])

  /**
   * Are there older queries on the server?
   */
  const hasOlder = ref(false)

  /**
   * Failure of the last read, `null` if none.
   */
  const error = ref<string | null>(null)

  /**
   * Stops following the events, `null` when not following.
   */
  let stop: (() => void) | null = null

  /**
   * Reads the most recent queries again, replacing those shown.
   */
  async function reload(): Promise<void> {
    try {
      const page = await listHistory(undefined, PAGE)
      // Queries received while reading stay, when the page does not have them.
      const last = page[page.length - 1]?.id ?? 0
      entries.value = [...page, ...entries.value.filter((entry) => entry.id > last)]
      hasOlder.value = page.length === PAGE
      error.value = null
    } catch (failure) {
      error.value = errorMessage(failure)
    }
  }

  /**
   * Reads the page of queries before the oldest one shown.
   */
  async function loadOlder(): Promise<void> {
    const first = entries.value[0]
    if (first === undefined) {
      return reload()
    }
    try {
      const page = await listHistory(first.id, PAGE)
      entries.value = [...page, ...entries.value]
      hasOlder.value = page.length === PAGE
      error.value = null
    } catch (failure) {
      error.value = errorMessage(failure)
    }
  }

  /**
   * Adds a query received, keeping the last `MAX_ENTRIES`.
   *
   * @param entry Query received.
   */
  function add(entry: QueryEntry): void {
    if (entries.value[entries.value.length - 1]?.id === entry.id) {
      return
    }
    const next = [...entries.value, entry]
    if (next.length > MAX_ENTRIES) {
      next.splice(0, next.length - MAX_ENTRIES)
      hasOlder.value = true
    }
    entries.value = next
  }

  /**
   * Starts following the history: reads it, then follows its growth. No
   * effect if already following.
   */
  function start(): void {
    if (stop !== null) {
      return
    }
    stop = followConsole({ onQuery: add, onGap: () => void reload() })
    void reload()
  }

  /**
   * Stops following the history.
   */
  function dispose(): void {
    stop?.()
    stop = null
  }

  return { open, tab, entries, hasOlder, error, start, dispose, reload, loadOlder, add }
})
