/**
 * Réglages du thème de Naive UI.
 *
 * Les fenêtres, menus, bulles et messages s'affichent en texte plus petit que
 * le reste de l'interface : `POPUP_THEME` s'applique à toute l'application.
 */

import type { GlobalThemeOverrides } from 'naive-ui'

/**
 * Taille du texte des fenêtres, menus et bulles.
 */
const POPUP_FONT_SIZE = '12px'

/**
 * Taille du titre des fenêtres.
 */
const POPUP_TITLE_FONT_SIZE = '14px'

/**
 * Texte réduit des fenêtres (`n-modal`, carte ou dialogue), des menus
 * déroulants et des listes de choix, des bulles et des messages éphémères.
 */
export const POPUP_THEME: GlobalThemeOverrides = {
  Card: { fontSizeMedium: POPUP_FONT_SIZE, titleFontSizeMedium: POPUP_TITLE_FONT_SIZE },
  Dialog: { fontSize: POPUP_FONT_SIZE, titleFontSize: POPUP_TITLE_FONT_SIZE },
  Dropdown: { fontSizeSmall: POPUP_FONT_SIZE, fontSizeMedium: POPUP_FONT_SIZE },
  InternalSelectMenu: {
    optionFontSizeSmall: POPUP_FONT_SIZE,
    optionFontSizeMedium: POPUP_FONT_SIZE,
  },
  Popover: { fontSize: POPUP_FONT_SIZE },
  Message: { fontSize: POPUP_FONT_SIZE },
}
