/**
 * Choice of a dropdown menu (`UiDropdownMenu.vue`), group of choices under a
 * heading, content of a submenu that holds fields, and the looks shared by the
 * menu and its submenus (`UiMenuEntries.vue`).
 */

import type { Component } from 'vue'

import type { Tooltip } from './tooltip'

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
  hint?: string | Tooltip
  /**
   * Is it the choice made, among the exclusive choices of its group
   * (`MenuGroup`)? Set on each choice of such a group, which becomes a group
   * of radio choices: the choice made keeps its icon and is highlighted.
   */
  checked?: boolean
  /**
   * Is the box of a checkbox ticked? Set on a choice that is ticked or not, which shows its box in
   * place of its icon, and keeps the menu open when it changes.
   */
  ticked?: boolean
  /**
   * Inactive choice.
   */
  disabled?: boolean
  /**
   * Does the choice keep the menu open? A choice that brings new settings into the menu keeps it
   * open, so that they show at once.
   */
  keepOpen?: boolean
  /**
   * Action started on choosing.
   */
  onSelect?: () => void
  /**
   * Current value of the setting that the submenu of this choice sets, on the right of its label.
   */
  value?: string
  /**
   * Choices of a submenu, opened by this choice instead of an action.
   */
  items?: MenuItem[]
  /**
   * Choices of a submenu gathered in groups, instead of `items`.
   */
  groups?: MenuGroup[]
  /**
   * Content of a submenu that holds fields, instead of choices (`MenuPanel`).
   */
  panel?: MenuPanel
}

/**
 * Content of a submenu that holds fields: a component, and its props and listeners (`onChange`).
 * The submenu keeps the keys of its fields from the menu: Escape closes it alone, and Tab and
 * Shift+Tab go round its fields.
 */
export interface MenuPanel {
  /**
   * Component drawn in the submenu.
   */
  component: Component
  /**
   * Props and listeners of the component.
   */
  props: Record<string, unknown>
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
