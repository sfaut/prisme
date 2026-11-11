/**
 * Display preferences kept in the browser's local storage, under keys
 * `prisme.<area>.<setting>` (`docs/DEVELOPER.md`, section "Local storage"):
 * a flag (`true` or `false`) or the size of a panel in CSS pixels. A value
 * missing or unreadable gives its default; when storage is unavailable
 * (private browsing, blocked storage), the defaults apply and nothing is
 * kept, which harms nothing: these are preferences of this machine only.
 */

/**
 * Reads a remembered flag.
 *
 * @param key Key in local storage.
 * @param fallback Value when nothing readable is remembered.
 */
export function loadFlag(key: string, fallback: boolean): boolean {
  try {
    const value = window.localStorage.getItem(key)
    return value === null ? fallback : value === 'true'
  } catch {
    return fallback
  }
}

/**
 * Remembers a flag; no effect if storage is unavailable.
 *
 * @param key Key in local storage.
 * @param value Value to remember.
 */
export function saveFlag(key: string, value: boolean): void {
  try {
    window.localStorage.setItem(key, String(value))
  } catch {
    // Display preference only: losing it harms nothing.
  }
}

/**
 * Reads the remembered size of a panel, in CSS pixels; `null` (its default
 * size) if none is readable.
 *
 * @param key Key in local storage.
 */
export function loadSize(key: string): number | null {
  try {
    const value = Number(window.localStorage.getItem(key) ?? Number.NaN)
    return Number.isFinite(value) && value > 0 ? value : null
  } catch {
    return null
  }
}

/**
 * Remembers the size of a panel; `null` forgets it. No effect if storage is
 * unavailable.
 *
 * @param key Key in local storage.
 * @param size Size in CSS pixels, `null` for the default size.
 */
export function saveSize(key: string, size: number | null): void {
  try {
    if (size === null) {
      window.localStorage.removeItem(key)
    } else {
      window.localStorage.setItem(key, String(size))
    }
  } catch {
    // Display preference only: losing it harms nothing.
  }
}
