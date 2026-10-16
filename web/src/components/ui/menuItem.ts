/**
 * Choice of a dropdown menu (`UiDropdownMenu.vue`).
 */

import type { Component } from 'vue'

/**
 * A menu choice: label, Lucide icon and action.
 */
export interface MenuItem {
  /**
   * Label of the choice.
   */
  label: string
  /**
   * Lucide icon, on the left of the label.
   */
  icon: Component
  /**
   * Tone of the choice: `error` for a destructive action ("Supprimer").
   */
  tone?: 'error'
  /**
   * Inactive choice.
   */
  disabled?: boolean
  /**
   * Action started on choosing.
   */
  onSelect?: () => void
}
