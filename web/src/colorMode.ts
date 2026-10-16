/**
 * Light, dark or system theme.
 *
 * The choice is remembered in the browser (`localStorage`). "System" sets no
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
 * Key of the choice in `localStorage`.
 */
const STORAGE_KEY = 'prisme-color-mode'

/**
 * Reads the remembered choice; "system" if there is none or if it is unreadable.
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
 * Applies a choice to the page: class on `<html>`, then remembers it.
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
