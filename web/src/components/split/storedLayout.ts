/**
 * Layout of the panels of a splitter of Prisme, kept in
 * the browser's local storage under a key of Prisme: per panel, a length in
 * CSS pixels (`null` for its default size) or whether it is folded. A value of
 * the wrong kind, or missing, gives its default. Forgetting a layout removes
 * its key (`forgetStoredLayout`). If storage is unavailable (private browsing,
 * blocked storage), the layout lives as long as the page.
 */
import { reactive, watch } from 'vue'

/**
 * Values of a layout: sizes in CSS pixels (`null`: default size) and folds.
 */
export type StoredLayout = Record<string, number | boolean | null>

/**
 * Layout kept under a key, as a reactive record whose changes are saved.
 *
 * @param key Key of the local storage.
 * @param defaults
 *     Value of each entry without a saved one: a boolean for a fold, `null`
 *     for a size.
 */
export function useStoredLayout<Layout extends StoredLayout>(
  key: string,
  defaults: Layout,
): Layout {
  const layout = reactive({ ...defaults }) as Layout
  try {
    const saved: unknown = JSON.parse(window.localStorage.getItem(key) ?? '{}')
    if (typeof saved === 'object' && saved !== null) {
      for (const name of Object.keys(defaults) as (keyof Layout & string)[]) {
        const value = (saved as Record<string, unknown>)[name]
        const fold = typeof defaults[name] === 'boolean'
        if (
          (fold && typeof value === 'boolean') ||
          (!fold && typeof value === 'number' && Number.isFinite(value) && value > 0)
        ) {
          layout[name] = value as Layout[typeof name]
        }
      }
    }
  } catch {
    // Storage unavailable or unreadable: the defaults.
  }
  watch(
    layout,
    (value) => {
      try {
        window.localStorage.setItem(key, JSON.stringify(value))
      } catch {
        // Storage unavailable: the layout lives as long as the page.
      }
    },
    { deep: true },
  )
  return layout
}

/**
 * Forgets a layout kept under a key.
 *
 * @param key Key of the local storage.
 */
export function forgetStoredLayout(key: string): void {
  try {
    window.localStorage.removeItem(key)
  } catch {
    // Storage unavailable: nothing was kept.
  }
}

/**
 * Prefix of the layouts that the splitter of Reka UI kept, before the splitter of Prisme.
 */
const REKA_PREFIX = 'reka:'

/**
 * Forgets the layouts the splitter of Reka UI kept (`reka:prisme.layout`,
 * `reka:prisme.cellLayout.1`...), which nothing reads any more: once per
 * page, as the interface starts.
 */
export function forgetRekaLayouts(): void {
  try {
    const storage = window.localStorage
    const keys = Array.from({ length: storage.length }, (_, index) => storage.key(index))
    for (const key of keys) {
      if (key?.startsWith(REKA_PREFIX)) {
        storage.removeItem(key)
      }
    }
  } catch {
    // Storage unavailable: nothing was kept.
  }
}
