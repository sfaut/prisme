/**
 * Lecture en pixels d'une longueur du thème (`main.css`).
 *
 * Le thème écrit ses longueurs en `rem`, relatives à la taille du texte de
 * la page. Quelques outils veulent pourtant un nombre de pixels : le
 * défilement virtuel (hauteur d'une ligne), le positionnement des menus de
 * Reka UI (écart avec leur bouton). Ils lisent la variable ici : le thème
 * reste la seule source de ces longueurs.
 */

/**
 * Valeur en pixels d'une variable CSS du thème, définie en `rem` ou en `px`.
 *
 * @param name Nom de la variable (`--row-height`).
 * @param fallback Valeur rendue si la variable est absente ou illisible
 *     (tests sans feuille de style, par exemple).
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
