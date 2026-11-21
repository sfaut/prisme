/**
 * Display settings of the user, common to all the editors: the theme, the
 * active editor, the tab and the filters of the console, the wrapping of the
 * data explorers. The stores of these areas read and write them here.
 *
 * They are saved on the server side (`GET` / `PUT /api/display`,
 * `crates/server/src/user_display.rs`), read before the interface is mounted
 * (`main.ts`), and saved shortly after each change (`utils/deferredSave.ts`).
 * Until they are read, or when they cannot be, the defaults apply and
 * nothing is saved. The theme is applied by `utils/colorMode.ts`, which keeps
 * a copy of it in the browser for the first paint of the next visit: the
 * two stay in step both ways. The schema is documented in
 * `docs/DEVELOPER.md`, section "Saved display".
 */

import { defineStore } from 'pinia'
import { nextTick, reactive, ref, watch } from 'vue'

import { getUserDisplay, saveUserDisplay } from '@/api/display'
import type { UserDisplay } from '@/api/generated/UserDisplay'
import { colorMode } from '@/utils/colorMode'
import { deferredSave, removeStoredKeys } from '@/utils/deferredSave'

/**
 * Keys under which these settings were kept in the browser before being
 * saved on the server side; they are removed. The copy of the theme
 * (`prisme.app.colorMode`) stays.
 */
const LEGACY_KEYS = new Set([
  'prisme.editor.active',
  'prisme.console.allEditors',
  'prisme.console.tab',
  'prisme.console.filtersShown',
  'prisme.console.historyFilter',
  'prisme.console.debugFilter',
  'prisme.explorer.wrap',
])

/**
 * Display settings of a user who has saved none.
 */
export function defaultUserDisplay(): UserDisplay {
  return {
    theme: 'system',
    active_editor: null,
    console: {
      tab: 'history',
      all_editors: false,
      filters_open: false,
      history_filter: { action: [] },
      debug_filter: { level: [], source: [] },
    },
    explorer: { wrap: false },
  }
}

export const useUserDisplayStore = defineStore('userDisplay', () => {
  removeStoredKeys((key) => LEGACY_KEYS.has(key))

  /**
   * Display settings of the user, the theme being that of the page.
   */
  const display = reactive<UserDisplay>({ ...defaultUserDisplay(), theme: colorMode.value })

  /**
   * Were the settings read from the server? Only then are changes saved.
   */
  const loaded = ref(false)

  const saves = deferredSave<'user'>(async (_key, keepalive) => {
    await saveUserDisplay(display, { keepalive })
  })

  /**
   * Reads the settings from the server; on failure, the defaults stay.
   */
  async function load(): Promise<void> {
    try {
      Object.assign(display, await getUserDisplay())
      // Saves start with the changes that follow, not with the reading.
      await nextTick()
      loaded.value = true
    } catch {
      // Unreadable: the defaults, and the theme of the page, stay.
    }
  }

  watch(
    display,
    () => {
      if (loaded.value) {
        saves.schedule('user')
      }
    },
    { deep: true },
  )

  // The theme of the page and that of the settings stay in step.
  watch(
    () => display.theme,
    (theme) => (colorMode.value = theme),
  )
  watch(colorMode, (mode) => (display.theme = mode))

  return { display, loaded, load }
})
