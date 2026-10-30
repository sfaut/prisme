/**
 * Choice of a dropdown menu (`UiDropdownMenu.vue`), and the looks shared by
 * the menu and its submenus (`UiMenuEntries.vue`).
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
   * Explanation of the choice, in its tooltip.
   */
  hint?: string
  /**
   * Inactive choice.
   */
  disabled?: boolean
  /**
   * Action started on choosing.
   */
  onSelect?: () => void
  /**
   * Choices of a submenu, opened by this choice instead of an action.
   */
  items?: MenuItem[]
}

/**
 * Classes of the panel that holds the choices: that of the menu, or that of
 * a submenu.
 */
export const MENU_PANEL =
  'z-50 min-w-53 rounded-xl border border-line bg-surface p-1 text-base shadow-lg'

/**
 * Classes of a choice, or of the choice that opens a submenu.
 */
export const MENU_ENTRY =
  'flex h-8 cursor-pointer items-center gap-3 rounded-lg px-3 select-none data-disabled:cursor-not-allowed data-disabled:opacity-50 data-highlighted:bg-surface-raised'
