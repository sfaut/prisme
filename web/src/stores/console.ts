/**
 * Console, at the bottom of each editor's tab: its state (tab shown,
 * filters), the history of the queries sent to the target servers and the
 * debug log, read from the server then kept up to date as they grow
 * (`api/console.ts`).
 *
 * Each editor's tab has its console, open by default: whether it is open
 * and its height belong to the display of the editor (`editorDisplays`). The
 * lists are those of the editor on screen (`follow`): the elements its
 * session wrote, or, on request, those of all the editors and of Prisme
 * itself (`allEditors`, off by default). They are followed while a console
 * is on screen only.
 *
 * The console opens on its History tab by default; the tab shown, the
 * filters and `allEditors` are common to all the consoles, saved with the
 * display settings of the user (`userDisplay` store). The interface keeps
 * the last
 * `MAX_ENTRIES` elements of each list; the older ones are read again on
 * request (`loadOlder`). Each list is cleared on request (`clear`), in every
 * window.
 *
 * Each list has its filter (`historyFilter`, `debugFilter`), saved there
 * too: the server reads only the elements it keeps, and the
 * elements received meanwhile go through it. A change of filter reads the
 * list again (`refilter`). The buttons of the filters show on request
 * (`filtersShown`, hidden by default); hidden, the filters still apply.
 */

import { defineStore } from 'pinia'
import { computed, reactive, ref, shallowRef, watch } from 'vue'

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
import { ACTION_LABELS, LEVEL_LABELS, SOURCE_LABELS } from '@/components/console/consoleFormat'

import { useUserDisplayStore } from '@/stores/userDisplay'

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
 * Removes from a saved filter, in place, the values that no longer exist: a
 * value that no longer exists would make the server refuse the page.
 *
 * @param filter Saved filter, its fields lists of values.
 * @param known Values of each field, as the keys of their labels.
 */
function dropUnknown(
  filter: Record<string, string[]>,
  known: Record<string, Record<string, string>>,
): void {
  for (const [field, labels] of Object.entries(known)) {
    const values = filter[field] ?? []
    if (values.some((value) => !(value in labels))) {
      filter[field] = values.filter((value) => value in labels)
    }
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
   * Deletes the elements of the list on the server, then empties it.
   */
  clear: () => Promise<void>
  /**
   * Empties the list, whose elements were deleted on the server, or removes
   * those that `deleted` keeps.
   */
  empty: (deleted?: (entry: T) => boolean) => void
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
 * @param deleteAll Deletes the elements of the list on the server.
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

  function empty(deleted?: (entry: T) => boolean): void {
    if (deleted === undefined) {
      entries.value = []
      hasOlder.value = false
    } else {
      entries.value = entries.value.filter((entry) => !deleted(entry))
    }
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
   * Editor whose console is on screen, the last one while none is.
   */
  const editorId = ref<number | null>(null)

  /**
   * Display settings of the user, which hold those of the console.
   */
  const user = useUserDisplayStore()

  /**
   * Do the lists show the elements of all the editors, and those written
   * outside an editor?
   */
  const allEditors = computed({
    get: () => user.display.console.all_editors,
    set: (value: boolean) => (user.display.console.all_editors = value),
  })

  /**
   * Editor whose elements the lists show; `null`, all of them.
   */
  const scope = computed(() => (allEditors.value ? null : editorId.value))

  /**
   * Is an element one of those the lists show?
   *
   * @param entry Element of a list.
   */
  function inScope(entry: { editor_id: number }): boolean {
    return scope.value === null || entry.editor_id === scope.value
  }

  /**
   * Tab shown.
   */
  const tab = computed({
    get: (): ConsoleTab => user.display.console.tab,
    set: (value: ConsoleTab) => (user.display.console.tab = value),
  })

  /**
   * Are the buttons of the filters shown, in both tabs?
   */
  const filtersShown = computed({
    get: () => user.display.console.filters_open,
    set: (value: boolean) => (user.display.console.filters_open = value),
  })

  dropUnknown(user.display.console.history_filter, { action: ACTION_LABELS })
  dropUnknown(user.display.console.debug_filter, { level: LEVEL_LABELS, source: SOURCE_LABELS })

  /**
   * Queries kept by the history, its values known (`dropUnknown`).
   */
  const historyFilter = computed({
    get: () => user.display.console.history_filter as HistoryFilter,
    set: (filter: HistoryFilter) => (user.display.console.history_filter = filter),
  })

  /**
   * Lines kept by the debug log, its values known (`dropUnknown`).
   */
  const debugFilter = computed({
    get: () => user.display.console.debug_filter as DebugFilter,
    set: (filter: DebugFilter) => (user.display.console.debug_filter = filter),
  })

  /**
   * History of the queries.
   */
  const history = pagedList(
    (before, limit) => listHistory(historyFilter.value, scope.value, before, limit),
    () => clearHistory(scope.value),
    (entry) => inScope(entry) && matches(historyFilter.value, entry),
  )

  /**
   * Debug log.
   */
  const debug = pagedList(
    (before, limit) => listDebug(debugFilter.value, scope.value, before, limit),
    () => clearDebug(scope.value),
    (entry) => inScope(entry) && matches(debugFilter.value, entry),
  )

  watch(historyFilter, () => void history.refilter(), { deep: true })

  watch(debugFilter, () => void debug.refilter(), { deep: true })

  /**
   * Stops following the console's events, `null` while they are not.
   */
  let stop: (() => void) | null = null

  /**
   * Shows the elements read again in both lists, those of the scope.
   */
  function refilterAll(): void {
    void history.refilter()
    void debug.refilter()
  }

  watch(allEditors, () => {
    if (stop !== null) {
      refilterAll()
    }
  })

  /**
   * Takes into account elements cleared on the server: all of them, or
   * those of an editor.
   *
   * @param list List cleared.
   * @param cleared Id of the editor whose elements were cleared; `null`, all
   *     of them.
   */
  function forget<T extends { id: number; editor_id: number }>(
    list: PagedList<T>,
    cleared: number | null,
  ): void {
    if (cleared === null || cleared === scope.value) {
      list.empty()
    } else if (scope.value === null) {
      list.empty((entry) => entry.editor_id === cleared)
    }
  }

  /**
   * Shows the console of an editor: its lists are read then followed, those
   * of this editor unless `allEditors`. `null`: no console on screen, the
   * lists are no longer followed.
   *
   * @param id Id of the editor whose console is on screen, `null` if none.
   */
  function follow(id: number | null): void {
    if (id === null) {
      stop?.()
      stop = null
      return
    }
    const moved = id !== editorId.value
    editorId.value = id
    if (stop === null) {
      stop = followConsole({
        onQuery: history.add,
        onLine: debug.add,
        onHistoryCleared: (cleared) => forget(history, cleared),
        onDebugCleared: (cleared) => forget(debug, cleared),
        onGap: () => {
          void history.reload()
          void debug.reload()
        },
      })
      refilterAll()
    } else if (moved && !allEditors.value) {
      refilterAll()
    }
  }

  return {
    allEditors,
    scope,
    tab,
    filtersShown,
    historyFilter,
    debugFilter,
    history,
    debug,
    follow,
  }
})
