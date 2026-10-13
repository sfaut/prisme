/**
 * Largeur du panneau de gauche (connexions et explorateur), réglable à la
 * souris et retenue d'une visite à l'autre.
 *
 * La largeur est gardée dans le stockage local du navigateur : c'est une
 * préférence d'affichage propre à ce poste. Si le stockage est indisponible
 * (navigation privée, stockage bloqué), la largeur par défaut s'applique.
 */

/**
 * Largeur par défaut, en pixels.
 */
export const DEFAULT_SIDER_WIDTH = 300

/**
 * Largeur minimale, en pixels : de quoi lire les boutons des connexions.
 */
export const MIN_SIDER_WIDTH = 200

/**
 * Part maximale de la fenêtre occupée par le panneau.
 */
export const MAX_SIDER_RATIO = 0.7

/**
 * Clef de la largeur dans le stockage local.
 */
const STORAGE_KEY = 'prisme.siderWidth'

/**
 * Ramène une largeur entre le minimum et la part maximale de la fenêtre.
 *
 * @param width Largeur voulue, en pixels.
 * @param windowWidth Largeur de la fenêtre, en pixels.
 */
export function clampSiderWidth(width: number, windowWidth: number): number {
  const max = Math.max(MIN_SIDER_WIDTH, Math.floor(windowWidth * MAX_SIDER_RATIO))
  return Math.min(max, Math.max(MIN_SIDER_WIDTH, Math.round(width)))
}

/**
 * Largeur retenue, ou la largeur par défaut si aucune n'est lisible.
 */
export function loadSiderWidth(): number {
  try {
    const width = Number(window.localStorage.getItem(STORAGE_KEY))
    return Number.isFinite(width) && width > 0 ? width : DEFAULT_SIDER_WIDTH
  } catch {
    return DEFAULT_SIDER_WIDTH
  }
}

/**
 * Retient la largeur ; sans effet si le stockage est indisponible.
 *
 * @param width Largeur, en pixels.
 */
export function saveSiderWidth(width: number): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, String(width))
  } catch {
    // Préférence d'affichage seulement : la perdre ne gêne rien.
  }
}
