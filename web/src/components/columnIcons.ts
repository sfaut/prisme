/**
 * Petits symboles placés devant le nom des colonnes dans l'explorateur.
 *
 * Dessinés en SVG, en `currentColor` : ils prennent la couleur et suivent la
 * taille du texte (1em), sans dépendre des polices installées.
 */

import { type VNode, h } from 'vue'

/**
 * Symbole SVG de 1em, à partir des tracés donnés (grille de 16 × 16).
 *
 * @param title Texte d'aide au survol, lu aussi par les lecteurs d'écran.
 * @param paths Tracés SVG (attribut `d`).
 */
function icon(title: string, paths: string[]): VNode {
  return h(
    'svg',
    {
      class: 'column-icon',
      viewBox: '0 0 16 16',
      width: '1em',
      height: '1em',
      fill: 'none',
      stroke: 'currentColor',
      'stroke-width': 1.6,
      'stroke-linecap': 'round',
      role: 'img',
    },
    [h('title', title), ...paths.map((d) => h('path', { d }))],
  )
}

/**
 * Symbole de clef, devant une colonne de clef primaire.
 */
export function keyIcon(): VNode {
  return icon('Clef primaire', [
    // Anneau de la clef, puis tige et deux dents.
    'M2.5 8a2.5 2.5 0 1 0 5 0a2.5 2.5 0 1 0 -5 0',
    'M7.5 8H14.5',
    'M11.5 8V10.5',
    'M14 8V10',
  ])
}
