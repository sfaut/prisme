/**
 * Partage de la hauteur de la cellule SQL entre l'éditeur, en haut, et les
 * résultats, en bas : part de la hauteur donnée à l'éditeur, réglable à la
 * souris et retenue d'une visite à l'autre.
 *
 * La part est gardée dans le stockage local du navigateur : c'est une
 * préférence d'affichage propre à ce poste. Si le stockage est indisponible
 * (navigation privée, stockage bloqué), la part par défaut s'applique.
 */

/**
 * Part par défaut : le tiers du haut pour l'éditeur.
 */
export const DEFAULT_EDITOR_SPLIT = 1 / 3

/**
 * Part minimale de l'éditeur : quelques lignes restent visibles.
 */
export const MIN_EDITOR_SPLIT = 0.1

/**
 * Part maximale de l'éditeur : les résultats gardent leur place.
 */
export const MAX_EDITOR_SPLIT = 0.8

/**
 * Clef de la part dans le stockage local.
 */
const STORAGE_KEY = 'prisme.editorSplit'

/**
 * Ramène une part entre le minimum et le maximum.
 *
 * @param split Part voulue, de 0 à 1.
 */
export function clampEditorSplit(split: number): number {
  return Math.min(MAX_EDITOR_SPLIT, Math.max(MIN_EDITOR_SPLIT, split))
}

/**
 * Part retenue, ou la part par défaut si aucune n'est lisible.
 */
export function loadEditorSplit(): number {
  try {
    const split = Number(window.localStorage.getItem(STORAGE_KEY) ?? Number.NaN)
    return Number.isFinite(split) && split > 0 ? clampEditorSplit(split) : DEFAULT_EDITOR_SPLIT
  } catch {
    return DEFAULT_EDITOR_SPLIT
  }
}

/**
 * Retient la part ; sans effet si le stockage est indisponible.
 *
 * @param split Part, de 0 à 1.
 */
export function saveEditorSplit(split: number): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, String(split))
  } catch {
    // Préférence d'affichage seulement : la perdre ne gêne rien.
  }
}
