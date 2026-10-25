/**
 * Console, at the bottom of the screen: its state (open, tab shown), the
 * history of the queries sent to the target servers and the debug log,
 * read from the server then kept up to date as they grow
 * (`api/console.ts`).
 *
 * The console is open on its History tab by default; its state is a
 * display preference specific to this machine, kept in the browser's local
 * storage. The interface keeps the last `MAX_ENTRIES` elements of each
 * list; the older ones are read again on request (`loadOlder`). Each list
 * is cleared on request (`clear`), in every window.
 *
 * Each list has its filter (`historyFilter`, `debugFilter`), kept in local
 * storage too: the server reads only the elements it keeps, and the
 * elements received meanwhile go through it. A change of filter reads the
 * list again (`refilter`).
 */

import { defineStore } from 'pinia'
import { reactive, ref, shallowRef, watch } from 'vue'

import {
  type DebugFilter,
  type HistoryFilter,
  clearDebug,
  clearHistory,
  followConsole,
  listDebug,
  listHistory,
} from '@/api/console'
import { errorMessage } from '@/api/errorMessage'
import { ACTION_LABELS, LEVEL_LABELS, SOURCE_LABELS } from '@/components/consoleFormat'

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
 * Keys of the filters of the history and of the debug log in local storage.
 */
const HISTORY_FILTER_KEY = 'prisme.consoleHistoryFilter'
const DEBUG_FILTER_KEY = 'prisme.consoleDebugFilter'

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
 * Filter remembered under `key`: for each of its fields, the values
 * remembered that still exist, none when unreadable. A value that no longer
 * exists would make the server refuse the page.
 *
 * @param key Key in local storage.
 * @param known Values of each field, as the keys of their labels.
 */
function loadFilter<F extends { [K in keyof F]: string[] }>(
  key: string,
  known: Record<keyof F, Record<string, string>>,
): F {
  let saved: unknown = null
  try {
    saved = JSON.parse(window.localStorage.getItem(key) ?? 'null')
  } catch {
    // Unreadable: no filter.
  }
  const filter: Record<string, string[]> = {}
  for (const [field, labels] of Object.entries<Record<string, string>>(known)) {
    const values: unknown = (saved as Record<string, unknown> | null)?.[field]
    filter[field] = Array.isArray(values)
      ? values.filter(
          (value): value is string =>
            typeof value === 'string' && Object.keys(labels).includes(value),
        )
      : []
  }
  return filter as F
}

/**
 * Remembers a filter under `key`.
 *
 * @param key Key in local storage.
 * @param filter Filter to remember.
 */
function saveFilter(key: string, filter: HistoryFilter | DebugFilter): void {
  try {
    window.localStorage.setItem(key, JSON.stringify(filter))
  } catch {
    // Display preference only: losing it harms nothing.
  }
}

/**
 * Does `filter` keep `entry`: is each of its fields among the values kept,
 * for the fields that keep some?
 *
 * @param filter Values kept, per field.
 * @param entry Element of a list.
 */
export function matches<T>(filter: { [K in keyof T]?: readonly T[K][] }, entry: T): boolean {
  return (Object.keys(filter) as (keyof T)[]).every((field) => {
    const values = filter[field]
    return values === undefined || values.length === 0 || values.includes(entry[field])
  })
}

/**
 * Does `filter` keep only some of the elements?
 *
 * @param filter Values kept, per field.
 */
export function isFiltering(filter: HistoryFilter | DebugFilter): boolean {
  return Object.values(filter).some((values: string[]) => values.length > 0)
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
   * Number of changes of the filter so far: each one shows the list again
   * from its most recent elements.
   */
  refilters: number
  /**
   * Reads the most recent elements again, keeping those received meanwhile.
   */
  reload: () => Promise<void>
  /**
   * Reads the page of elements before the oldest one shown.
   */
  loadOlder: () => Promise<void>
  /**
   * Keeps the elements shown that the new filter keeps, then reads the most
   * recent ones again. The reads still running are ignored.
   */
  refilter: () => Promise<void>
  /**
   * Adds an element received, if the filter keeps it, keeping the last
   * `MAX_ENTRIES`.
   */
  add: (entry: T) => void
  /**
   * Deletes all the elements on the server, then empties the list.
   */
  clear: () => Promise<void>
  /**
   * Empties the list, whose elements were deleted on the server.
   */
  empty: () => void
  /**
   * Hides the failure of the last read.
   */
  dismissError: () => void
}

