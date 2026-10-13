/**
 * Choix d'un menu déroulant (`UiDropdownMenu.vue`).
 */

import type { Component } from 'vue'

/**
 * Un choix de menu : libellé, icône Lucide et action.
 */
export interface MenuItem {
  /**
   * Libellé du choix.
   */
  label: string
  /**
   * Icône Lucide, à gauche du libellé.
   */
  icon: Component
  /**
   * Ton du choix : `error` pour une action destructrice (« Supprimer »).
   */
  tone?: 'error'
  /**
   * Choix inactif.
   */
  disabled?: boolean
  /**
   * Action lancée au choix.
   */
  onSelect?: () => void
}
