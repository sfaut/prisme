/**
 * Light, dark or system theme.
 *
 * The choice is saved on the server side with the display settings of the
 * user (`userDisplay` store, `users.theme`), which keep it in step with this
 * module both ways. The browser keeps a copy of it (`localStorage`), read as
 * this module loads, before the interface is mounted and the settings read:
 * the page shows the right theme from its first paint. "System" sets no
 * class: colors then follow the machine's setting (`color-scheme: light dark`
 * in `main.css`). "Light" and "Dark" set the `light` or `dark` class on
 * `<html>`, which forces one or the other.
 */

import { ref, watch } from 'vue'

/**
 * Possible choices.
 */
export type ColorMode = 'system' | 'light' | 'dark'

/**
 * Key of the copy of the choice in `localStorage`.
 */
const STORAGE_KEY = 'prisme.app.colorMode'

/**
 * Reads the copy of the choice; "system" if there is none or if it is unreadable.
 */
function storedMode(): ColorMode {
  try {
    const value = localStorage.getItem(STORAGE_KEY)
    return value === 'light' || value === 'dark' ? value : 'system'
  } catch {
    return 'system'
  }
}

/**
 * Current choice, shared by the whole application.
 */
export const colorMode = ref<ColorMode>(storedMode())

/**
 * Applies a choice to the page: class on `<html>`, then keeps its copy.
 *
 * @param mode Choice to apply.
 */
function apply(mode: ColorMode): void {
  const root = document.documentElement
  root.classList.toggle('light', mode === 'light')
  root.classList.toggle('dark', mode === 'dark')
  try {
    localStorage.setItem(STORAGE_KEY, mode)
  } catch {
    // Storage refused (private browsing): the choice holds for this visit.
  }
}

watch(colorMode, apply, { immediate: true })