/**
 * List of the console read by pages.
 *
 * @param readPage Reads a page of the elements the filter keeps: those
 *     before `before`, `limit` at most.
 * @param deleteAll Deletes all the elements on the server.
 * @param accepts Does the filter keep this element? All of them by default.
 */
export function pagedList<T extends { id: number }>(
  readPage: (before: number | undefined, limit: number) => Promise<T[]>,
  deleteAll: () => Promise<void>,
  accepts: (entry: T) => boolean = () => true,
): PagedList<T> {
  const entries = shallowRef<T[]>([])
  const hasOlder = ref(false)
  const error = ref<string | null>(null)
  const refilters = ref(0)

  async function read(before: number | undefined, merge: (page: T[]) => T[]): Promise<void> {
    // A page read under a previous filter is out of date.
    const filterAtStart = refilters.value
    try {
      const page = await readPage(before, PAGE)
      if (filterAtStart !== refilters.value) {
        return
      }
      entries.value = merge(page)
      hasOlder.value = page.length === PAGE
      error.value = null
    } catch (failure) {
      if (filterAtStart === refilters.value) {
        error.value = errorMessage(failure)
      }
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

  function refilter(): Promise<void> {
    refilters.value++
    entries.value = entries.value.filter(accepts)
    return reload()
  }

  function add(entry: T): void {
    if (!accepts(entry) || entries.value[entries.value.length - 1]?.id === entry.id) {
      return
    }
    const next = [...entries.value, entry]
    if (next.length > MAX_ENTRIES) {
      next.splice(0, next.length - MAX_ENTRIES)
      hasOlder.value = true
    }
    entries.value = next
  }

  function empty(): void {
    entries.value = []
    hasOlder.value = false
  }

  async function clear(): Promise<void> {
    try {
      await deleteAll()
      empty()
      error.value = null
    } catch (failure) {
      error.value = errorMessage(failure)
    }
  }

  function dismissError(): void {
    error.value = null
  }

  return reactive({
    entries,
    hasOlder,
    error,
    refilters,
    reload,
    loadOlder,
    refilter,
    add,
    clear,
    empty,
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
   * Queries kept by the history.
   */
  const historyFilter = ref(
    loadFilter<HistoryFilter>(HISTORY_FILTER_KEY, { action: ACTION_LABELS }),
  )

  /**
   * Lines kept by the debug log.
   */
  const debugFilter = ref(
    loadFilter<DebugFilter>(DEBUG_FILTER_KEY, { level: LEVEL_LABELS, source: SOURCE_LABELS }),
  )

  /**
   * History of the queries.
   */
  const history = pagedList(
    (before, limit) => listHistory(historyFilter.value, before, limit),
    clearHistory,
    (entry) => matches(historyFilter.value, entry),
  )

  /**
   * Debug log.
   */
  const debug = pagedList(
    (before, limit) => listDebug(debugFilter.value, before, limit),
    clearDebug,
    (entry) => matches(debugFilter.value, entry),
  )

  watch(
    historyFilter,
    (filter) => {
      saveFilter(HISTORY_FILTER_KEY, filter)
      void history.refilter()
    },
    { deep: true },
  )

  watch(
    debugFilter,
    (filter) => {
      saveFilter(DEBUG_FILTER_KEY, filter)
      void debug.refilter()
    },
    { deep: true },
  )

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
      onHistoryCleared: history.empty,
      onDebugCleared: debug.empty,
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

  return { open, tab, historyFilter, debugFilter, history, debug, start, dispose }
})
