/**
 * TEMPORARY TOOL, TO REMOVE: migration of the keys of the local storage.
 *
 * - Added in: the version after v0.11.0 (v0.12.0).
 * - Moves the keys written by: Prisme v0.11.0 and every earlier version, and
 *   the development builds after v0.11.0, never released (commits 54cb888 to
 *   0796b54).
 * - To remove in: v0.13.0 or a later version. The installations started at
 *   least once with v0.12.0 have no old key left; the others only lose
 *   display preferences (sizes, folds, filters, theme).
 * - Removing it: this file, its import at the top of `main.ts`, its tests
 *   (`utils/__tests__/storage.spec.ts`) and its paragraph in
 *   `docs/DEVELOPER.md`, section "Local storage".
 *
 * Since v0.12.0, the keys follow `prisme.<area>.<setting>`. This module runs
 * once, as the interface starts (`migrateStorage`, run when `main.ts` imports
 * it, before any module reads a key), and moves what the earlier builds kept:
 *
 * - v0.11.0 and earlier: the preferences under their old names
 *   (`prisme.objectsOpen`, `prisme.consoleTab`, `prisme-color-mode`...),
 *   moved under their new names; the layouts of the splitter of Reka UI
 *   (`reka:prisme.layout`, `reka:prisme.centerLayout`,
 *   `reka:prisme.cellLayout.<editor>`), in percent of their group, which no
 *   size of Prisme's splitter can be made from: removed.
 * - The development builds: the sizes of the panels of the layout in one key
 *   (`prisme.panelSizes`) and the layout of each editor
 *   (`prisme.cellLayout.<editor>`), moved under their new names.
 *
 * A value already kept under its new name stays; the old key goes in every
 * case.
 */

/**
 * Old keys moved under a new name, their values unchanged.
 */
const RENAMED: Record<string, string> = {
  'prisme.objectsOpen': 'prisme.navigator.open',
  'prisme.dataExplorerWrap': 'prisme.explorer.wrap',
  'prisme.consoleOpen': 'prisme.console.open',
  'prisme.consoleTab': 'prisme.console.tab',
  'prisme.consoleFiltersShown': 'prisme.console.filtersShown',
  'prisme.consoleHistoryFilter': 'prisme.console.historyFilter',
  'prisme.consoleDebugFilter': 'prisme.console.debugFilter',
  'prisme.activeEditor': 'prisme.editor.active',
  'prisme-color-mode': 'prisme.app.colorMode',
}

/**
 * Old prefix of the layout of each editor, and its new one.
 */
const EDITOR_LAYOUT = { old: 'prisme.cellLayout.', new: 'prisme.editor.layout.' }

/**
 * Old key of the sizes of the panels of the layout, and the new key of each
 * size it held.
 */
const PANEL_SIZES = {
  key: 'prisme.panelSizes',
  sizes: {
    navigator: 'prisme.navigator.width',
    explorer: 'prisme.explorer.width',
    console: 'prisme.console.height',
  } as Record<string, string>,
}

/**
 * Prefix of the layouts that the splitter of Reka UI kept.
 */
const REKA_PREFIX = 'reka:'

/**
 * Keeps a value under its new key, unless one is kept there already.
 *
 * @param storage Local storage.
 * @param key New key.
 * @param value Value of the old key.
 */
function keep(storage: Storage, key: string, value: string): void {
  if (storage.getItem(key) === null) {
    storage.setItem(key, value)
  }
}

/**
 * Moves the keys kept by the earlier builds under the keys of
 * `prisme.<area>.<setting>`, and removes those nothing reads any more.
 */
export function migrateStorage(): void {
  try {
    const storage = window.localStorage
    const keys = Array.from({ length: storage.length }, (_, index) => storage.key(index))
    for (const key of keys) {
      if (key === null) {
        continue
      }
      const value = storage.getItem(key)
      if (value !== null && key in RENAMED) {
        keep(storage, RENAMED[key]!, value)
      } else if (value !== null && key.startsWith(EDITOR_LAYOUT.old)) {
        keep(storage, EDITOR_LAYOUT.new + key.slice(EDITOR_LAYOUT.old.length), value)
      } else if (value !== null && key === PANEL_SIZES.key) {
        let sizes: unknown = null
        try {
          sizes = JSON.parse(value)
        } catch {
          // Unreadable: nothing to keep.
        }
        for (const [panel, newKey] of Object.entries(PANEL_SIZES.sizes)) {
          const size = (sizes as Record<string, unknown> | null)?.[panel]
          if (typeof size === 'number' && Number.isFinite(size) && size > 0) {
            keep(storage, newKey, String(size))
          }
        }
      } else if (!key.startsWith(REKA_PREFIX)) {
        continue
      }
      storage.removeItem(key)
    }
  } catch {
    // Storage unavailable: nothing was kept.
  }
}

// Run once, on import: `main.ts` imports this module first.
migrateStorage()
