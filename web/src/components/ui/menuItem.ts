/**
 * Choice of a dropdown menu (`UiDropdownMenu.vue`), group of choices under a
 * heading, and the looks shared by the menu and its submenus
 * (`UiMenuEntries.vue`).
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
   * Is it the choice made, among the exclusive choices of its group
   * (`MenuGroup`)? Set on each choice of such a group, which becomes a group
   * of radio choices: the choice made keeps its icon and is highlighted.
   */
  checked?: boolean
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
 * Choices of a menu gathered under a heading, apart from the other groups.
 */
export interface MenuGroup {
  /**
   * Heading of the group, which names it for assistive technologies.
   */
  label: string
  /**
   * Choices of the group.
   */
  items: MenuItem[]
}

/**
 * Classes of the panel that holds the choices: that of the menu, or that of
 * a submenu.
 */
export const MENU_PANEL =
  'z-50 min-w-53 rounded-xl border border-line bg-surface p-1 text-base shadow-lg'

/**
 * Classes of a choice, or of the choice that opens a submenu. The choice
 * made among radio choices is highlighted in the primary color, darker under
 * the pointer or the keyboard.
 */
export const MENU_ENTRY =
  'flex h-8 cursor-pointer items-center gap-3 rounded-lg px-3 select-none data-disabled:cursor-not-allowed data-disabled:opacity-50 data-highlighted:bg-surface-raised data-[state=checked]:bg-primary/15 data-[state=checked]:text-primary data-highlighted:data-[state=checked]:bg-primary/25'

/**
 * Classes of the heading of a group of choices.
 */
export const MENU_HEADING = 'px-3 py-1 text-sm font-medium text-fg-secondary select-none'
