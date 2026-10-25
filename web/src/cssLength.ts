/**
 * Reading a theme length (`main.css`) in pixels.
 *
 * The theme writes its lengths as multiples of `--space-unit`, in `rem`,
 * relative to the page's text size. A few APIs of dependencies want a number
 * of pixels though: the sizes of Reka UI's splitter panels, the positioning
 * of its menus (gap with their button), virtual scrolling (height of a row).
 * They read the variable here, once: the theme stays the only source of
 * these lengths. These variables are registered as lengths (`@property` in
 * `main.css`), so that the browser gives them in pixels. The list of these
 * APIs, to drop a reading once one accepts relative lengths, is in
 * `docs/DEVELOPER.md`, section "Lengths in pixels".
 */

/**
 * Value in pixels of a CSS variable of the theme, computed in `px` (registered
 * length) or written in `rem`.
 *
 * @param name Name of the variable (`--row-height`).
 * @param fallback Value returned if the variable is missing or unreadable
 *     (tests without a stylesheet, for example).
 */
export function cssPixels(name: string, fallback: number): number {
  const style = getComputedStyle(document.documentElement)
  const value = style.getPropertyValue(name).trim()
  const number = Number.parseFloat(value)
  if (Number.isNaN(number)) {
    return fallback
  }
  if (value.endsWith('rem')) {
    return number * (Number.parseFloat(style.fontSize) || 16)
  }
  return value.endsWith('px') ? number : fallback
}
