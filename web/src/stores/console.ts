/**
 * Console, at the bottom of the screen: its state (open, tab shown), the
 * history of the queries sent to the target servers and the debug log,
 * read from the server then kept up to date as they grow
 * (`api/console.ts`).
 *
 * The console is open on its History tab by default; its state is a
 * display preference specific to this machine, kept in the browser's local
 * storage. The interface keeps the last `MAX_ENTRIES` elements of each
 * list; the older ones are read again on request (`loadOlder`).
 */

import { defineStore } from 'pinia'
import { reactive, ref, shallowRef, watch } from 'vue'

import { followConsole, listDebug, listHistory } from '@/api/console'
import { errorMessage } from '@/api/errorMessage'

import { loadFlag, saveFlag } from './layout'

/**
 * Key of the console's state in local storage.
 */
const OPEN_KEY = 'prisme.consoleOpen'

/**
 * Key of the tab shown in local storage.
 */
const TAB_KEY = 'prisme.consoleTab'

/**
 * Most elements kept by the interface, per list.
 */
export const MAX_ENTRIES = 2000

/**
 * Number of elements read per page.
 */
const PAGE = 500

/**
 * Tabs of the console.
 */
export type ConsoleTab = 'history' | 'debug'

/**
 * Tab remembered, the history when none is readable.
 */
function loadTab(): ConsoleTab {
  try {
    return window.localStorage.getItem(TAB_KEY) === 'debug' ? 'debug' : 'history'
  } catch {
    return 'history'
  }
}

/**
 * List of the console read by pages, then followed: the history or the
 * debug log. A reactive object, whose properties are read directly.
 */
export interface PagedList<T> {
  /**
   * Elements, in chronological order. The array is replaced as a whole,
   * never changed in place.
   */
  entries: T[]
  /**
   * Are there older elements on the server?
   */
  hasOlder: boolean
  /**
   * Failure of the last read, `null` if none.
   */
  error: string | null
  /**
   * Reads the most recent elements again, keeping those received meanwhile.
   */
  reload: () => Promise<void>
  /**
   * Reads the page of elements before the oldest one shown.
   */
  loadOlder: () => Promise<void>
  /**
   * Adds an element received, keeping the last `MAX_ENTRIES`.
   */
  add: (entry: T) => void
  /**
   * Hides the failure of the last read.
   */
  dismissError: () => void
}

/**
 * List of the console read by pages.
 *
 * @param readPage Reads a page: the elements before `before`, `limit` at most.
 */
export function pagedList<T extends { id: number }>(
  readPage: (before: number | undefined, limit: number) => Promise<T[]>,
): PagedList<T> {
  const entries = shallowRef<T[]>([])
  const hasOlder = ref(false)
  const error = ref<string | null>(null)

  async function read(before: number | undefined, merge: (page: T[]) => T[]): Promise<void> {
    try {
      const page = await readPage(before, PAGE)
      entries.value = merge(page)
      hasOlder.value = page.length === PAGE
      error.value = null
    } catch (failure) {
      error.value = errorMessage(failure)
    }
  }

  function reload(): Promise<void> {
    return read(undefined, (page) => {
      const last = page[page.length - 1]?.id ?? 0
      return [...page, ...entries.value.filter((entry) => entry.id > last)]
    })
  }

  function loadOlder(): Promise<void> {
    const first = entries.value[0]
    return first === undefined ? reload() : read(first.id, (page) => [...page, ...entries.value])
  }

  function add(entry: T): void {
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

  function dismissError(): void {
    error.value = null
  }

  return reactive({
    entries,
    hasOlder,
    error,
    reload,
    loadOlder,
    add,
    dismissError,
  }) as PagedList<T>
}

export const useConsoleStore = defineStore('console', () => {
  /**
   * Is the console open?
   */
  const open = ref(loadFlag(OPEN_KEY, true))

  watch(open, (value) => saveFlag(OPEN_KEY, value))

  /**
   * Tab shown.
   */
  const tab = ref<ConsoleTab>(loadTab())

  watch(tab, (value) => {
    try {
      window.localStorage.setItem(TAB_KEY, value)
    } catch {
      // Display preference only: losing it harms nothing.
    }
  })

  /**
   * History of the queries.
   */
  const history = pagedList(listHistory)

  /**
   * Debug log.
   */
  const debug = pagedList(listDebug)

  /**
   * Stops following the events, `null` when not following.
   */
  let stop: (() => void) | null = null

  /**
   * Starts following the console: reads both lists, then follows their
   * growth. No effect if already following.
   */
  function start(): void {
    if (stop !== null) {
      return
    }
    stop = followConsole({
      onQuery: history.add,
      onLine: debug.add,
      onGap: () => {
        void history.reload()
        void debug.reload()
      },
    })
    void history.reload()
    void debug.reload()
  }

  /**
   * Stops following the console.
   */
  function dispose(): void {
    stop?.()
    stop = null
  }

  return { open, tab, history, debug, start, dispose }
})
