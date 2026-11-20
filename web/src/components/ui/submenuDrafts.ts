/**
 * What a submenu of fields (`MenuPanel`) does with the texts being typed: they apply at Enter, on
 * leaving their field, and when the submenu closes otherwise than by Escape (the pointer over
 * another choice of the menu, a click outside it), as the field may leave before it loses the
 * focus. Escape, which closes the submenu alone (`UiMenuEntries.vue`), forgets them.
 */

import { onBeforeUnmount } from 'vue'

/**
 * Texts being typed in a submenu of fields.
 */
export interface SubmenuDrafts {
  /**
   * Applies the texts being typed, unless Escape has been pressed: at Enter, on leaving a field.
   */
  confirm: () => void
  /**
   * Listens to the keys of the submenu, for Escape: to be set on its root element.
   */
  onKeydown: (event: KeyboardEvent) => void
}

/**
 * Follows the texts being typed in the submenu that calls it, applied by `apply`.
 *
 * @param apply
 *     Applies the texts being typed; changes nothing when they are those of the settings, or when
 *     they are refused.
 */
export function useSubmenuDrafts(apply: () => void): SubmenuDrafts {
  let escaped = false
  onBeforeUnmount(() => {
    if (!escaped) {
      apply()
    }
  })
  return {
    confirm: () => {
      if (!escaped) {
        apply()
      }
    },
    onKeydown: (event) => {
      if (event.key === 'Escape') {
        escaped = true
      }
    },
  }
}
